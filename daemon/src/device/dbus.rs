//! D-Bus interface: org.opengg.Daemon.Device

use std::sync::Arc;
use tokio::sync::Mutex;
use zbus::interface;

use super::{
    headset::HeadsetManager,
    ratbag::RatbagManager,
};

pub struct DeviceInterface {
    ratbag: Arc<Mutex<Option<RatbagManager>>>,
}

impl DeviceInterface {
    pub async fn new() -> Self {
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
        }
    }
}

/// Parse a headset device ID of the form "headset:{vid}:{pid}" and return (vid, pid).
/// Returns None if the string does not match the expected format.
fn parse_headset_id(device_id: &str) -> Option<(u16, u16)> {
    let body = device_id.strip_prefix("headset:")?;
    let (vid_str, pid_str) = body.split_once(':')?;
    let vid = u16::from_str_radix(vid_str, 16).ok()?;
    let pid = u16::from_str_radix(pid_str, 16).ok()?;
    Some((vid, pid))
}

#[interface(name = "org.opengg.Daemon.Device")]
impl DeviceInterface {
    async fn get_devices(&self) -> String {
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

    async fn set_dpi(&self, device_id: &str, dpi: u32) -> zbus::fdo::Result<()> {
        let sysname = strip_prefix(device_id, "ratbag:")
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("not a mouse device id".into()))?;

        self.ratbag
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| zbus::fdo::Error::ServiceUnknown("ratbagd not available".into()))?
            .set_dpi(sysname, dpi)
            .await
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    async fn set_polling_rate(&self, device_id: &str, rate: u32) -> zbus::fdo::Result<()> {
        let sysname = strip_prefix(device_id, "ratbag:")
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("not a mouse device id".into()))?;

        self.ratbag
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| zbus::fdo::Error::ServiceUnknown("ratbagd not available".into()))?
            .set_polling_rate(sysname, rate)
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

    async fn set_rgb(&self, zone: &str, color: &str, mode: &str) -> zbus::fdo::Result<()> {
        tracing::info!("SetRGB: {zone} → {color} ({mode}) [not yet implemented]");
        Ok(())
    }

    async fn set_profile(&self, profile_name: &str) -> zbus::fdo::Result<()> {
        tracing::info!("SetProfile: {profile_name} [not yet implemented]");
        Ok(())
    }

    async fn get_profiles(&self) -> String {
        "[]".into()
    }
}

fn strip_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.strip_prefix(prefix)
}
