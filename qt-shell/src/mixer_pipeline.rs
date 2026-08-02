//! Multi-track clip audio, as a GStreamer pipeline.
//!
//! Qt Multimedia decodes exactly one audio stream at a time — it can switch
//! tracks (`activeAudioTrack`) but never mix them. An OpenGG capture carries a
//! separate track per channel (Game / Chat / Mic), so previewing one meant the
//! rest were simply silent. This pipeline decodes every audio stream and sums
//! them, with an independent `volume` element per track:
//!
//! ```text
//!   filesrc -> decodebin =*=> queue -> audioconvert -> audioresample -> volume -> audiomixer -> volume -> autoaudiosink
//! ```
//!
//! Video pads are sunk to a fakesink: the picture still comes from Qt
//! Multimedia's `VideoOutput`, this pipeline only ever owns the sound.
//!
//! Ported from the qt6 video PoC (`src/mixer_pipeline.rs`), generalised from
//! its fixed 3-track array to any track count.

use gstreamer as gst;
use gstreamer::glib;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// Qt facts cxx-qt cannot express — see `src/cpp/video_bridge.cpp`.
extern "C" {
    fn opengg_has_gl_window_system() -> bool;
    fn opengg_find_video_item(object_name: *const std::os::raw::c_char) -> *mut std::ffi::c_void;
}

/// objectName the QML video surface must carry for the sink to find it.
pub const VIDEO_ITEM_OBJECT_NAME: &str = "clipVideoItem";

/// Whether this process can render GL video at all. False under
/// `QT_QPA_PLATFORM=offscreen`, where qml6glsink fails the whole pipeline
/// with "Could not initialize window system" — the screenshot harness runs
/// there, so the caller falls back to a poster frame instead.
pub fn gl_video_available() -> bool {
    unsafe { opengg_has_gl_window_system() }
}

/// Point a qml6glsink at the QML video surface.
///
/// The sink's `widget` property is a bare G_TYPE_POINTER, so this sets it
/// through glib directly rather than dragging GStreamer headers into the C++
/// shim — the shim only locates the QQuickItem and hands back its address.
fn attach_video_item(sink: &gst::Element) -> bool {
    let name = std::ffi::CString::new(VIDEO_ITEM_OBJECT_NAME).expect("static name");
    let item = unsafe { opengg_find_video_item(name.as_ptr()) };
    if item.is_null() {
        return false;
    }
    unsafe {
        let mut value = glib::Value::from_type(glib::Type::POINTER);
        glib::gobject_ffi::g_value_set_pointer(
            value.as_ptr() as *mut glib::gobject_ffi::GValue,
            item,
        );
        sink.set_property_from_value("widget", &value);
    }
    true
}

#[derive(Default)]
struct TrackState {
    /// Volumes requested so far, by track index. Held separately from the
    /// elements because QML can set a volume before decodebin has discovered
    /// that track's pad; the value is applied when the element appears.
    requested: Vec<f64>,
    /// Per-track `volume` elements, in decodebin discovery order.
    elements: Vec<gst::Element>,
}

impl TrackState {
    fn requested_for(&self, index: usize) -> f64 {
        self.requested.get(index).copied().unwrap_or(1.0)
    }
    fn set_requested(&mut self, index: usize, value: f64) {
        if self.requested.len() <= index {
            self.requested.resize(index + 1, 1.0);
        }
        self.requested[index] = value;
    }
}

pub struct MixerPipeline {
    pipeline: gst::Pipeline,
    master_volume: gst::Element,
    tracks: Arc<Mutex<TrackState>>,
    discovery: Arc<(Mutex<Option<i32>>, Condvar)>,
    /// Set once a GL video branch is actually linked. Written from
    /// decodebin's pad-added callback, read by QML through the controller.
    video_linked: Arc<AtomicBool>,
}

/// Sink a video pad we are not rendering. A video pad MUST be linked or the
/// pipeline errors out "not-linked", so unwanted frames go to a fakesink
/// rather than being left dangling.
fn drop_video_pad(pipeline: &gst::Pipeline, pad: &gst::Pad) {
    let Ok(fakesink) = gst::ElementFactory::make("fakesink").build() else {
        return;
    };
    if pipeline.add(&fakesink).is_ok() {
        let _ = fakesink.sync_state_with_parent();
        if let Some(sinkpad) = fakesink.static_pad("sink") {
            let _ = pad.link(&sinkpad);
        }
    }
}

