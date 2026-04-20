//! HeadsetControl integration — spawns the `headsetcontrol` CLI for device enumeration and control.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

use super::types::{DeviceInfo, DeviceType, EqMeta};

// headsetcontrol --output json top-level response
#[derive(Debug, Deserialize)]
struct HscOutput {
    #[serde(default)]
    devices: Vec<HscDevice>,
}

#[derive(Debug, Deserialize)]
struct HscDevice {
    #[serde(default)]
    product: String,
    #[serde(default)]
    id_vendor: Option<String>,
    #[serde(default)]
    id_product: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    battery: Option<HscBattery>,
    #[serde(default)]
    chatmix: Option<u32>,
    #[serde(default)]
    equalizer: Option<HscEqMeta>,
    #[serde(default)]
    equalizer_presets: Option<HashMap<String, Vec<f32>>>,
}

#[derive(Debug, Deserialize)]
struct HscBattery {
    level: Option<i32>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HscEqMeta {
    bands: u32,
    baseline: f32,
    step: f32,
    min: f32,
    max: f32,
}

fn parse_hex_u16(s: &str) -> u16 {
    u16::from_str_radix(s.trim_start_matches("0x"), 16).unwrap_or(0)
}

fn normalize_cap(cap: &str) -> String {
    cap.strip_prefix("CAP_").unwrap_or(cap).to_lowercase()
}

pub struct HeadsetManager;

impl HeadsetManager {
    pub fn list_devices() -> Vec<DeviceInfo> {
        let path_env = std::env::var("PATH")
            .unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".to_string());

        let output = match Command::new("headsetcontrol")
            .env("PATH", path_env)
            .args(["--output", "json"])
            .output()
        {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("headsetcontrol spawn failed: {e}");
                return vec![];
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("headsetcontrol exited {:?}: {stderr}", output.status.code());
            return vec![];
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let parsed: HscOutput = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("headsetcontrol JSON parse error: {e}");
                return vec![];
            }
        };

        parsed
            .devices
            .into_iter()
            .enumerate()
            .map(|(idx, d)| {
                let vid = parse_hex_u16(d.id_vendor.as_deref().unwrap_or(""));
                let pid = parse_hex_u16(d.id_product.as_deref().unwrap_or(""));

                let battery_level = d.battery.as_ref().and_then(|b| b.level);
                let battery_charging = d
                    .battery
                    .as_ref()
                    .and_then(|b| b.status.as_deref())
                    .map(|s| s == "BATTERY_CHARGING");

                let capabilities: Vec<String> =
                    d.capabilities.iter().map(|c| normalize_cap(c)).collect();

                let eq_meta = d.equalizer.map(|e| EqMeta {
                    bands: e.bands,
                    min: e.min,
                    max: e.max,
                    step: e.step,
                });

                DeviceInfo {
                    id: format!("headset:{idx}"),
                    name: d.product.clone(),
                    model: d.product,
                    device_type: DeviceType::Headset,
                    vid,
                    pid,
                    dpi: None,
                    polling_rate: None,
                    dpi_options: None,
                    battery_level,
                    battery_charging,
                    sidetone: None,
                    chatmix: d.chatmix,
                    capabilities: Some(capabilities),
                    eq_presets: d.equalizer_presets,
                    eq_meta,
                }
            })
            .collect()
    }

