//! Button-hotspot coordinate storage — Devices roadmap Phase 5, Part B.
//!
//! Ties a mouse's identity to a set of named button hotspots for the
//! button-mapping editor's photo overlay (Part C). Two separate concerns,
//! deliberately kept apart:
//! - **This module**: hotspot *coordinates* only — small, per-user JSON,
//!   `~/.config/opengg/button-hotspots.json` (see
//!   [`crate::settings::button_hotspots_path`]).
//! - **The user's photo itself**: saved separately by Part C into
//!   [`crate::paths::device_images_cache_dir`] — the same per-model image
//!   cache tier Phase 3's [`crate::device_assets`] already reads from (see
//!   that module's doc comment). Never bundled, never synced, never
//!   referenced from here.
//!
//! ## Identity key
//!
//! Keyed by `"{vendor_id:04x}:{product_id:04x}"` — a *physical link's*
//! vendor:product pair, not a `DeviceInfo.id` string. This is deliberate:
//! Phase 1's cross-transport merge means the *same* physical mouse's `id`
//! string can differ over time (`"ratbag:046d:c08d"` vs
//! `"ratbag:merged:046d:407f+046d:c08d"`, depending on which links ratbagd
//! currently enumerates and whether a manual merge/split override is in
//! effect) — storing under that string directly would silently "lose" a
//! saved photo/hotspot set the next time the merge grouping changes for any
//! reason. [`save_hotspots`] is expected to be called once per *member*
//! vendor:product pair a device's `linkedIds` reports (the caller already
//! has that list from `DeviceInfo`), so a lookup by whichever link happens
//! to be "primary" today always finds the same data.
//!
//! ## Button-index numbering
//!
//! `Hotspot::button_index` is the **0-based** index from
//! `GetButtonMappings`'s `ButtonMapping.index` (`daemon/src/device/ratbag.rs`)
//! — not a `ButtonAction::Button`'s 1-based `target` value. A hotspot names
//! *which physical button on the device* it marks; it says nothing about
//! what that button is currently mapped to do.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// One placed hotspot on a device's photo.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Hotspot {
    /// 0-based, matches `ButtonMapping.index` — see the module doc comment.
    pub button_index: u32,
    /// Normalized position in `[0.0, 1.0]` on both axes, independent of the
    /// backing photo's actual pixel resolution or aspect ratio — so the
    /// same saved hotspots still line up correctly if the user later
    /// replaces their photo with a differently-sized one.
    pub x: f32,
    pub y: f32,
}

/// One device's saved hotspot set.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceHotspots {
    pub hotspots: Vec<Hotspot>,
}

type Store = HashMap<String, DeviceHotspots>;

fn device_key(vendor_id: u16, product_id: u16) -> String {
    format!("{vendor_id:04x}:{product_id:04x}")
}

