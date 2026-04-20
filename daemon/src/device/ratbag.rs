//! ratbagd D-Bus integration for mouse management (org.freedesktop.ratbag1).

use anyhow::{Context, Result};
use zbus::{proxy, zvariant, Connection};

use super::types::{DeviceInfo, DeviceType};

// ── D-Bus proxy definitions ──────────────────────────────────────────────────

#[proxy(
    interface = "org.freedesktop.ratbag1.Manager",
    default_service = "org.freedesktop.ratbag1",
    default_path = "/org/freedesktop/ratbag1"
)]
trait Manager {
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.ratbag1.Device",
    default_service = "org.freedesktop.ratbag1"
)]
trait Device {
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn model(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn profiles(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    fn commit(&self) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.freedesktop.ratbag1.Profile",
    default_service = "org.freedesktop.ratbag1"
)]
trait Profile {
    #[zbus(property)]
    fn resolutions(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    #[zbus(property)]
    fn is_active(&self) -> zbus::Result<bool>;
}

#[proxy(
    interface = "org.freedesktop.ratbag1.Resolution",
    default_service = "org.freedesktop.ratbag1"
)]
trait Resolution {
    #[zbus(property)]
    fn resolution(&self) -> zbus::Result<(u32, u32)>;

    #[zbus(property)]
    fn report_rate(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn report_rates(&self) -> zbus::Result<Vec<u32>>;

    #[zbus(property)]
    fn resolutions(&self) -> zbus::Result<Vec<(u32, u32)>>;

    #[zbus(property)]
    fn is_active(&self) -> zbus::Result<bool>;
}

// ── RatbagManager ────────────────────────────────────────────────────────────

pub struct RatbagManager {
    conn: Connection,
}

impl RatbagManager {
    pub async fn new() -> Result<Self> {
        let conn = Connection::system()
            .await
            .context("failed to connect to D-Bus system bus")?;
        Ok(Self { conn })
    }

    pub async fn list_devices(&self) -> Vec<DeviceInfo> {
        let manager = {
            let mut last_err = None;
            let mut result = None;
            for _ in 0..3 {
                match ManagerProxy::new(&self.conn).await {
                    Ok(m) => { result = Some(m); break; }
                    Err(e) => {
                        last_err = Some(e);
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }
                }
            }
            match result {
                Some(m) => m,
                None => {
                    tracing::warn!("ratbagd not available after 3 attempts: {}", last_err.unwrap());
                    return vec![];
                }
            }
        };

        let paths = match manager.devices().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("ratbagd devices property failed: {e}");
                return vec![];
            }
        };

