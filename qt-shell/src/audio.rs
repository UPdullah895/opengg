//! AudioController — a QML singleton that talks to the running `openggd` daemon
//! over D-Bus (`org.opengg.Daemon.Audio`) for live audio-channel state.
//!
//! Boundary note (plan §2.2/§2.3): the daemon owns the audio business logic; this
//! is only a thin D-Bus *client*. It uses zbus's blocking API directly for now;
//! the client will move into `opengg-core` during the core-extraction task
//! (TODO(core)). No audio logic is implemented here.

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
    }
}

use core::pin::Pin;
use cxx_qt_lib::QString;

const DEST: &str = "org.opengg.Daemon";
const PATH: &str = "/org/opengg/Daemon/Audio";
const IFACE: &str = "org.opengg.Daemon.Audio";

pub struct AudioControllerRust {
    channels_json: QString,
    connected: bool,
    conn: Option<zbus::blocking::Connection>,
}

impl Default for AudioControllerRust {
    fn default() -> Self {
        Self {
            channels_json: QString::default(),
            connected: false,
            conn: zbus::blocking::Connection::session().ok(),
        }
    }
}

impl qobject::AudioController {
    pub fn refresh(mut self: Pin<&mut Self>) {
        let json = self
            .conn
            .as_ref()
            .and_then(|c| fetch_channels(c).ok());
        match json {
            Some(j) => {
                self.as_mut().set_channels_json(QString::from(&j));
                self.as_mut().set_connected(true);
            }
            None => self.as_mut().set_connected(false),
        }
    }

    pub fn set_volume(mut self: Pin<&mut Self>, channel: &QString, volume: i32) {
        let ch = channel.to_string();
        let vol = volume.clamp(0, 100) as u32;
        if let Some(c) = self.conn.as_ref() {
            let _ = c.call_method(Some(DEST), PATH, Some(IFACE), "SetVolume", &(ch.as_str(), vol));
        }
        self.as_mut().refresh();
    }

    pub fn set_mute(mut self: Pin<&mut Self>, channel: &QString, muted: bool) {
        let ch = channel.to_string();
        if let Some(c) = self.conn.as_ref() {
            let _ = c.call_method(Some(DEST), PATH, Some(IFACE), "SetMute", &(ch.as_str(), muted));
        }
        self.as_mut().refresh();
    }
}

fn fetch_channels(c: &zbus::blocking::Connection) -> zbus::Result<String> {
    let reply = c.call_method(Some(DEST), PATH, Some(IFACE), "GetChannels", &())?;
    let json: String = reply.body().deserialize()?;
    Ok(json)
}
