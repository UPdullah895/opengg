//! D-Bus interface: org.opengg.Daemon.Device

use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::interface;

use super::{
    headset::HeadsetManager,
    profiles::ProfileManager,
    ratbag::RatbagManager,
};

pub struct DeviceInterface {
    ratbag: Arc<Mutex<Option<RatbagManager>>>,
    /// Where game profiles live. Read on demand rather than cached: the
    /// directory is tiny and a user may drop a profile in while we run.
    profiles_dir: std::path::PathBuf,
}

impl DeviceInterface {
    pub async fn new(profiles_dir: std::path::PathBuf) -> Self {
        let ratbag = match RatbagManager::new().await {
            Ok(m) => {
                tracing::info!("ratbagd D-Bus connection established");
                Some(m)
            }
            Err(e) => {
                tracing::warn!("ratbagd unavailable: {e}");
                None
            }
        };
        Self {
            ratbag: Arc::new(Mutex::new(ratbag)),
            profiles_dir,
        }
    }

    /// Get all connected devices as JSON string (public method for trait impls).
    pub async fn get_devices_json(&self) -> String {
        let mut devices = vec![];

        // Mice via ratbagd
        if let Some(ref mgr) = *self.ratbag.lock().await {
            let mut mice = mgr.list_devices().await;
            devices.append(&mut mice);
        }

        // Headsets via headsetcontrol CLI
        let mut headsets = HeadsetManager::list_devices();
        devices.append(&mut headsets);

        serde_json::to_string(&devices).unwrap_or_else(|_| "[]".into())
    }
}

/// Parse a `"{prefix}{vid:04x}:{pid:04x}"` device ID (e.g.
/// `"headset:046d:0a5c"`). Delegates to `types::parse_device_id` so the
/// parser can never drift from the builder again — see the note there.
fn parse_vid_pid_id(device_id: &str, prefix: &str) -> Option<(u16, u16)> {
    super::types::parse_device_id(device_id, prefix)
}

/// Parse a headset device ID of the form "headset:{vid}:{pid}" and return (vid, pid).
fn parse_headset_id(device_id: &str) -> Option<(u16, u16)> {
    parse_vid_pid_id(device_id, "headset:")
}

#[interface(name = "org.opengg.Daemon.Device")]
impl DeviceInterface {
    async fn get_devices(&self) -> String {
        self.get_devices_json().await
    }

    async fn set_dpi(&self, device_id: &str, dpi: u32) -> zbus::fdo::Result<()> {
        let id_body = device_id
            .strip_prefix("ratbag:")
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("not a mouse device id".into()))?;