    pub fn set_sidetone(device_idx: usize, level: u32) -> Result<()> {
        let idx_str = device_idx.to_string();
        let output = Command::new("headsetcontrol")
            .args(["--device", &idx_str, "-s", &level.to_string()])
            .output()
            .context("headsetcontrol spawn failed")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("headsetcontrol failed (exit {:?}): {}", output.status.code(), stderr.trim());
        }
        Ok(())
    }

    pub fn set_chatmix(device_idx: usize, level: u32) -> Result<()> {
        let idx_str = device_idx.to_string();
        let output = Command::new("headsetcontrol")
            .args(["--device", &idx_str, "--chatmix", &level.to_string()])
            .output()
            .context("headsetcontrol spawn failed")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("headsetcontrol failed (exit {:?}): {}", output.status.code(), stderr.trim());
        }
        Ok(())
    }

    pub fn set_inactive_time(device_idx: usize, minutes: u32) -> Result<()> {
        let idx_str = device_idx.to_string();
        let output = Command::new("headsetcontrol")
            .args(["--device", &idx_str, "-i", &minutes.to_string()])
            .output()
            .context("headsetcontrol spawn failed")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("headsetcontrol failed (exit {:?}): {}", output.status.code(), stderr.trim());
        }
        Ok(())
    }

    pub fn set_microphone_volume(device_idx: usize, level: u32) -> Result<()> {
        let idx_str = device_idx.to_string();
        let output = Command::new("headsetcontrol")
            .args(["--device", &idx_str, "--microphone-volume", &level.to_string()])
            .output()
            .context("headsetcontrol spawn failed")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("headsetcontrol failed (exit {:?}): {}", output.status.code(), stderr.trim());
        }
        Ok(())
    }

    pub fn set_mic_mute_led_brightness(device_idx: usize, brightness: u32) -> Result<()> {
        let idx_str = device_idx.to_string();
        let output = Command::new("headsetcontrol")
            .args(["--device", &idx_str, "--microphone-mute-led-brightness", &brightness.to_string()])
            .output()
            .context("headsetcontrol spawn failed")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("headsetcontrol failed (exit {:?}): {}", output.status.code(), stderr.trim());
        }
        Ok(())
    }

    pub fn set_volume_limiter(device_idx: usize, enabled: bool) -> Result<()> {
        let idx_str = device_idx.to_string();
        let output = Command::new("headsetcontrol")
            .args(["--device", &idx_str, "--volume-limiter", if enabled { "1" } else { "0" }])
            .output()
            .context("headsetcontrol spawn failed")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("headsetcontrol failed (exit {:?}): {}", output.status.code(), stderr.trim());
        }
        Ok(())
    }

    pub fn set_bt_when_powered_on(device_idx: usize, enabled: bool) -> Result<()> {
        let idx_str = device_idx.to_string();
        let output = Command::new("headsetcontrol")
            .args(["--device", &idx_str, "--bt-when-powered-on", if enabled { "1" } else { "0" }])
            .output()
            .context("headsetcontrol spawn failed")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("headsetcontrol failed (exit {:?}): {}", output.status.code(), stderr.trim());
        }
        Ok(())
    }

    pub fn set_bt_call_volume(device_idx: usize, level: u32) -> Result<()> {
        let idx_str = device_idx.to_string();
        let output = Command::new("headsetcontrol")
            .args(["--device", &idx_str, "--bt-call-volume", &level.to_string()])
            .output()
            .context("headsetcontrol spawn failed")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("headsetcontrol failed (exit {:?}): {}", output.status.code(), stderr.trim());
        }
        Ok(())
    }

    pub fn set_eq_preset(device_idx: usize, preset_idx: u32) -> Result<()> {
        let idx_str = device_idx.to_string();
        let output = Command::new("headsetcontrol")
            .args(["--device", &idx_str, "-p", &preset_idx.to_string()])
            .output()
            .context("headsetcontrol spawn failed")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("headsetcontrol failed (exit {:?}): {}", output.status.code(), stderr.trim());
        }
        Ok(())
    }

    pub fn set_eq_curve(device_idx: usize, bands: &[f32]) -> Result<()> {
        let idx_str = device_idx.to_string();
        let csv = bands
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let output = Command::new("headsetcontrol")
            .args(["--device", &idx_str, "-e", &csv])
            .output()
            .context("headsetcontrol spawn failed")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("headsetcontrol failed (exit {:?}): {}", output.status.code(), stderr.trim());
        }
        Ok(())
    }
}
