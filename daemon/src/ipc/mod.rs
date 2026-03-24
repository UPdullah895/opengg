//! D-Bus IPC layer

pub mod audio_iface;

use anyhow::{Context, Result};
use zbus::connection::Builder;
use zbus::Connection;

use crate::audio::AudioHub;
use crate::device::DeviceInterface;
use crate::replay::ReplayInterface;

pub async fn serve(
    audio: Option<AudioHub>,
    device: Option<DeviceInterface>,
    replay: Option<ReplayInterface>,
) -> Result<Connection> {
    let mut builder = Builder::session()
        .context("Failed to connect to session D-Bus")?
        .name("org.opengg.Daemon")
        .context("Failed to acquire bus name")?;

    if let Some(hub) = audio {
        let iface = audio_iface::AudioInterface::new(hub);
        builder = builder
            .serve_at("/org/opengg/Daemon/Audio", iface)
            .context("Failed to register Audio interface")?;
        tracing::info!("D-Bus: /org/opengg/Daemon/Audio");
    }

    if let Some(dev) = device {
        builder = builder
            .serve_at("/org/opengg/Daemon/Device", dev)
            .context("Failed to register Device interface")?;
        tracing::info!("D-Bus: /org/opengg/Daemon/Device");
    }

    if let Some(rep) = replay {
        builder = builder
            .serve_at("/org/opengg/Daemon/Replay", rep)
            .context("Failed to register Replay interface")?;
        tracing::info!("D-Bus: /org/opengg/Daemon/Replay");
    }

    let connection = builder.build().await?;
    Ok(connection)
}
