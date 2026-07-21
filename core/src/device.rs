//! Device commands — blocking wrapper for D-Bus device interface.
//!
//! Provides access to mouse, keyboard, and headset device control via the
//! OpenGG daemon. All functions are synchronous and delegate to the blocking
//! D-Bus client.

use crate::daemon::{call_dbus, call_dbus_void, DV_IFACE, DV_PATH};

/// Fetch all connected devices as a JSON string.
pub fn get_devices() -> Result<String, String> {
    call_dbus("GetDevices", DV_PATH, DV_IFACE, ())
}

/// Set mouse DPI for a device.
pub fn set_mouse_dpi(device_id: String, dpi: u32) -> Result<(), String> {
    call_dbus_void("SetDpi", DV_PATH, DV_IFACE, (device_id.as_str(), dpi))
}

/// Set mouse polling rate for a device.
pub fn set_mouse_polling_rate(device_id: String, rate: u32) -> Result<(), String> {
    call_dbus_void("SetPollingRate", DV_PATH, DV_IFACE, (device_id.as_str(), rate))
}

/// Set headset sidetone level.
pub fn set_headset_sidetone(device_id: String, level: u32) -> Result<(), String> {
    call_dbus_void("SetSidetone", DV_PATH, DV_IFACE, (device_id.as_str(), level))
}

/// Set headset chatmix level (balance between game and chat audio).
pub fn set_headset_chatmix(device_id: String, level: u32) -> Result<(), String> {
    call_dbus_void("SetChatmix", DV_PATH, DV_IFACE, (device_id.as_str(), level))
}

/// Set headset idle/inactive timeout in minutes.
pub fn set_headset_inactive_time(device_id: String, minutes: u32) -> Result<(), String> {
    call_dbus_void("SetInactiveTime", DV_PATH, DV_IFACE, (device_id.as_str(), minutes))
}

/// Set headset microphone volume level.
pub fn set_headset_mic_volume(device_id: String, level: u32) -> Result<(), String> {
    call_dbus_void("SetMicrophoneVolume", DV_PATH, DV_IFACE, (device_id.as_str(), level))
}

/// Set headset microphone mute LED brightness.
pub fn set_headset_mic_mute_led(device_id: String, brightness: u32) -> Result<(), String> {
    call_dbus_void(
        "SetMicMuteLedBrightness",
        DV_PATH,
        DV_IFACE,
        (device_id.as_str(), brightness),
    )
}

/// Enable/disable headset volume limiter.
pub fn set_headset_volume_limiter(device_id: String, enabled: bool) -> Result<(), String> {
    call_dbus_void("SetVolumeLimiter", DV_PATH, DV_IFACE, (device_id.as_str(), enabled))
}

/// Enable/disable headset Bluetooth when device is powered on.
pub fn set_headset_bt_powered_on(device_id: String, enabled: bool) -> Result<(), String> {
    call_dbus_void("SetBtWhenPoweredOn", DV_PATH, DV_IFACE, (device_id.as_str(), enabled))
}

/// Set headset Bluetooth call volume level.
pub fn set_headset_bt_call_volume(device_id: String, level: u32) -> Result<(), String> {
    call_dbus_void("SetBtCallVolume", DV_PATH, DV_IFACE, (device_id.as_str(), level))
}

/// Set headset EQ preset by index.
pub fn set_headset_eq_preset(device_id: String, preset_idx: u32) -> Result<(), String> {
    call_dbus_void("SetEqPreset", DV_PATH, DV_IFACE, (device_id.as_str(), preset_idx))
}

/// Set custom headset EQ curve (bands as JSON string).
pub fn set_headset_eq_curve(device_id: String, bands_json: String) -> Result<(), String> {
    call_dbus_void(
        "SetEqCurve",
        DV_PATH,
        DV_IFACE,
        (device_id.as_str(), bands_json.as_str()),
    )
}