fn load_store() -> Store {
    let path = crate::settings::button_hotspots_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_store(store: &Store) -> Result<(), String> {
    let path = crate::settings::button_hotspots_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// Get one link's saved hotspots — empty if nothing has been saved yet for
/// this vendor:product pair.
pub fn get_hotspots(vendor_id: u16, product_id: u16) -> Vec<Hotspot> {
    load_store()
        .get(&device_key(vendor_id, product_id))
        .map(|c| c.hotspots.clone())
        .unwrap_or_default()
}

/// Validate a hotspot set against a device's real button count before it's
/// ever written to disk — a pure function so it's unit-testable without
/// touching the real config file (`save_hotspots` below does that, and is
/// verified live/manually instead, same split `daemon/src/device/
/// identity_overrides.rs` uses between its own pure mutation methods and its
/// untested `load`/`save`).
///
/// - every `button_index` must be `< button_count`
/// - no two hotspots may name the same `button_index`
/// - `x`/`y` must each be within `[0.0, 1.0]`
fn validate_hotspots(button_count: u32, hotspots: &[Hotspot]) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for h in hotspots {
        if h.button_index >= button_count {
            return Err(format!(
                "hotspot references button {} but this device only has {button_count} buttons",
                h.button_index
            ));
        }
        if !seen.insert(h.button_index) {
            return Err(format!(
                "button {} has two hotspots — each button may only have one",
                h.button_index
            ));
        }
        if !(0.0..=1.0).contains(&h.x) || !(0.0..=1.0).contains(&h.y) {
            return Err(format!(
                "hotspot position ({}, {}) is outside the normalized 0.0-1.0 range",
                h.x, h.y
            ));
        }
    }
    Ok(())
}

/// Replace one link's hotspot set wholesale — the editor's add/move/delete
/// operations all resolve to one call of this with the final list, once per
/// call. Validates against `button_count` (the caller already has this from
/// `DeviceInfo.button_count`/`GetButtonMappings`) so a bad request is a real
/// error, not silently-accepted junk that only breaks later in the UI.
///
/// `aliases` lets one save cover every link of a merged device at once (see
/// the module doc comment) — pass every vendor:product pair from that
/// device's `linkedIds` (or just its own single pair for an unmerged
/// device) and every one of them resolves to this same hotspot set.
pub fn save_hotspots(
    aliases: &[(u16, u16)],
    button_count: u32,
    hotspots: Vec<Hotspot>,
) -> Result<(), String> {
    if aliases.is_empty() {
        return Err("no device identity given to save hotspots under".to_string());
    }
    validate_hotspots(button_count, &hotspots)?;

    let mut store = load_store();
    let entry = DeviceHotspots { hotspots };
    for &(vid, pid) in aliases {
        store.insert(device_key(vid, pid), entry.clone());
    }
    save_store(&store)
}

// ── Built-in presets ────────────────────────────────────────────────────────
//
// Deliberately empty right now. The task this module was written for asked
// for "a handful of common models" with coordinates "genuinely measured
// against a real reference image" viewed during the session that adds them
// — explicitly *not* coordinates recalled from general familiarity with a
// popular product's typical layout, which is not the same thing as having
// actually looked at one this session and is exactly the kind of
// unverified-but-presented-as-checked accuracy the task's own scope
// boundary warns against ("don't let the preset table imply verified
// accuracy for models nobody has actually checked").
//
// This session's attempt to do that (viewing a real Logitech G502 product
// photo via the sandboxed Browser pane to measure its 11 buttons' positions)
// could not complete: the pane's screenshot capability returned "the Browser
// pane is not displayed" for every attempt, in this environment, so no
// image was actually viewed. Shipping fabricated-from-memory numbers here
// and describing them as measured would violate the task's explicit
// instruction. Leaving the table empty is the honest outcome — the
// machinery (schema, storage, validation, the preset-assist wiring in the
// UI) is fully built and ready for real entries the moment a future session
// can actually view a reference photo.
//
// Add an entry as `(vendor_id, product_id, model_name, &[(button_index, x,
// y, label)])` — `model_name` is shown to the user before they accept the
// preset-assist offer, so they can tell it apart from their own literal
// mouse if the framing doesn't match.
type PresetHotspot = (u32, f32, f32, &'static str);
pub struct DevicePreset {
    pub vendor_id: u16,
    pub product_id: u16,
    pub model_name: &'static str,
    pub hotspots: &'static [PresetHotspot],
}

pub const BUILTIN_PRESETS: &[DevicePreset] = &[];

/// Find a built-in preset for a vendor:product pair, if one has been
/// verified and added (see [`BUILTIN_PRESETS`]).
pub fn find_preset(vendor_id: u16, product_id: u16) -> Option<&'static DevicePreset> {
    BUILTIN_PRESETS
        .iter()
        .find(|p| p.vendor_id == vendor_id && p.product_id == product_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hs(button_index: u32, x: f32, y: f32) -> Hotspot {
        Hotspot { button_index, x, y }
    }

    #[test]
    fn device_key_formats_as_lowercase_hex_pair() {
        assert_eq!(device_key(0x046d, 0xc08d), "046d:c08d");
    }

    #[test]
    fn rejects_a_button_index_the_device_does_not_have() {
        let err = validate_hotspots(5, &[hs(5, 0.5, 0.5)]).unwrap_err();
        assert!(err.contains("only has 5 buttons"), "got: {err}");
    }

    #[test]
    fn rejects_two_hotspots_on_the_same_button() {
        let err =
            validate_hotspots(5, &[hs(1, 0.1, 0.1), hs(1, 0.9, 0.9)]).unwrap_err();
        assert!(err.contains("two hotspots"), "got: {err}");
    }

    #[test]
    fn rejects_an_out_of_range_normalized_position() {
        let err = validate_hotspots(5, &[hs(0, 1.5, 0.5)]).unwrap_err();
        assert!(err.contains("0.0-1.0"), "got: {err}");
    }

    #[test]
    fn accepts_exactly_button_count_hotspots_at_the_boundary_indices() {
        // 3 buttons -> valid indices are 0, 1, 2 -- the off-by-one case a
        // `<=` instead of `<` check would get wrong.
        let hotspots = [hs(0, 0.0, 0.0), hs(1, 0.5, 0.5), hs(2, 1.0, 1.0)];
        assert!(validate_hotspots(3, &hotspots).is_ok());
    }

    #[test]
    fn save_hotspots_rejects_an_empty_alias_list_before_touching_validation() {
        // No real device identity to save under at all -- must fail even
        // for an otherwise-valid, empty hotspot set, and must not depend on
        // validate_hotspots to catch it (an empty hotspot list always
        // validates fine on its own).
        let err = save_hotspots(&[], 5, vec![]).unwrap_err();
        assert!(err.contains("no device identity"), "got: {err}");
    }

    #[test]
    fn builtin_presets_table_has_no_duplicate_vendor_product_pairs() {
        let mut pairs: Vec<(u16, u16)> =
            BUILTIN_PRESETS.iter().map(|p| (p.vendor_id, p.product_id)).collect();
        let before = pairs.len();
        pairs.sort_unstable();
        pairs.dedup();
        assert_eq!(pairs.len(), before, "duplicate vendor:product in BUILTIN_PRESETS");
    }

    #[test]
    fn find_preset_returns_none_when_the_table_is_empty() {
        // Documents the current, deliberate state (see BUILTIN_PRESETS'
        // doc comment) rather than leaving it as an untested assumption —
        // this test is expected to need updating the day a real entry is
        // added, which is the point.
        assert!(find_preset(0x046d, 0xc08d).is_none());
    }
}
