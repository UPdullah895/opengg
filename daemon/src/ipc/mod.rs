//! D-Bus IPC layer

pub mod audio_iface;
pub mod extensions_iface;

use anyhow::{Context, Result};
use std::sync::Arc;
use zbus::connection::Builder;
use zbus::Connection;

use crate::audio::AudioHub;
use crate::device::DeviceInterface;
use crate::extensions::ExtensionManager;
use crate::replay::ReplayInterface;

pub async fn serve(
    audio: Option<AudioHub>,
    device: Option<DeviceInterface>,
    replay: Option<ReplayInterface>,
    extensions: Arc<ExtensionManager>,
) -> Result<Connection> {
    // zbus 6 defers every builder error to `build()`, so a bad name or path
    // and a missing session bus all surface from the one call at the end.
    let mut builder = Builder::session().name("org.opengg.Daemon");

    if let Some(hub) = audio {
        let iface = audio_iface::AudioInterface::new(hub);
        builder = builder
            .serve_at("/org/opengg/Daemon/Audio", iface);
        tracing::info!("D-Bus: /org/opengg/Daemon/Audio");
    }

    if let Some(dev) = device {
        builder = builder
            .serve_at("/org/opengg/Daemon/Device", dev);
        tracing::info!("D-Bus: /org/opengg/Daemon/Device");
    }

    if let Some(rep) = replay {
        builder = builder
            .serve_at("/org/opengg/Daemon/Replay", rep);
        tracing::info!("D-Bus: /org/opengg/Daemon/Replay");
    }

    builder = builder
        .serve_at(
            "/org/opengg/Daemon/Extensions",
            extensions_iface::ExtensionsInterface::new(extensions),
        );
    tracing::info!("D-Bus: /org/opengg/Daemon/Extensions");

    let connection = builder
        .build()
        .await
        .context("Failed to connect to the session D-Bus and serve org.opengg.Daemon")?;
    Ok(connection)
}
