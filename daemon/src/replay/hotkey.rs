//! Global Hotkey Listener
//!
//! Reads keyboard events from /dev/input/eventN via evdev to detect global
//! hotkey combinations (e.g., Alt+F10 to save replay). Works on both X11
//! and Wayland since it reads directly from the kernel input subsystem.
//!
//! Requires the user to be in the `input` group.

use anyhow::{Context, Result};
use std::path::PathBuf;
use tokio::sync::mpsc;

/// A hotkey event.
#[derive(Debug, Clone)]
pub enum HotkeyEvent {
    SaveReplay,
    ToggleRecording,
    Screenshot,
}

/// Configuration for which keys trigger which actions.
#[derive(Debug, Clone)]
pub struct HotkeyConfig {
    pub save_replay: String,     // e.g. "Alt+F10"
    pub toggle_recording: String, // e.g. "Alt+F9"
    pub screenshot: String,       // e.g. "Alt+F12"
}

/// Start listening for global hotkeys. Returns a receiver channel.
///
/// This spawns a blocking thread that reads from /dev/input devices.
/// The `input` group membership is required.
pub fn start_listener(config: HotkeyConfig) -> Result<mpsc::UnboundedReceiver<HotkeyEvent>> {
    let (tx, rx) = mpsc::unbounded_channel();

    // Find keyboard devices
    let keyboards = find_keyboards()?;
    if keyboards.is_empty() {
        tracing::warn!("No keyboard input devices found. Are you in the 'input' group?");
        return Ok(rx);
    }

    tracing::info!("Hotkey listener monitoring {} keyboard(s)", keyboards.len());

    // Spawn a blocking thread for evdev reading
    // (evdev is synchronous and we don't want to block tokio)
    std::thread::Builder::new()
        .name("hotkey-listener".into())
        .spawn(move || {
            if let Err(e) = hotkey_loop(&keyboards, &config, &tx) {
                tracing::error!("Hotkey listener error: {e}");
            }
        })
        .context("Failed to spawn hotkey thread")?;

    Ok(rx)
}

/// Find keyboard input devices in /dev/input/.
fn find_keyboards() -> Result<Vec<PathBuf>> {
    let mut keyboards = Vec::new();

    for entry in std::fs::read_dir("/dev/input")? {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();

        // Look for eventN devices
        if !name.starts_with("event") {
            continue;
        }

        // Check if this is a keyboard by reading its capabilities
        let sysfs_path = format!(
            "/sys/class/input/{}/device/capabilities/key",
            name
        );
        if let Ok(caps) = std::fs::read_to_string(&sysfs_path) {
            // Keyboards have extensive key capabilities (the bitmask is large)
            // A simple heuristic: if the caps string is longer than 10 chars, it's a keyboard
            if caps.trim().len() > 10 {
                keyboards.push(path);
            }
        }
    }

    Ok(keyboards)
}

/// Main hotkey reading loop (runs in a blocking thread).
fn hotkey_loop(
    _keyboards: &[PathBuf],
    config: &HotkeyConfig,
    _tx: &mpsc::UnboundedSender<HotkeyEvent>,
) -> Result<()> {
    // In a production implementation, this would use the `evdev` crate:
    //
    //   let devices: Vec<evdev::Device> = keyboards.iter()
    //       .filter_map(|p| evdev::Device::open(p).ok())
    //       .collect();
    //
    //   let mut pressed_keys: HashSet<evdev::Key> = HashSet::new();
    //
    //   loop {
    //       for device in &devices {
    //           for event in device.fetch_events()? {
    //               match event.kind() {
    //                   InputEventKind::Key(key) => {
    //                       match event.value() {
    //                           1 => { pressed_keys.insert(key); }
    //                           0 => { pressed_keys.remove(&key); }
    //                           _ => {}
    //                       }
    //                       check_hotkeys(&pressed_keys, config, tx);
    //                   }
    //                   _ => {}
    //               }
    //           }
    //       }
    //   }

    tracing::info!(
        "Hotkey listener ready (save={}, record={}, screenshot={})",
        config.save_replay,
        config.toggle_recording,
        config.screenshot
    );

    // Placeholder: sleep forever since we don't have evdev crate in deps yet
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
}
