//! ratbagd Integration — uses `ratbagctl` CLI for reliable device management.

use anyhow::{Context, Result};
use serde::Serialize;
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct RatbagDevice {
    pub name: String,
    pub model: String,
}

/// List all devices via ratbagctl.
pub fn list_devices() -> Result<Vec<RatbagDevice>> {
    let output = Command::new("ratbagctl")
        .args(["list"])
        .output()
        .context("ratbagctl not found — install libratbag")?;

    let text = String::from_utf8_lossy(&output.stdout);
    let devices: Vec<RatbagDevice> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            RatbagDevice {
                name: parts.get(1).unwrap_or(&"Unknown").trim().to_string(),
                model: parts.first().unwrap_or(&"").trim().to_string(),
            }
        })
        .collect();
    Ok(devices)
}

pub fn set_dpi(device_name: &str, dpi: u32) -> Result<()> {
    Command::new("ratbagctl")
        .args([device_name, "dpi", "set", &dpi.to_string()])
        .output()
        .context("ratbagctl set dpi failed")?;
    tracing::info!("Set DPI to {dpi} on {device_name}");
    Ok(())
}

pub fn set_polling_rate(device_name: &str, rate: u32) -> Result<()> {
    Command::new("ratbagctl")
        .args([device_name, "rate", "set", &rate.to_string()])
        .output()
        .context("ratbagctl set rate failed")?;
    tracing::info!("Set polling rate to {rate}Hz on {device_name}");
    Ok(())
}