        let mut devices = Vec::new();
        for path in paths {
            if let Some(info) = self.read_device(&path).await {
                devices.push(info);
            }
        }
        devices
    }

    async fn read_device(&self, path: &zbus::zvariant::OwnedObjectPath) -> Option<DeviceInfo> {
        let dev = DeviceProxy::builder(&self.conn)
            .path(path.as_ref())
            .ok()?
            .build()
            .await
            .ok()?;

        let name = dev.name().await.unwrap_or_default();
        let model_str = dev.model().await.unwrap_or_default();

        // Parse VID/PID from model string "usb:VVVV:PPPP:00"
        let (vid, pid) = parse_model_id(&model_str);

        // Get DPI and polling rate from active profile's active resolution
        let (dpi, polling_rate, dpi_options) = self.read_active_resolution(&dev).await;

        // Use last segment of object path as sysname
        let sysname = path
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or("unknown")
            .to_string();

        Some(DeviceInfo {
            id: format!("ratbag:{sysname}"),
            name: name.clone(),
            model: model_str,
            device_type: DeviceType::Mouse,
            vid,
            pid,
            dpi,
            polling_rate,
            dpi_options,
            battery_level: None,
            battery_charging: None,
            sidetone: None,
            chatmix: None,
            capabilities: None,
            eq_presets: None,
            eq_meta: None,
        })
    }

    async fn read_active_resolution(
        &self,
        dev: &DeviceProxy<'_>,
    ) -> (Option<u32>, Option<u32>, Option<Vec<u32>>) {
        let profiles = match dev.profiles().await {
            Ok(p) => p,
            Err(_) => return (None, None, None),
        };

        for profile_path in &profiles {
            let profile = match ProfileProxy::builder(&self.conn)
                .path(profile_path.as_ref())
                .ok()
                .and(None::<ProfileProxy>)
            {
                Some(p) => p,
                None => {
                    let Ok(p) = ProfileProxy::builder(&self.conn)
                        .path(profile_path.as_ref())
                        .unwrap()
                        .build()
                        .await
                    else {
                        continue;
                    };
                    p
                }
            };

            if !profile.is_active().await.unwrap_or(false) {
                continue;
            }

            let res_paths = match profile.resolutions().await {
                Ok(r) => r,
                Err(_) => continue,
            };

            for res_path in &res_paths {
                let Ok(res) = ResolutionProxy::builder(&self.conn)
                    .path(res_path.as_ref())
                    .unwrap()
                    .build()
                    .await
                else {
                    continue;
                };

                if !res.is_active().await.unwrap_or(false) {
                    continue;
                }

                let dpi = res.resolution().await.ok().map(|(x, _)| x);
                let rate = res.report_rate().await.ok();
                let dpi_list = res
                    .resolutions()
                    .await
                    .ok()
                    .map(|v| v.into_iter().map(|(x, _)| x).collect());

                return (dpi, rate, dpi_list);
            }
        }
        (None, None, None)
    }

    pub async fn set_dpi(&self, sysname: &str, dpi: u32) -> Result<()> {
        let path = format!("/org/freedesktop/ratbag1/device/{sysname}");
        let dev = DeviceProxy::builder(&self.conn)
            .path(path.as_str())
            .context("invalid path")?
            .build()
            .await
            .context("DeviceProxy build failed")?;

        let profiles = dev.profiles().await.context("get profiles")?;
        for profile_path in &profiles {
            let profile = ProfileProxy::builder(&self.conn)
                .path(profile_path.as_ref())
                .unwrap()
                .build()
                .await?;
            if !profile.is_active().await.unwrap_or(false) {
                continue;
            }
            let res_paths = profile.resolutions().await?;
            for res_path in &res_paths {
                let res = ResolutionProxy::builder(&self.conn)
                    .path(res_path.as_ref())
                    .unwrap()
                    .build()
                    .await?;
                if res.is_active().await.unwrap_or(false) {
                    res.inner().set_property("Resolution", zvariant::Value::from((dpi, dpi))).await?;
                    dev.commit().await?;
                    tracing::info!("Set DPI to {dpi} on {sysname}");
                    return Ok(());
                }
            }
        }
        anyhow::bail!("active resolution not found for {sysname}")
    }

    pub async fn set_polling_rate(&self, sysname: &str, rate: u32) -> Result<()> {
        let path = format!("/org/freedesktop/ratbag1/device/{sysname}");
        let dev = DeviceProxy::builder(&self.conn)
            .path(path.as_str())
            .context("invalid path")?
            .build()
            .await
            .context("DeviceProxy build failed")?;

        let profiles = dev.profiles().await.context("get profiles")?;
        for profile_path in &profiles {
            let profile = ProfileProxy::builder(&self.conn)
                .path(profile_path.as_ref())
                .unwrap()
                .build()
                .await?;
            if !profile.is_active().await.unwrap_or(false) {
                continue;
            }
            let res_paths = profile.resolutions().await?;
            for res_path in &res_paths {
                let res = ResolutionProxy::builder(&self.conn)
                    .path(res_path.as_ref())
                    .unwrap()
                    .build()
                    .await?;
                if res.is_active().await.unwrap_or(false) {
                    res.inner().set_property("ReportRate", zvariant::Value::from(rate)).await?;
                    dev.commit().await?;
                    tracing::info!("Set polling rate to {rate}Hz on {sysname}");
                    return Ok(());
                }
            }
        }
        anyhow::bail!("active resolution not found for {sysname}")
    }
}

fn parse_model_id(model: &str) -> (u16, u16) {
    // Format: "usb:VVVV:PPPP:00" or empty
    let parts: Vec<&str> = model.split(':').collect();
    let vid = parts
        .get(1)
        .and_then(|s| u16::from_str_radix(s, 16).ok())
        .unwrap_or(0);
    let pid = parts
        .get(2)
        .and_then(|s| u16::from_str_radix(s, 16).ok())
        .unwrap_or(0);
    (vid, pid)
}
