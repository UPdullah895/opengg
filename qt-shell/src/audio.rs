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
        // Every live audio-producing app (routed or not — `channel: ""` means
        // unrouted/Master), refreshed alongside channelsJson. Used to drive
        // the Mixer page's "route an app to a channel" UI.
        #[qproperty(QString, apps_json, cxx_name = "appsJson")]
        #[qproperty(bool, connected)]
        #[qproperty(bool, virtual_audio_ready, cxx_name = "virtualAudioReady")]
        #[qproperty(bool, checking_virtual_audio, cxx_name = "checkingVirtualAudio")]
        // Live per-channel VU levels as a JSON object (`{"Master": -12.3, ...}`,
        // dB, -60..0). Only populated while `vuRunning` — see startVuStream.
        #[qproperty(QString, vu_levels_json, cxx_name = "vuLevelsJson")]
        #[qproperty(bool, vu_running, cxx_name = "vuRunning")]
        // Ear Blast Protection config (`{enabled, channels, threshold,
        // target}`) — lives at the top-level `mixer.earBlast` envelope key,
        // not under `"settings"`, so it's read/written here rather than via
        // SettingsController (same reasoning as ExtensionsController's
        // modules/extensionConsents — see that file's header comment).
        // Output sinks / input sources for the per-strip device selector that
        // ChannelStrip.vue renders under each fader.
        #[qproperty(QString, output_devices_json, cxx_name = "outputDevicesJson")]
        #[qproperty(QString, input_devices_json, cxx_name = "inputDevicesJson")]
        // The device each channel is actually using right now, as a
        // `{"Master": "<node.name>", "Game": "...", ...}` object keyed by
        // channel name — lets each ChannelStrip's device ComboBox highlight
        // the real current selection instead of always defaulting to
        // whatever happens to be first in outputDevicesJson.
        #[qproperty(QString, channel_devices_json, cxx_name = "channelDevicesJson")]
        #[qproperty(QString, ear_blast_json, cxx_name = "earBlastJson")]
        // Overdrive — lets faders exceed 100% (up to 150%). Client-side UI
        // state only, in-memory, not persisted (matches MixerPage.vue's
        // `overdriveEnabled` ref). Lives here rather than as a page-local
        // property so the Home dashboard's Quick Mixer and the Mixer page's
        // ChannelStrips share one answer to "is 150% currently allowed" —
        // previously Home's slider had its own hardcoded 150 cap with no way
        // to know whether Mixer's toggle was on, so it could push a channel
        // to 150% even with Overdrive off.
        #[qproperty(bool, overdrive_enabled, cxx_name = "overdriveEnabled")]
        type AudioController = super::AudioControllerRust;

        /// Fetch the current channel list from the daemon.
        #[qinvokable]
        fn refresh(self: Pin<&mut Self>);

        /// Restore saved audio state at startup: per-channel output devices
        /// (`mixer.devices`) and app→channel links (`mixer.appRules`).
        ///
        /// `hydrate_audio_routing` had been dead code since the Tauri host
        /// that used to call it was removed — nothing in the Qt shell ever
        /// took over the job, so both the channel device map and every app
        /// link were silently dropped on each restart.
        #[qinvokable]
        fn hydrate(self: Pin<&mut Self>);

        /// Set a channel's volume (0–100) and refresh.
        #[qinvokable]
        #[cxx_name = "setVolume"]
        fn set_volume(self: Pin<&mut Self>, channel: &QString, volume: i32);

        /// Toggle a channel's mute and refresh.
        #[qinvokable]
        #[cxx_name = "setMute"]
        fn set_mute(self: Pin<&mut Self>, channel: &QString, muted: bool);

        /// Route one app's audio stream to a channel ("Master" = system
        /// default). Runs on a background thread (retries can take up to
        /// ~1.5s) and refreshes on completion, success or not.
        #[qinvokable]
        #[cxx_name = "routeApp"]
        fn route_app(self: Pin<&mut Self>, app_id: i32, channel: &QString, binary: &QString);

        /// Route an app back to the system default sink ("unroute").
        #[qinvokable]
        #[cxx_name = "unrouteApp"]
        fn unroute_app(self: Pin<&mut Self>, app_id: i32, binary: &QString);

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

        /// Reload `earBlastJson` from the `ui-settings.json` envelope.
        #[qinvokable]
        #[cxx_name = "refreshEarBlast"]
        fn refresh_ear_blast(self: Pin<&mut Self>);

        /// Set `mixer.earBlast.<key>` to `value_json` (a JSON-encoded
        /// scalar/array), save, and refresh.
        #[qinvokable]
        #[cxx_name = "setEarBlast"]
        fn set_ear_blast(self: Pin<&mut Self>, key: &QString, value_json: &QString);

        /// Re-enumerate output sinks and input sources for the strip selectors.
        #[qinvokable]
        #[cxx_name = "refreshDevices"]
        fn refresh_devices(self: Pin<&mut Self>);

        /// Bind a channel to a specific output sink / input source.
        #[qinvokable]
        #[cxx_name = "setChannelDevice"]
        fn set_channel_device(self: Pin<&mut Self>, channel: &QString, device: &QString);
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
    apps_json: QString,
    connected: bool,
    virtual_audio_ready: bool,
    checking_virtual_audio: bool,
    vu_levels_json: QString,
    vu_running: bool,
    vu_flag: Arc<AtomicBool>,
    vu_gen: Arc<AtomicU64>,
    output_devices_json: QString,
    input_devices_json: QString,
    channel_devices_json: QString,
    ear_blast_json: QString,
    overdrive_enabled: bool,
}

