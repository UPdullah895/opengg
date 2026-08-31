use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Mouse,
    Headset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqMeta {
    pub bands: u32,
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub model: String,
    pub device_type: DeviceType,
    pub vid: u16,
    pub pid: u16,
    /// When this card represents a cross-transport merge of two or more
    /// ratbagd device objects that are believed to be the same physical
    /// mouse (see `ratbag::merge_key_slug` / `IdentityOverrides`), this
    /// lists every member link as a `"{vid:04x}:{pid:04x}"` string. `None`
    /// (or omitted) for an ordinary single-link device. `vid`/`pid` above
    /// always describe the currently "primary" (live, if any) link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_ids: Option<Vec<String>>,
    // Mouse-only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polling_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi_options: Option<Vec<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polling_rate_options: Option<Vec<u32>>,
    /// Number of physical buttons on the active profile (Devices Phase 5's
    /// hotspot editor needs this to cap how many hotspots can be placed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub button_count: Option<u32>,
    // Headset-only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_level: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_charging: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidetone: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chatmix: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_presets: Option<HashMap<String, Vec<f32>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eq_meta: Option<EqMeta>,
}
