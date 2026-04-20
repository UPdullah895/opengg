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
    // Mouse-only
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polling_rate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dpi_options: Option<Vec<u32>>,
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
