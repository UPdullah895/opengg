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
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

#[derive(Default)]
pub struct AudioControllerRust {
    channels_json: QString,
    connected: bool,
    virtual_audio_ready: bool,
    checking_virtual_audio: bool,
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
}
