//! Device image resolution — Devices roadmap Phase 3.
//!
//! This is greenfield: qt-shell has never had a device-image pipeline (the
//! old Tauri+Vue UI's `deviceAssets.ts`/`device_database.json`/
//! `frontend/src/assets/devices/` and `scripts/fetch_device_assets.sh` were
//! all removed along with `frontend/` — see `docs/AI-CHANGELOG.md`'s
//! "Remove the archived Tauri + Vue frontend" entry).
//!
//! **Scope, deliberately limited**: this phase ships only bundled generic
//! silhouettes shipped with the app itself — no network sync, no per-model
//! real photos. The OpenLogi-applicability report this plan is based on
//! flagged the actual device-render image catalogs projects like it pull
//! from (e.g. `assets.openlogi.org` / `@logi-assets` npm packages) as having
//! **no stated sourcing/licensing** anywhere in that project's own repo —
//! only a trademark disclaimer. Pulling from a catalog like that here would
//! be introducing an asset of unverified license, not "using the same
//! approach as an MIT project." Don't add a network sync path against any
//! such catalog without independently verifying its actual license first,
//! separately from the code license of whatever project hosts it.
//!
//! Resolution order:
//! 1. A per-user cached per-model image at
//!    [`crate::paths::device_images_cache_dir`] — the tier a future sync
//!    pipeline would populate. Nothing writes to it yet; checked first so
//!    that pipeline, when it exists, doesn't require touching this function.
//! 2. A bundled generic silhouette for the device's kind (mouse/headset),
//!    installed at packaging time — see `bundled_dir`.
//! 3. `None` — callers must treat this exactly like "no image available
//!    ever," not paint a broken-image placeholder.

use std::path::PathBuf;

/// Generic device kind, for selecting a bundled silhouette. Deliberately not
/// `daemon::device::types::DeviceType` — `core/` never imports `daemon/`
/// types (see `AGENTS.md`'s crate-separation rule) — callers parse this from
/// the same `deviceType` string already present in every device JSON entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Mouse,
    Headset,
}

impl DeviceKind {
    /// Parse the `deviceType` string from a device JSON entry (`"mouse"` /
    /// `"headset"`, matching `daemon/src/device/types.rs`'s `DeviceType`
    /// serde rename). Returns `None` for anything else rather than
    /// guessing — an unrecognized kind falls through to no image.
    pub fn from_device_type(s: &str) -> Option<Self> {
        match s {
            "mouse" => Some(Self::Mouse),
            "headset" => Some(Self::Headset),
            _ => None,
        }
    }

    fn silhouette_filename(self) -> &'static str {
        match self {
            Self::Mouse => "mouse-silhouette.png",
            Self::Headset => "headset-silhouette.png",
        }
    }
}

/// Resolve the image to show for a device. `vendor_id`/`product_id` drive
/// the (currently always-empty) per-model cache lookup; `kind` drives the
/// bundled-silhouette fallback. Returns `None` — never a broken-image
/// path — when neither tier has anything.
pub fn resolve_device_image(
    vendor_id: u16,
    product_id: u16,
    kind: Option<DeviceKind>,
) -> Option<PathBuf> {
    let cached = crate::paths::device_images_cache_dir()
        .join(format!("{vendor_id:04x}-{product_id:04x}.png"));
    if cached.is_file() {
        return Some(cached);
    }

    let silhouette = bundled_dir().join(kind?.silhouette_filename());
    silhouette.is_file().then_some(silhouette)
}

/// Where the bundled placeholder silhouettes live. Dev default: the repo's
/// `packaging/device-images/` (works out of `./dev.sh` with zero install
/// step). Override with `OPENGG_DEVICE_IMAGES_DIR` in production — same
/// dev-default/env-override idiom as `qt-shell/src/i18n.rs`'s
/// `locales_dir()`. Like that function's own production path, actually
/// wiring an installed system path into packaging (AUR/Flatpak/deb/rpm) is
/// not done as part of this phase — see this commit's AI-CHANGELOG entry.
fn bundled_dir() -> PathBuf {
    std::env::var("OPENGG_DEVICE_IMAGES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../packaging/device-images"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_kind_parses_known_strings_only() {
        assert_eq!(DeviceKind::from_device_type("mouse"), Some(DeviceKind::Mouse));
        assert_eq!(DeviceKind::from_device_type("headset"), Some(DeviceKind::Headset));
        assert_eq!(DeviceKind::from_device_type("keyboard"), None);
        assert_eq!(DeviceKind::from_device_type(""), None);
    }

    #[test]
    fn resolves_bundled_silhouettes_when_no_cache_entry_exists() {
        // No per-user cache entry will ever exist for this made-up
        // vendor:product pair, so this exercises the bundled-fallback tier
        // against the real files shipped in packaging/device-images/.
        let mouse = resolve_device_image(0xffff, 0xffff, Some(DeviceKind::Mouse));
        assert!(mouse.as_deref().is_some_and(|p| p.ends_with("mouse-silhouette.png")));

        let headset = resolve_device_image(0xffff, 0xffff, Some(DeviceKind::Headset));
        assert!(headset.as_deref().is_some_and(|p| p.ends_with("headset-silhouette.png")));
    }

    #[test]
    fn resolves_to_none_for_an_unrecognized_kind() {
        assert_eq!(resolve_device_image(0xffff, 0xffff, None), None);
    }
}
