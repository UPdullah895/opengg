//! User-declared identity overrides for cross-transport mouse merging.
//!
//! ratbagd's D-Bus API exposes no per-unit serial (`Model` is identical for
//! every unit of the same mouse model), so the automatic merge heuristic in
//! `ratbag.rs` (vendor ID + normalized model-name match) is a best-effort
//! guess, not a guarantee. This module is the manual safety net: it lets a
//! user confirm or reject a specific pairing, and persists that decision
//! across daemon restarts and future scans.
//!
//! Stored as a small JSON file at `~/.config/opengg/device-identity-overrides.json`.
//! Re-read from disk on every access rather than cached in memory — this file
//! is tiny and rarely written, and re-reading avoids a stale in-memory copy
//! ever drifting from what's on disk (the same freshness-over-caching
//! tradeoff `RatbagManager::resolve_sysname_for_id` already makes for sysname
//! resolution).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A `"{vid:04x}:{pid:04x}"` pair, in no particular order.
pub type LinkPair = (String, String);

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct IdentityOverrides {
    /// Link pairs the user has explicitly confirmed are the same physical
    /// device, regardless of what the name-similarity heuristic concludes.
    #[serde(default)]
    pub force_merge: Vec<LinkPair>,
    /// Link pairs the user has explicitly confirmed are NOT the same
    /// device, even though the name heuristic would otherwise merge them.
    #[serde(default)]
    pub force_split: Vec<LinkPair>,
}

fn overrides_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("opengg/device-identity-overrides.json")
}

impl IdentityOverrides {
    pub fn load() -> Self {
        let path = overrides_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_else(|e| {
                tracing::warn!("device-identity-overrides.json is malformed, ignoring: {e}");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Persist the current state to disk. Callers that add several pairs in
    /// one logical operation (e.g. merging two multi-link groups) should
    /// batch their `add_force_*` calls and save once at the end.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = overrides_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn is_force_split(&self, a: &str, b: &str) -> bool {
        self.force_split
            .iter()
            .any(|(x, y)| pair_matches(x, y, a, b))
    }

    pub fn is_force_merge(&self, a: &str, b: &str) -> bool {
        self.force_merge
            .iter()
            .any(|(x, y)| pair_matches(x, y, a, b))
    }

    /// Record (in memory only — call `save()` to persist) that `a` and `b`
    /// are the same physical device. Clears any conflicting `force_split`
    /// entry for the same pair — an explicit "merge" always overrides a
    /// previous explicit "split" for that exact pair.
    pub fn add_force_merge(&mut self, a: &str, b: &str) {
        self.force_split.retain(|(x, y)| !pair_matches(x, y, a, b));
        if !self.is_force_merge(a, b) {
            self.force_merge.push((a.to_string(), b.to_string()));
        }
    }

    /// Record (in memory only — call `save()` to persist) that `a` and `b`
    /// are NOT the same physical device. Clears any conflicting
    /// `force_merge` entry for the same pair.
    pub fn add_force_split(&mut self, a: &str, b: &str) {
        self.force_merge.retain(|(x, y)| !pair_matches(x, y, a, b));
        if !self.is_force_split(a, b) {
            self.force_split.push((a.to_string(), b.to_string()));
        }
    }
}

fn pair_matches(x: &str, y: &str, a: &str, b: &str) -> bool {
    (x == a && y == b) || (x == b && y == a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_matches_is_order_independent() {
        assert!(pair_matches("046d:c08d", "046d:407f", "046d:407f", "046d:c08d"));
        assert!(pair_matches("046d:c08d", "046d:407f", "046d:c08d", "046d:407f"));
        assert!(!pair_matches("046d:c08d", "046d:407f", "046d:c08d", "046d:aaaa"));
    }

    #[test]
    fn add_force_merge_clears_conflicting_split() {
        let mut o = IdentityOverrides::default();
        o.add_force_split("046d:c08d", "046d:407f");
        assert!(o.is_force_split("046d:407f", "046d:c08d"));

        o.add_force_merge("046d:c08d", "046d:407f");
        assert!(o.is_force_merge("046d:407f", "046d:c08d"));
        assert!(!o.is_force_split("046d:407f", "046d:c08d"));
    }

    #[test]
    fn add_force_split_clears_conflicting_merge() {
        let mut o = IdentityOverrides::default();
        o.add_force_merge("046d:c08d", "046d:407f");
        o.add_force_split("046d:c08d", "046d:407f");
        assert!(o.is_force_split("046d:c08d", "046d:407f"));
        assert!(!o.is_force_merge("046d:c08d", "046d:407f"));
    }

    #[test]
    fn add_force_merge_is_idempotent() {
        let mut o = IdentityOverrides::default();
        o.add_force_merge("046d:c08d", "046d:407f");
        o.add_force_merge("046d:407f", "046d:c08d"); // same pair, reversed order
        assert_eq!(o.force_merge.len(), 1);
    }
}