impl qobject::AudioController {
    /// Reload channels + apps.
    ///
    /// Both of these are blocking IPC (a D-Bus round trip, or a `pactl`
    /// subprocess on the fallback path), and this used to run them straight
    /// on the Qt thread — where a 2s repeating Timer calls it forever, and
    /// where every queued volume/mute change called it again. Each call
    /// therefore stalled the render loop, which is what made scrolling and
    /// dragging feel like they were catching on something. The I/O now runs
    /// on a worker and only the finished strings are marshalled back.
    pub fn refresh(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let channels = opengg_core::audio::get_channels();
            let apps = opengg_core::audio::get_apps();
            let _ = qt_thread.queue(move |mut controller| {
                match channels {
                    Ok(j) => {
                        controller.as_mut().set_channels_json(QString::from(&j));
                        controller.as_mut().set_connected(true);
                    }
                    Err(_) => controller.as_mut().set_connected(false),
                }
                if let Ok(j) = apps {
                    controller.as_mut().set_apps_json(QString::from(&j));
                }
            });
        });
    }

    pub fn hydrate(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            // Both touch pactl/pw-link, so they stay off the Qt thread.
            opengg_core::audio::hydrate_audio_routing();
            opengg_core::audio::apply_saved_app_rules();
            let _ = qt_thread.queue(move |controller| {
                controller.refresh();
            });
        });
    }

    pub fn route_app(self: Pin<&mut Self>, app_id: i32, channel: &QString, binary: &QString) {
        let channel = channel.to_string();
        let binary = binary.to_string();
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            match opengg_core::audio::route_app(app_id as u32, channel.clone(), binary.clone()) {
                // Remember the choice so it survives the stream ending and
                // the machine restarting — a bare `move-sink-input` lasts
                // only as long as this sink-input does.
                Ok(()) => store_app_rule(&binary, Some(&channel)),
                Err(e) => eprintln!("routeApp: {e}"),
            }
            let _ = qt_thread.queue(|mut controller| controller.as_mut().refresh());
        });
    }

    pub fn unroute_app(self: Pin<&mut Self>, app_id: i32, binary: &QString) {
        let binary = binary.to_string();
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            match opengg_core::audio::unroute_app(app_id as u32, binary.clone()) {
                // Back on Master means "no rule", not "a rule pointing at
                // Master" — otherwise re-linking later would fight it.
                Ok(()) => store_app_rule(&binary, None),
                Err(e) => eprintln!("unrouteApp: {e}"),
            }
            let _ = qt_thread.queue(|mut controller| controller.as_mut().refresh());
        });
    }

    pub fn set_volume(self: Pin<&mut Self>, channel: &QString, volume: i32) {
        // 150 (not 100) so the Mixer page's Overdrive toggle can push faders
        // past unity gain, matching ChannelStrip.vue's `maxVol` (100 normally,
        // 150 with overdrive) — the QML slider's own `to:` already enforces
        // the lower 100 cap when overdrive is off, this just avoids silently
        // clamping the legitimate 100-150 range back down.
        let vol = volume.clamp(0, 150) as u32;
        let channel = channel.to_string();
        // Was a direct synchronous call (pactl subprocess spawn or a D-Bus
        // round trip, then a second round trip for refresh()) on the Qt/UI
        // thread. A fader drag calls this on every `onPositionChanged`, so
        // every pixel of movement blocked the render loop on two blocking
        // I/O calls — the fader visibly froze/stuttered while dragging.
        // Backgrounded to match the established route_app/setChannelDevice
        // pattern.
        //
        // Deliberately does *not* refresh afterwards: a drag fires this
        // continuously, and every refresh rewrites channelsJson and so
        // re-runs every binding downstream of it. ChannelStrip already shows
        // the dragged value optimistically, and MixerPage's 2s poll
        // reconciles with the daemon shortly after.
        std::thread::spawn(move || {
            if let Err(e) = opengg_core::audio::set_volume(channel, vol) {
                eprintln!("setVolume: {e}");
            }
        });
    }

    pub fn set_mute(self: Pin<&mut Self>, channel: &QString, muted: bool) {
        let channel = channel.to_string();
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            if let Err(e) = opengg_core::audio::set_mute(channel, muted) {
                eprintln!("setMute: {e}");
            }
            let _ = qt_thread.queue(|mut c| c.as_mut().refresh());
        });
    }

    /// Reload the output/input device lists plus which device each channel
    /// is currently using.
    ///
    /// `current_channel_device` alone runs `pw-link -l` up to twice per
    /// channel (5 channels), on top of two `pactl list` calls for the
    /// device lists — enough blocking subprocess work that, like every
    /// other refresh in this file, it has to run off the Qt thread rather
    /// than stall the render loop.
    pub fn refresh_devices(self: Pin<&mut Self>) {
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            let sinks = opengg_core::audio::list_audio_sinks_friendly().unwrap_or_default();
            let sources = opengg_core::audio::list_mic_input_sources().unwrap_or_default();
            let channel_devices = opengg_core::audio::get_channel_devices_json();
            let _ = qt_thread.queue(move |mut c| {
                c.as_mut().set_output_devices_json(QString::from(
                    &serde_json::to_string(&sinks).unwrap_or_else(|_| "[]".into()),
                ));
                c.as_mut().set_input_devices_json(QString::from(
                    &serde_json::to_string(&sources).unwrap_or_else(|_| "[]".into()),
                ));
                c.as_mut()
                    .set_channel_devices_json(QString::from(&channel_devices));
            });
        });
    }

    pub fn set_channel_device(self: Pin<&mut Self>, channel: &QString, device: &QString) {
        let channel = channel.to_string();
        let device = device.to_string();
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            if let Err(e) = opengg_core::audio::set_channel_device(channel, device) {
                eprintln!("setChannelDevice: {e}");
            }
            let _ = qt_thread.queue(|mut c| {
                c.as_mut().refresh();
                c.as_mut().refresh_devices();
            });
        });
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

            // Ear Blast Protection: checked against every live level on
            // every iteration (see opengg_core::ear_blast::check), same as
            // the Tauri original's check_ear_blast call from its VU emitter
            // task. Config is re-read from disk periodically (not every
            // iteration — that's 30 JSON-file reads/sec for no benefit) so
            // toggling it in Settings takes effect within ~1s without
            // resetting the per-channel duck/restore state on every read.
            let mut eb_state = opengg_core::ear_blast::EarBlastState::default();
            let (eb_en, eb_ch, eb_th, eb_ta) = opengg_core::ear_blast::load_config();
            eb_state.set_config(eb_en, eb_ch, eb_th, eb_ta);
            let mut eb_reload_counter = 0u32;

            while running.load(Ordering::Relaxed) && gen.load(Ordering::Relaxed) == my_gen {
                while let Ok((ch, db)) = rx.try_recv() {
                    levels.insert(ch, db);
                }

                eb_reload_counter += 1;
                if eb_reload_counter >= 30 {
                    eb_reload_counter = 0;
                    let (en, ch, th, ta) = opengg_core::ear_blast::load_config();
                    eb_state.set_config(en, ch, th, ta);
                }
                if eb_state.enabled {
                    for (ch, db) in &levels {
                        opengg_core::ear_blast::check(&mut eb_state, ch, *db);
                    }
                }

                let json = serde_json::to_string(&levels).unwrap_or_else(|_| "{}".into());
                let _ = qt_thread.queue(move |mut controller| {
                    controller.as_mut().set_vu_levels_json(QString::from(&json));
                });
                std::thread::sleep(std::time::Duration::from_millis(32));
            }
            // Force-release any channel still ducked when the loop stops
            // (page closed, app quitting, or a newer start_vu_stream call
            // superseding this thread) — otherwise a duck that was active
            // right when metering stopped never gets its release check and
            // the channel's real PipeWire volume is left stuck at the Ear
            // Blast target indefinitely. See ear_blast::release_all's
            // header comment for the user-visible symptom this fixes.
            opengg_core::ear_blast::release_all(&mut eb_state);
            let _ = qt_thread.queue(move |mut controller| {
                controller.as_mut().set_vu_running(false);
            });
        });
    }

    pub fn stop_vu_stream(mut self: Pin<&mut Self>) {
        self.vu_flag.store(false, Ordering::Relaxed);
        self.as_mut().set_vu_running(false);
    }

    pub fn refresh_ear_blast(mut self: Pin<&mut Self>) {
        let v = load_settings_envelope();
        let eb = v
            .get("mixer")
            .and_then(|m| m.get("earBlast"))
            .cloned()
            .unwrap_or_else(default_ear_blast);
        let json = serde_json::to_string(&eb).unwrap_or_else(|_| "{}".into());
        self.as_mut().set_ear_blast_json(QString::from(&json));
    }

    pub fn set_ear_blast(mut self: Pin<&mut Self>, key: &QString, value_json: &QString) {
        let mut v = load_settings_envelope();
        if !v["mixer"].is_object() {
            v["mixer"] = serde_json::json!({});
        }
        if !v["mixer"]["earBlast"].is_object() {
            v["mixer"]["earBlast"] = default_ear_blast();
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&value_json.to_string()).unwrap_or(serde_json::Value::Null);
        v["mixer"]["earBlast"][key.to_string()] = parsed;
        if let Ok(s) = serde_json::to_string(&v) {
            let _ = opengg_core::settings::save_ui_settings(&s);
        }
        self.as_mut().refresh_ear_blast();
    }
}

