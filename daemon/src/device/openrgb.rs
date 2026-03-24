//! OpenRGB SDK Client
//!
//! Connects to the OpenRGB server (default port 6742) via its binary TCP protocol
//! to control RGB lighting on motherboards, RAM, GPUs, peripherals, etc.
//!
//! Protocol reference: https://gitlab.com/CalcProgrammer1/OpenRGB/-/wikis/OpenRGB-SDK-Documentation

use anyhow::{Context, Result};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const OPENRGB_MAGIC: &[u8; 4] = b"ORGB";

// Protocol packet IDs
const NET_PACKET_ID_REQUEST_CONTROLLER_COUNT: u32 = 0;
const NET_PACKET_ID_REQUEST_CONTROLLER_DATA: u32 = 1;
const NET_PACKET_ID_SET_CLIENT_NAME: u32 = 50;
const NET_PACKET_ID_RGBCONTROLLER_UPDATESINGLELED: u32 = 1050;
const NET_PACKET_ID_RGBCONTROLLER_UPDATEMODE: u32 = 1101;

/// A connected OpenRGB client session.
pub struct OpenRGBClient {
    stream: TcpStream,
}

/// An RGB controller (device) reported by OpenRGB.
#[derive(Debug, Clone, Serialize)]
pub struct RGBController {
    pub index: u32,
    pub name: String,
    pub vendor: String,
    pub device_type: String,
    pub num_leds: u32,
    pub modes: Vec<String>,
    pub active_mode: u32,
}

impl OpenRGBClient {
    /// Connect to the OpenRGB SDK server.
    pub async fn connect(host: &str, port: u16) -> Result<Self> {
        let addr = format!("{host}:{port}");
        let stream = TcpStream::connect(&addr)
            .await
            .with_context(|| format!("Could not connect to OpenRGB at {addr}"))?;

        let mut client = Self { stream };

        // Identify ourselves
        client
            .send_packet(NET_PACKET_ID_SET_CLIENT_NAME, 0, b"OpenGG\0")
            .await?;

        tracing::info!("Connected to OpenRGB at {addr}");
        Ok(client)
    }

    /// Get the number of RGB controllers.
    pub async fn get_controller_count(&mut self) -> Result<u32> {
        self.send_packet(NET_PACKET_ID_REQUEST_CONTROLLER_COUNT, 0, &[])
            .await?;
        let (_id, _dev, data) = self.recv_packet().await?;

        if data.len() >= 4 {
            Ok(u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
        } else {
            Ok(0)
        }
    }

    /// Set a single LED color on a controller.
    pub async fn set_led_color(
        &mut self,
        controller_idx: u32,
        led_idx: u32,
        r: u8,
        g: u8,
        b: u8,
    ) -> Result<()> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&led_idx.to_le_bytes());
        payload.push(r);
        payload.push(g);
        payload.push(b);
        payload.push(0); // padding

        self.send_packet(
            NET_PACKET_ID_RGBCONTROLLER_UPDATESINGLELED,
            controller_idx,
            &payload,
        )
        .await?;

        Ok(())
    }

    /// Set all LEDs on a controller to a single color.
    pub async fn set_all_leds(
        &mut self,
        controller_idx: u32,
        num_leds: u32,
        r: u8,
        g: u8,
        b: u8,
    ) -> Result<()> {
        for led in 0..num_leds {
            self.set_led_color(controller_idx, led, r, g, b).await?;
        }
        Ok(())
    }

    /// Set the active mode on a controller.
    pub async fn set_mode(&mut self, controller_idx: u32, mode_idx: u32) -> Result<()> {
        let payload = mode_idx.to_le_bytes();
        self.send_packet(
            NET_PACKET_ID_RGBCONTROLLER_UPDATEMODE,
            controller_idx,
            &payload,
        )
        .await?;
        Ok(())
    }

    // ── Protocol helpers ────────────────────────────────────────

    async fn send_packet(&mut self, pkt_id: u32, dev_idx: u32, data: &[u8]) -> Result<()> {
        let mut header = Vec::with_capacity(16 + data.len());
        header.extend_from_slice(OPENRGB_MAGIC);
        header.extend_from_slice(&dev_idx.to_le_bytes());
        header.extend_from_slice(&pkt_id.to_le_bytes());
        header.extend_from_slice(&(data.len() as u32).to_le_bytes());
        header.extend_from_slice(data);

        self.stream.write_all(&header).await?;
        Ok(())
    }

    async fn recv_packet(&mut self) -> Result<(u32, u32, Vec<u8>)> {
        let mut header = [0u8; 16];
        self.stream.read_exact(&mut header).await?;

        // Validate magic
        if &header[0..4] != OPENRGB_MAGIC {
            anyhow::bail!("Invalid OpenRGB response magic");
        }

        let dev_idx = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        let pkt_id = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
        let data_len = u32::from_le_bytes([header[12], header[13], header[14], header[15]]) as usize;

        let mut data = vec![0u8; data_len];
        if data_len > 0 {
            self.stream.read_exact(&mut data).await?;
        }

        Ok((pkt_id, dev_idx, data))
    }
}

/// Parse a `#RRGGBB` hex string into (r, g, b).
pub fn parse_hex_color(hex: &str) -> Result<(u8, u8, u8)> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        anyhow::bail!("Invalid hex color: #{hex}");
    }
    let r = u8::from_str_radix(&hex[0..2], 16)?;
    let g = u8::from_str_radix(&hex[2..4], 16)?;
    let b = u8::from_str_radix(&hex[4..6], 16)?;
    Ok((r, g, b))
}
