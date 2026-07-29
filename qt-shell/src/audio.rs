//! AudioController — a QML singleton exposing live audio-channel state from the
//! running `openggd` daemon (`org.opengg.Daemon.Audio`).
//!
//! Boundary note (plan §2.2/§2.3): the daemon D-Bus client + pactl fallbacks live
//! in `opengg_core::audio` (blocking API); this controller is pure presentation
//! glue over `opengg_core::audio::{get_channels, set_volume, set_mute}`. qt-shell
//! no longer talks to zbus directly.

#[cxx_qt::bridge]
pub mod qobject {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // Raw channels JSON from the daemon (parsed in QML with JSON.parse);
        // a QAbstractListModel is the Phase 3 upgrade (plan §2.2).
        #[qproperty(QString, channels_json, cxx_name = "channelsJson")]
        #[qproperty(bool, connected)]
        #[qproperty(bool, virtual_audio_ready, cxx_name = "virtualAudioReady")]
        #[qproperty(bool, checking_virtual_audio, cxx_name = "checkingVirtualAudio")]
        // Live per-channel VU levels as a JSON object (`{"Master": -12.3, ...}`,
        // dB, -60..0). Only populated while `vuRunning` — see startVuStream.
        #[qproperty(QString, vu_levels_json, cxx_name = "vuLevelsJson")]
        #[qproperty(bool, vu_running, cxx_name = "vuRunning")]
        type AudioController = super::AudioControllerRust;

        /// Fetch the current channel list from the daemon.
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);

        /// Set a channel's volume (0–100) and refresh.
        #[qinvokable]
        #[cxx_name = "setVolume"]
        fn set_volume(self: Pin<&mut Self>, channel: &QString, volume: i32);

        /// Toggle a channel's mute and refresh.
        #[qinvokable]
        #[cxx_name = "setMute"]
        fn set_mute(self: Pin<&mut Self>, channel: &QString, muted: bool);

        /// Re-check whether OpenGG's virtual sinks/sources are present.
        #[qinvokable]
        #[cxx_name = "refreshVirtualAudioStatus"]
        fn refresh_virtual_audio_status(self: Pin<&mut Self>);

        /// Create OpenGG's virtual audio sinks (idempotent).
        #[qinvokable]
        #[cxx_name = "createVirtualAudio"]
        fn create_virtual_audio(self: Pin<&mut Self>);

        /// Tear down OpenGG's virtual audio sinks and restore OS defaults.
        #[qinvokable]
        #[cxx_name = "removeVirtualAudio"]
        fn remove_virtual_audio(self: Pin<&mut Self>);

        /// Start streaming live VU levels for Master/Game/Chat/Media/Aux/Mic
        /// (~30 fps) into `vuLevelsJson`. Safe to call repeatedly — restarting
        /// tears down any previous stream via the generation counter before
        /// spawning fresh readers. Callers MUST pair this with stopVuStream
        /// when the Mixer page is no longer visible: each channel spawns a
        /// real `pw-cat` subprocess, so leaving this running in the
        /// background wastes CPU for no on-screen benefit.
        #[qinvokable]
        #[cxx_name = "startVuStream"]
        fn start_vu_stream(self: Pin<&mut Self>);

        /// Stop the VU stream; reader threads exit cooperatively within one
        /// read period.
        #[qinvokable]
        #[cxx_name = "stopVuStream"]
        fn stop_vu_stream(self: Pin<&mut Self>);
    }

    impl cxx_qt::Threading for AudioController {}
}

use core::pin::Pin;
use cxx_qt::Threading;
use cxx_qt_lib::QString;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

#[derive(Default)]
pub struct AudioControllerRust {
    channels_json: QString,
    connected: bool,
    virtual_audio_ready: bool,
    checking_virtual_audio: bool,
    vu_levels_json: QString,
    vu_running: bool,
    vu_flag: Arc<AtomicBool>,
    vu_gen: Arc<AtomicU64>,
}