/// Insert or clear one `mixer.appRules` entry, preserving the rest of
/// ui-settings.json. Read-modify-write through the same envelope helper
/// `set_ear_blast` uses, so this stays consistent with how every other
/// mixer preference in this file is persisted.
///
/// Keyed by process binary. A stream with no binary is skipped rather than
/// keyed by its sink-input id — that id is regenerated every time the app
/// starts, so a rule under it could never match again.
fn store_app_rule(binary: &str, channel: Option<&str>) {
    let key = binary.trim();
    if key.is_empty() {
        return;
    }
    let mut v = load_settings_envelope();
    if !v["mixer"].is_object() {
        v["mixer"] = serde_json::json!({});
    }
    if !v["mixer"]["appRules"].is_object() {
        v["mixer"]["appRules"] = serde_json::json!({});
    }
    if let Some(rules) = v["mixer"]["appRules"].as_object_mut() {
        // Drop any case variant first so one app can never hold two rules.
        let lower = key.to_lowercase();
        rules.retain(|k, _| k.trim().to_lowercase() != lower);
        if let Some(ch) = channel {
            rules.insert(key.to_string(), serde_json::json!(ch));
        }
    }
    if let Ok(out) = serde_json::to_string(&v) {
        let _ = opengg_core::settings::save_ui_settings(&out);
    }
}

fn load_settings_envelope() -> serde_json::Value {
    let raw = opengg_core::settings::load_ui_settings().unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}))
}

fn default_ear_blast() -> serde_json::Value {
    serde_json::json!({ "enabled": false, "channels": ["Game"], "threshold": 85, "target": 60 })
}