/// Build the GL video chain and add it to the pipeline while it is still at
/// NULL, returning its entry element.
///
/// Built UP FRONT rather than inside decodebin's pad-added callback, which is
/// the mistake that cost a debugging round: adding it dynamically to an
/// already-PAUSED pipeline forces `sync_state_with_parent`, and qml6glsink
/// cannot change state until the Qt scene graph has given it a GL context —
/// so it failed with "Failed to sync state with parent". At NULL the whole
/// pipeline transitions together and the sink gets its context in step.
fn build_video_chain(pipeline: &gst::Pipeline) -> Result<gst::Element, gst::glib::BoolError> {
    let upload = gst::ElementFactory::make("glupload").build()?;
    let convert = gst::ElementFactory::make("glcolorconvert").build()?;
    let sink = gst::ElementFactory::make("qml6glsink").build()?;
    // The sink reads its widget when it starts; a null widget fails the
    // state change, so this has to happen before the pipeline moves.
    if !attach_video_item(&sink) {
        return Err(gst::glib::bool_error!("no QML video surface in the scene"));
    }
    pipeline.add_many([&upload, &convert, &sink])?;
    gst::Element::link_many([&upload, &convert, &sink])?;
    Ok(upload)
}

/// `want_video` renders the picture into the QML surface named
/// [`VIDEO_ITEM_OBJECT_NAME`]; false discards it (audio-only, or offscreen).
pub fn build_mixer_pipeline(path: &str, want_video: bool) -> Result<MixerPipeline, gst::glib::BoolError> {
    // Idempotent; safe to call on every clip load.
    gst::init().map_err(|e| gst::glib::bool_error!("gstreamer init failed: {e}"))?;

    let pipeline = gst::Pipeline::new();
    let filesrc = gst::ElementFactory::make("filesrc")
        .property("location", path)
        .build()?;
    let decodebin = gst::ElementFactory::make("decodebin").build()?;
    let mixer = gst::ElementFactory::make("audiomixer").name("mixer").build()?;
    let master_volume = gst::ElementFactory::make("volume")
        .name("master_volume")
        .build()?;
    let convert = gst::ElementFactory::make("audioconvert").build()?;
    let sink = gst::ElementFactory::make("autoaudiosink").build()?;

    pipeline.add_many([
        &filesrc,
        &decodebin,
        &mixer,
        &master_volume,
        &convert,
        &sink,
    ])?;
    filesrc.link(&decodebin)?;
    gst::Element::link_many([&mixer, &master_volume, &convert, &sink])?;

    let tracks = Arc::new(Mutex::new(TrackState::default()));
    let discovery: Arc<(Mutex<Option<i32>>, Condvar)> =
        Arc::new((Mutex::new(None), Condvar::new()));

    let video_linked = Arc::new(AtomicBool::new(false));

    // Built while the pipeline is still at NULL — see build_video_chain.
    let video_chain = if want_video {
        match build_video_chain(&pipeline) {
            Ok(upload) => Some(upload),
            Err(e) => {
                eprintln!("mixer: no GL video branch ({e}) — audio only");
                None
            }
        }
    } else {
        None
    };

    let pipeline_weak = pipeline.downgrade();
    let mixer_weak = mixer.downgrade();
    let tracks_cb = Arc::clone(&tracks);
    let video_linked_cb = Arc::clone(&video_linked);
    let video_chain_cb = video_chain.clone();
    decodebin.connect_pad_added(move |_, pad| {
        let (Some(pipeline), Some(mixer)) = (pipeline_weak.upgrade(), mixer_weak.upgrade()) else {
            return;
        };
        let is_audio = pad
            .current_caps()
            .and_then(|c| c.structure(0).map(|s| s.name().starts_with("audio/")))
            .unwrap_or(false);
        if !is_audio {
            // Link to the pre-built GL chain if we have one and it is still
            // free; otherwise discard the frames.
            let linked = video_chain_cb.as_ref().is_some_and(|upload| {
                upload
                    .static_pad("sink")
                    .filter(|sinkpad| !sinkpad.is_linked())
                    .is_some_and(|sinkpad| pad.link(&sinkpad).is_ok())
            });
            if linked {
                video_linked_cb.store(true, Ordering::Relaxed);
            } else {
                drop_video_pad(&pipeline, pad);
            }
            return;
        }

        let mut st = tracks_cb.lock().unwrap();
        let idx = st.elements.len();

        let build = || -> Result<(gst::Element, gst::Element, gst::Element, gst::Element), gst::glib::BoolError> {
            Ok((
                gst::ElementFactory::make("queue").build()?,
                gst::ElementFactory::make("audioconvert").build()?,
                gst::ElementFactory::make("audioresample").build()?,
                gst::ElementFactory::make("volume")
                    .name(format!("track{idx}_volume"))
                    .build()?,
            ))
        };
        let Ok((queue, convert, resample, volume)) = build() else {
            eprintln!("mixer: failed to build chain for audio track {idx}");
            return;
        };

        volume.set_property("volume", st.requested_for(idx));
        if pipeline
            .add_many([&queue, &convert, &resample, &volume])
            .is_err()
            || gst::Element::link_many([&queue, &convert, &resample, &volume]).is_err()
        {
            eprintln!("mixer: failed to link chain for audio track {idx}");
            return;
        }
        let Some(mixer_pad) = mixer.request_pad_simple("sink_%u") else {
            eprintln!("mixer: audiomixer refused a sink pad for track {idx}");
            return;
        };
        if let Some(srcpad) = volume.static_pad("src") {
            let _ = srcpad.link(&mixer_pad);
        }
        for e in [&queue, &convert, &resample, &volume] {
            let _ = e.sync_state_with_parent();
        }
        if let Some(sinkpad) = queue.static_pad("sink") {
            let _ = pad.link(&sinkpad);
        }
        st.elements.push(volume);
    });

    let tracks_cb = Arc::clone(&tracks);
    let discovery_cb = Arc::clone(&discovery);
    decodebin.connect_no_more_pads(move |_| {
        let count = tracks_cb.lock().unwrap().elements.len() as i32;
        let (lock, cvar) = &*discovery_cb;
        *lock.lock().unwrap() = Some(count);
        cvar.notify_all();
    });

    Ok(MixerPipeline {
        pipeline,
        master_volume,
        tracks,
        discovery,
        video_linked,
    })
}