impl qobject::AudioController {
    pub fn refresh(mut self: Pin<&mut Self>) {
        match opengg_core::audio::get_channels() {
            Ok(j) => {
                self.as_mut().set_channels_json(QString::from(&j));
                self.as_mut().set_connected(true);
            }
            Err(_) => self.as_mut().set_connected(false),
        }
    }

    pub fn set_volume(mut self: Pin<&mut Self>, channel: &QString, volume: i32) {
        let vol = volume.clamp(0, 100) as u32;
        let _ = opengg_core::audio::set_volume(channel.to_string(), vol);
        self.as_mut().refresh();
    }

    pub fn set_mute(mut self: Pin<&mut Self>, channel: &QString, muted: bool) {
        let _ = opengg_core::audio::set_mute(channel.to_string(), muted);
        self.as_mut().refresh();
    }

    pub fn refresh_virtual_audio_status(mut self: Pin<&mut Self>) {
        self.as_mut().set_checking_virtual_audio(true);
        let ready = opengg_core::audio::check_virtual_audio_status().unwrap_or(false);
        self.as_mut().set_virtual_audio_ready(ready);
        self.as_mut().set_checking_virtual_audio(false);
    }

    pub fn create_virtual_audio(mut self: Pin<&mut Self>) {
        let _ = opengg_core::audio::create_virtual_audio();
        self.as_mut().refresh_virtual_audio_status();
    }

    pub fn remove_virtual_audio(mut self: Pin<&mut Self>) {
        let _ = opengg_core::audio::remove_virtual_audio();
        self.as_mut().refresh_virtual_audio_status();
    }

    pub fn start_vu_stream(mut self: Pin<&mut Self>) {
        // Generation-counter dedup (mirrors the Tauri implementation): stop
        // any previous stream's threads, bump the generation so they notice
        // and exit even if `vu_flag` flips back to true before they check.
        self.vu_flag.store(false, Ordering::Relaxed);
        let my_gen = self.vu_gen.fetch_add(1, Ordering::SeqCst) + 1;
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.vu_flag.store(true, Ordering::Relaxed);
        self.as_mut().set_vu_running(true);

        let running = self.vu_flag.clone();
        let gen = self.vu_gen.clone();
        let qt_thread = self.qt_thread();

        std::thread::spawn(move || {
            let mic = opengg_core::vu::resolve_mic_source();
            let known = opengg_core::vu::known_sources();
            let targets = opengg_core::vu::channel_targets(mic);

            let (tx, rx) = mpsc::channel::<(String, f32)>();
            for (name, target, is_source) in targets {
                if target.is_empty() || !known.contains(&target) {
                    let _ = tx.send((name.to_string(), -60.0));
                    continue;
                }
                opengg_core::vu::spawn_channel_reader(
                    name.to_string(),
                    target,
                    is_source,
                    tx.clone(),
                    running.clone(),
                    gen.clone(),
                    my_gen,
                );
            }
            drop(tx); // only reader threads' clones keep `rx` alive now

            let mut levels: HashMap<String, f32> = HashMap::with_capacity(6);
            while running.load(Ordering::Relaxed) && gen.load(Ordering::Relaxed) == my_gen {
                while let Ok((ch, db)) = rx.try_recv() {
                    levels.insert(ch, db);
                }
                let json = serde_json::to_string(&levels).unwrap_or_else(|_| "{}".into());
                let _ = qt_thread.queue(move |mut controller| {
                    controller.as_mut().set_vu_levels_json(QString::from(&json));
                });
                std::thread::sleep(std::time::Duration::from_millis(32));
            }
            let _ = qt_thread.queue(move |mut controller| {
                controller.as_mut().set_vu_running(false);
            });
        });
    }

    pub fn stop_vu_stream(mut self: Pin<&mut Self>) {
        self.vu_flag.store(false, Ordering::Relaxed);
        self.as_mut().set_vu_running(false);
    }
}
