//! ClipAudioMixer — QML front for [`crate::mixer_pipeline`], which plays every
//! audio track of a clip at once with independent per-track gain.
//!
//! Split of responsibility while a clip is open:
//!   * Qt Multimedia's `MediaPlayer`/`VideoOutput` owns the PICTURE, with its
//!     own audio output muted.
//!   * This pipeline owns the SOUND — all tracks, mixed.
//!
//! The two run on independent clocks, so QML resyncs the mixer to the video's
//! position whenever they drift (see `positionMs` / `seek`).
//!
//! Ported from the qt6 video PoC's `AudioMixerController`, generalised from
//! its fixed game/chat/mic properties to an arbitrary track count.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Number of audio tracks being mixed; 0 when the clip has none (or
        /// the pipeline failed, in which case QML falls back to Qt's own
        /// single-track audio). -1 while discovery is still running.
        #[qproperty(i32, track_count, cxx_name = "trackCount")]
        /// True once a pipeline is live and owns this clip's audio.
        #[qproperty(bool, active)]
        type ClipAudioMixer = super::ClipAudioMixerRust;

        /// Tear down any existing pipeline and build one for `source`
        /// (accepts a plain path or a file:// URL). Sets `trackCount` and
        /// returns an ownership token for `release`.
        ///
        /// This is a singleton with two callers (the clip player and the
        /// editor page), and navigating between them hides one while showing
        /// the other. An unconditional `unload` from the one being hidden
        /// would kill the audio the other just started — which is exactly how
        /// the editor ended up silent. Callers keep their token and release
        /// with it; a stale release is ignored.
        #[qinvokable]
        fn load(self: Pin<&mut Self>, source: &QString) -> i64;

        /// Drop the pipeline IF `token` still owns it, else do nothing.
        #[qinvokable]
        fn release(self: Pin<&mut Self>, token: i64);

        /// Unconditional teardown — app shutdown only.
        #[qinvokable]
        fn unload(self: Pin<&mut Self>);

        #[qinvokable]
        fn play(self: Pin<&mut Self>);

        #[qinvokable]
        fn pause(self: Pin<&mut Self>);

        #[qinvokable]
        fn seek(self: Pin<&mut Self>, position_ms: i64);

        /// Where the audio pipeline actually is, in ms (-1 if unknown).
        #[qinvokable]
        #[cxx_name = "positionMs"]
        fn position_ms(self: &Self) -> i64;

        /// Per-track gain, 0.0..=1.0. Muting a track is `setTrackVolume(i, 0)`.
        #[qinvokable]
        #[cxx_name = "setTrackVolume"]
        fn set_track_volume(self: Pin<&mut Self>, index: i32, value: f64);

        /// Gain on the summed mix — the player's master volume slider.
        #[qinvokable]
        #[cxx_name = "setMasterVolume"]
        fn set_master_volume(self: Pin<&mut Self>, value: f64);

        /// Match the video's playback rate so the two stay aligned.
        #[qinvokable]
        #[cxx_name = "setRate"]
        fn set_rate(self: Pin<&mut Self>, rate: f64);
    }

    impl cxx_qt::Threading for ClipAudioMixer {}
}

use crate::mixer_pipeline::{build_mixer_pipeline, MixerPipeline};
use core::pin::Pin;
use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;
use std::time::Duration;

pub struct ClipAudioMixerRust {
    track_count: i32,
    active: bool,
    master_volume: f64,
    pipeline: Option<MixerPipeline>,
    /// Monotonic ownership token; see `load`.
    token: i64,
}

impl Default for ClipAudioMixerRust {
    fn default() -> Self {
        Self {
            track_count: 0,
            active: false,
            master_volume: 1.0,
            pipeline: None,
            token: 0,
        }
    }
}

impl qobject::ClipAudioMixer {
    pub fn load(mut self: Pin<&mut Self>, source: &QString) -> i64 {
        // Tear the old pipeline down completely first — leaving a stale one
        // holding the audio device was a real bug in the PoC.
        self.as_mut().unload();

        let token = self.token.wrapping_add(1);
        self.as_mut().rust_mut().token = token;

        let source = source.to_string();
        let path = source
            .strip_prefix("file://")
            .unwrap_or(&source)
            .to_string();
        if path.is_empty() {
            return token;
        }

        let pipeline = match build_mixer_pipeline(&path) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("clip audio mixer: pipeline build failed for {path}: {e}");
                self.as_mut().set_track_count(0);
                return token;
            }
        };
        pipeline.set_master_volume(self.master_volume);
        pipeline.start_discovery();

        // Discovery on a local file is near-instant; the short block keeps the
        // "who owns the audio" decision synchronous, so QML never briefly
        // plays Qt's single track before the mixer takes over.
        match pipeline.wait_for_discovery(Duration::from_secs(5)) {
            // Nothing to mix (or discovery stalled) — drop it and let Qt
            // Multimedia handle the audio as it did before.
            Some(0) | None => {
                pipeline.shutdown();
                self.as_mut().set_track_count(0);
                self.as_mut().set_active(false);
            }
            Some(count) => {
                self.as_mut().rust_mut().pipeline = Some(pipeline);
                self.as_mut().set_track_count(count);
                self.as_mut().set_active(true);
            }
        }
        token
    }

    pub fn release(mut self: Pin<&mut Self>, token: i64) {
        if token != self.token {
            return; // superseded by another owner — not ours to tear down
        }
        self.as_mut().unload();
    }

    pub fn unload(mut self: Pin<&mut Self>) {
        if let Some(old) = self.as_mut().rust_mut().pipeline.take() {
            old.shutdown();
        }
        self.as_mut().set_active(false);
        self.as_mut().set_track_count(0);
    }

    pub fn play(self: Pin<&mut Self>) {
        if let Some(p) = self.rust().pipeline.as_ref() {
            p.play();
        }
    }

    pub fn pause(self: Pin<&mut Self>) {
        if let Some(p) = self.rust().pipeline.as_ref() {
            p.pause();
        }
    }

    pub fn seek(self: Pin<&mut Self>, position_ms: i64) {
        if let Some(p) = self.rust().pipeline.as_ref() {
            p.seek(position_ms);
        }
    }

    pub fn position_ms(&self) -> i64 {
        self.pipeline.as_ref().map_or(-1, |p| p.position_ms())
    }

    pub fn set_track_volume(self: Pin<&mut Self>, index: i32, value: f64) {
        if index < 0 {
            return;
        }
        if let Some(p) = self.rust().pipeline.as_ref() {
            p.set_track_volume(index as usize, value.clamp(0.0, 1.0));
        }
    }

    pub fn set_master_volume(mut self: Pin<&mut Self>, value: f64) {
        let v = value.clamp(0.0, 1.0);
        self.as_mut().rust_mut().master_volume = v;
        if let Some(p) = self.rust().pipeline.as_ref() {
            p.set_master_volume(v);
        }
    }

    pub fn set_rate(self: Pin<&mut Self>, rate: f64) {
        if let Some(p) = self.rust().pipeline.as_ref() {
            p.set_rate(rate);
        }
    }
}
