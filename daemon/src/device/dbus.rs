//! D-Bus interface: org.opengg.Daemon.Device

use zbus::interface;

pub struct DeviceInterface {
    // Will hold ratbagd proxy, OpenRGB client, ProfileManager
}

impl DeviceInterface {
    pub fn new() -> Self {
        Self {}
    }
}

#[interface(name = "org.opengg.Daemon.Device")]
impl DeviceInterface {
    /// List all detected gaming devices (mice, keyboards, RGB controllers).
    async fn get_devices(&self) -> String {
        // TODO: combine ratbag + openrgb device lists
        "[]".into()
    }

    /// Set DPI on a specific device.
    async fn set_dpi(&self, device_id: &str, dpi: u32) -> zbus::fdo::Result<()> {
        tracing::info!("SetDPI: {device_id} → {dpi}");
        Ok(())
    }

    /// Set RGB color and mode on a zone.
    async fn set_rgb(&self, zone: &str, color: &str, mode: &str) -> zbus::fdo::Result<()> {
        tracing::info!("SetRGB: {zone} → {color} ({mode})");
        Ok(())
    }

    /// Switch to a named profile.
    async fn set_profile(&self, profile_name: &str) -> zbus::fdo::Result<()> {
        tracing::info!("SetProfile: {profile_name}");
        Ok(())
    }

    /// List available profiles.
    async fn get_profiles(&self) -> String {
        "[]".into()
    }
}