impl MixerPipeline {
    /// Kick decodebin into parsing the file so pads get discovered.
    pub fn start_discovery(&self) {
        let _ = self.pipeline.set_state(gst::State::Paused);
    }

    /// Block until `no-more-pads` fires; `None` on timeout.
    pub fn wait_for_discovery(&self, timeout: Duration) -> Option<i32> {
        let (lock, cvar) = &*self.discovery;
        let deadline = Instant::now() + timeout;
        let mut done = lock.lock().unwrap();
        while done.is_none() {
            let remaining = deadline.checked_duration_since(Instant::now())?;
            let (guard, res) = cvar.wait_timeout(done, remaining).unwrap();
            done = guard;
            if res.timed_out() && done.is_none() {
                return None;
            }
        }
        *done
    }

    /// Per-track gain, 0.0..=1.0 (mute is just 0.0). Values set before the
    /// track's element exists are remembered and applied on discovery.
    pub fn set_track_volume(&self, index: usize, value: f64) {
        let mut st = self.tracks.lock().unwrap();
        st.set_requested(index, value);
        if let Some(el) = st.elements.get(index) {
            el.set_property("volume", value);
        }
    }

    /// Gain applied to the summed mix.
    pub fn set_master_volume(&self, value: f64) {
        self.master_volume.set_property("volume", value);
    }

    pub fn play(&self) {
        let _ = self.pipeline.set_state(gst::State::Playing);
    }

    pub fn pause(&self) {
        let _ = self.pipeline.set_state(gst::State::Paused);
    }

    pub fn seek(&self, position_ms: i64) {
        let _ = self.pipeline.seek_simple(
            gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
            gst::ClockTime::MSECOND * position_ms.max(0) as u64,
        );
    }