        self.ratbag
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| zbus::fdo::Error::ServiceUnknown("ratbagd not available".into()))?
            .set_dpi(id_body, dpi)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn set_polling_rate(&self, device_id: &str, rate: u32) -> zbus::fdo::Result<()> {
        let id_body = device_id
            .strip_prefix("ratbag:")
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("not a mouse device id".into()))?;

        self.ratbag
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| zbus::fdo::Error::ServiceUnknown("ratbagd not available".into()))?
            .set_polling_rate(id_body, rate)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Every button on a mouse's active profile: index, which action types
    /// it supports, and its current action — JSON array of `ButtonMapping`
    /// (Devices Phase 5). Returns `"[]"` (not an error) if ratbagd is
    /// unavailable or the id isn't a mouse, matching `get_devices`'s
    /// always-succeeds shape rather than `set_dpi`'s fail-loud shape, since
    /// this is a read the UI polls opportunistically.
    async fn get_button_mappings(&self, device_id: &str) -> String {
        let Some(id_body) = device_id.strip_prefix("ratbag:") else {
            return "[]".into();
        };
        let guard = self.ratbag.lock().await;
        let Some(mgr) = guard.as_ref() else {
            return "[]".into();
        };
        match mgr.get_button_mappings(id_body).await {
            Ok(mappings) => serde_json::to_string(&mappings).unwrap_or_else(|_| "[]".into()),
            Err(e) => {
                tracing::warn!("GetButtonMappings failed for {device_id}: {e}");
                "[]".into()
            }
        }
    }

    /// Set one button's action. `action_json` is a `ButtonAction` (Devices
    /// Phase 5) — e.g. `{"type":"key","name":"e"}` or `{"type":"none"}`.
    /// Applies immediately, no separate "save" step, matching `SetDpi`.
    async fn set_button_action(
        &self,
        device_id: &str,
        button_index: u32,
        action_json: &str,
    ) -> zbus::fdo::Result<()> {
        let id_body = device_id
            .strip_prefix("ratbag:")
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("not a mouse device id".into()))?;
        let action: super::ratbag::ButtonAction = serde_json::from_str(action_json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("bad action JSON: {e}")))?;

        self.ratbag
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| zbus::fdo::Error::ServiceUnknown("ratbagd not available".into()))?
            .set_button_action(id_body, button_index, &action)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Static catalog of every action `SetButtonAction` will accept for the
    /// `special`/`key` types (device-independent — no D-Bus round trip to
    /// ratbagd needed). `button`/`none` aren't listed here since they need
    /// no picker beyond "which of this device's own buttons/disable".
    async fn get_button_action_catalog(&self) -> String {
        super::ratbag::button_action_catalog_json()
    }

    /// Manual identity-merge override (see `ratbag::RatbagManager::merge_devices`):
    /// confirm that two mouse device ids are the same physical device even
    /// though the automatic name heuristic didn't (or wouldn't) merge them.
    /// Takes effect on the next `GetDevices` call.
    async fn merge_mouse_devices(&self, device_id_a: &str, device_id_b: &str) -> zbus::fdo::Result<()> {
        let id_a = device_id_a
            .strip_prefix("ratbag:")
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("not a mouse device id".into()))?;
        let id_b = device_id_b
            .strip_prefix("ratbag:")
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("not a mouse device id".into()))?;

        self.ratbag
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| zbus::fdo::Error::ServiceUnknown("ratbagd not available".into()))?
            .merge_devices(id_a, id_b)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Manual identity-split override: undo an (auto or manual) merge and
    /// remember that these links must never be auto-merged again. Takes
    /// effect on the next `GetDevices` call.
    async fn split_mouse_device(&self, device_id: &str) -> zbus::fdo::Result<()> {
        let id_body = device_id
            .strip_prefix("ratbag:")
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("not a mouse device id".into()))?;

        self.ratbag
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| zbus::fdo::Error::ServiceUnknown("ratbagd not available".into()))?
            .split_device(id_body)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn set_sidetone(&self, device_id: &str, level: u32) -> zbus::fdo::Result<()> {
        let (vid, pid) = parse_headset_id(device_id)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("invalid headset device id".into()))?;
        HeadsetManager::set_sidetone(vid, pid, level)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn set_chatmix(&self, device_id: &str, level: u32) -> zbus::fdo::Result<()> {
        let (vid, pid) = parse_headset_id(device_id)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("invalid headset device id".into()))?;
        HeadsetManager::set_chatmix(vid, pid, level)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn set_inactive_time(&self, device_id: &str, minutes: u32) -> zbus::fdo::Result<()> {
        let (vid, pid) = parse_headset_id(device_id)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("invalid headset device id".into()))?;
        HeadsetManager::set_inactive_time(vid, pid, minutes)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn set_microphone_volume(&self, device_id: &str, level: u32) -> zbus::fdo::Result<()> {
        let (vid, pid) = parse_headset_id(device_id)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("invalid headset device id".into()))?;
        HeadsetManager::set_microphone_volume(vid, pid, level)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn set_mic_mute_led_brightness(&self, device_id: &str, brightness: u32) -> zbus::fdo::Result<()> {
        let (vid, pid) = parse_headset_id(device_id)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("invalid headset device id".into()))?;
        HeadsetManager::set_mic_mute_led_brightness(vid, pid, brightness)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn set_volume_limiter(&self, device_id: &str, enabled: bool) -> zbus::fdo::Result<()> {
        let (vid, pid) = parse_headset_id(device_id)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("invalid headset device id".into()))?;
        HeadsetManager::set_volume_limiter(vid, pid, enabled)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn set_bt_when_powered_on(&self, device_id: &str, enabled: bool) -> zbus::fdo::Result<()> {
        let (vid, pid) = parse_headset_id(device_id)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("invalid headset device id".into()))?;
        HeadsetManager::set_bt_when_powered_on(vid, pid, enabled)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn set_bt_call_volume(&self, device_id: &str, level: u32) -> zbus::fdo::Result<()> {
        let (vid, pid) = parse_headset_id(device_id)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("invalid headset device id".into()))?;
        HeadsetManager::set_bt_call_volume(vid, pid, level)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn set_eq_preset(&self, device_id: &str, preset_idx: u32) -> zbus::fdo::Result<()> {
        let (vid, pid) = parse_headset_id(device_id)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("invalid headset device id".into()))?;
        HeadsetManager::set_eq_preset(vid, pid, preset_idx)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn set_eq_curve(&self, device_id: &str, bands_json: &str) -> zbus::fdo::Result<()> {
        let (vid, pid) = parse_headset_id(device_id)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("invalid headset device id".into()))?;
        let bands: Vec<f32> = serde_json::from_str(bands_json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        HeadsetManager::set_eq_curve(vid, pid, &bands)
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Returns JSON {level: i32, charging: bool} for the given headset device.
    async fn get_headset_battery(&self, device_id: &str) -> String {
        let (vid, pid) = match parse_headset_id(device_id) {
            Some(ids) => ids,
            None => return r#"{"level":-1,"charging":false}"#.into(),
        };

        let devices = HeadsetManager::list_devices();
        match devices.into_iter().find(|d| d.vid == vid && d.pid == pid) {
            Some(d) => {
                let level = d.battery_level.unwrap_or(-1);
                let charging = d.battery_charging.unwrap_or(false);
                serde_json::json!({"level": level, "charging": charging}).to_string()
            }
            None => r#"{"level":-1,"charging":false}"#.into(),
        }
    }

    /// Unified RGB is not implemented, and says so.
    ///
    /// This used to log "not yet implemented" and return `Ok(())`, so every
    /// caller was told the colour had been applied. `openrgb.rs` does hold a
    /// working SDK client, but it can set an LED or a mode and cannot yet
    /// enumerate a controller's LED count (that needs
    /// RequestControllerData, which is unparsed), so there is nothing
    /// honest to call here. Failing loudly is strictly better than a
    /// success that did nothing.
    async fn set_rgb(&self, zone: &str, color: &str, mode: &str) -> zbus::fdo::Result<()> {
        tracing::warn!("SetRgb({zone}, {color}, {mode}): not implemented");
        Err(zbus::fdo::Error::NotSupported(
            "unified RGB is not implemented yet: the OpenRGB client cannot enumerate \
             controller LED counts (RequestControllerData is unparsed)"
                .into(),
        ))
    }

    /// Applying a profile is not implemented, and says so — same reasoning
    /// as `set_rgb`. `GetProfiles` below is real, so a client can still
    /// list what exists.
    async fn set_profile(&self, profile_name: &str) -> zbus::fdo::Result<()> {
        tracing::warn!("SetProfile({profile_name}): not implemented");
        Err(zbus::fdo::Error::NotSupported(
            "applying a game profile is not implemented yet (it would need to drive \
             audio routing, mouse DPI and RGB together)"
                .into(),
        ))
    }

    /// The profiles actually on disk. Previously hardcoded to `"[]"`, which
    /// made the profiles directory look permanently empty no matter what
    /// the user put in it.
    async fn get_profiles(&self) -> String {
        match ProfileManager::new(&self.profiles_dir) {
            Ok(mgr) => serde_json::to_string(mgr.list()).unwrap_or_else(|_| "[]".into()),
            Err(e) => {
                tracing::warn!("GetProfiles: {e}");
                "[]".into()
            }
        }
    }
}