    /// Current playback position in ms, or -1 if the pipeline can't answer.
    /// Used to detect drift against Qt Multimedia's independent video clock.
    pub fn position_ms(&self) -> i64 {
        self.pipeline
            .query_position::<gst::ClockTime>()
            .map(|t| (t.nseconds() / 1_000_000) as i64)
            .unwrap_or(-1)
    }

    pub fn set_rate(&self, rate: f64) {
        let pos = self.pipeline.query_position::<gst::ClockTime>();
        if let Some(pos) = pos {
            let _ = self.pipeline.seek(
                rate,
                gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
                gst::SeekType::Set,
                pos,
                gst::SeekType::None,
                gst::ClockTime::ZERO,
            );
        }
    }

    pub fn shutdown(&self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }

    /// First ERROR on the bus within `timeout`, if any. Used by the tests to
    /// assert the graph actually runs rather than just builds.
    pub fn pop_error(&self, timeout: Duration) -> Option<String> {
        let bus = self.pipeline.bus()?;
        let msg = bus.timed_pop_filtered(
            gst::ClockTime::from_nseconds(timeout.as_nanos() as u64),
            &[gst::MessageType::Error],
        )?;
        Some(format!("{msg:?}"))
    }

    /// Whether the picture is being rendered into the QML surface. False for
    /// audio-only clips, and whenever the GL branch could not be built.
    pub fn has_video(&self) -> bool {
        self.video_linked.load(Ordering::Relaxed)
    }

    /// How many per-track volume elements were built. Test/diagnostic hook.
    pub fn track_count(&self) -> usize {
        self.tracks.lock().unwrap().elements.len()
    }
}

impl Drop for MixerPipeline {
    fn drop(&mut self) {
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Discovery over a real multi-track capture. Skipped (rather than failed)
    /// when no such clip is present, so the suite still runs on a machine
    /// without the fixture.
    #[test]
    fn discovers_every_audio_track() {
        let Some(clip) = find_multitrack_clip() else {
            eprintln!("skipping: no multi-track clip available");
            return;
        };
        let p = build_mixer_pipeline(&clip, false).expect("pipeline build");
        p.set_master_volume(0.0); // keep the test run silent
        p.start_discovery();
        let count = p.wait_for_discovery(Duration::from_secs(10));
        p.shutdown();
        assert!(
            count.is_some_and(|c| c >= 2),
            "expected a multi-track clip to yield 2+ audio tracks, got {count:?}"
        );
    }

    /// The point of the whole module: every track must be mixed at once, and
    /// the graph must actually reach PLAYING rather than merely link up.
    #[test]
    fn plays_all_tracks_together_without_errors() {
        let Some(clip) = find_multitrack_clip() else {
            eprintln!("skipping: no multi-track clip available");
            return;
        };
        let p = build_mixer_pipeline(&clip, false).expect("pipeline build");
        p.set_master_volume(0.0); // silent test run
        p.start_discovery();
        let discovered = p.wait_for_discovery(Duration::from_secs(10)).unwrap_or(0);
        assert!(discovered >= 2);

        // Every discovered track gets its own gain element — that is what makes
        // per-track volume/mute possible instead of a single switchable stream.
        assert_eq!(p.track_count(), discovered as usize);
        for i in 0..discovered as usize {
            p.set_track_volume(i, 0.5);
        }

        p.play();
        let err = p.pop_error(Duration::from_secs(3));
        p.shutdown();
        assert!(err.is_none(), "pipeline errored while playing: {err:?}");
    }

    fn find_multitrack_clip() -> Option<String> {
        let dir = dirs_home()?.join("Videos/OpenGG");
        let entries = std::fs::read_dir(dir).ok()?;
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_some_and(|x| x == "mp4") {
                let p = path.to_string_lossy().to_string();
                if opengg_core::media::analyze_media_sync(&p)
                    .is_ok_and(|i| i.audio_streams >= 2)
                {
                    return Some(p);
                }
            }
        }
        None
    }

    fn dirs_home() -> Option<std::path::PathBuf> {
        std::env::var_os("HOME").map(std::path::PathBuf::from)
    }
}
