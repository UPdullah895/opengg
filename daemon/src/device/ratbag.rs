//! ratbagd D-Bus integration for mouse management (org.freedesktop.ratbag1).

use anyhow::{Context, Result};
use zbus::{proxy, zvariant, Connection};

use super::identity_overrides::IdentityOverrides;
use super::types::{DeviceInfo, DeviceType};

// ── D-Bus proxy definitions ──────────────────────────────────────────────────

#[proxy(
    interface = "org.freedesktop.ratbag1.Manager",
    default_service = "org.freedesktop.ratbag1",
    default_path = "/org/freedesktop/ratbag1"
)]
trait Manager {
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.ratbag1.Device",
    default_service = "org.freedesktop.ratbag1"
)]
trait Device {
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn model(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn profiles(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    fn commit(&self) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.freedesktop.ratbag1.Profile",
    default_service = "org.freedesktop.ratbag1"
)]
trait Profile {
    #[zbus(property)]
    fn resolutions(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    #[zbus(property)]
    fn is_active(&self) -> zbus::Result<bool>;
}

#[proxy(
    interface = "org.freedesktop.ratbag1.Resolution",
    default_service = "org.freedesktop.ratbag1"
)]
trait Resolution {
    #[zbus(property)]
    fn resolution(&self) -> zbus::Result<(u32, u32)>;

    #[zbus(property)]
    fn report_rate(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn report_rates(&self) -> zbus::Result<Vec<u32>>;

    #[zbus(property)]
    fn resolutions(&self) -> zbus::Result<Vec<(u32, u32)>>;

    #[zbus(property)]
    fn is_active(&self) -> zbus::Result<bool>;
}

// ── Cross-transport identity ────────────────────────────────────────────────
//
// ratbagd's D-Bus API exposes no persistent per-unit serial — `Model` is only
// "usb:VVVV:PPPP:version", identical for every unit of the same mouse model
// (confirmed against a live ratbagd instance: a Logitech G502 LIGHTSPEED
// reports "usb:046d:c08d:0" over its wireless receiver and "usb:046d:407f:0"
// when wired, as two entirely separate ratbagd device objects with no shared
// property at all). So a single physical mouse can legitimately show up as
// two distinct (vid, pid) pairs depending on connection mode, and both can be
// present in ratbagd's device list *simultaneously* — this isn't a
// stale/replaced-link case we can resolve by timing.
//
// The fallback here is a name-similarity heuristic: same vendor ID, and the
// device names match after stripping known connection-mode/marketing tokens.
// It is deliberately conservative — see `merge_key_slug` — and is always
// overridable by an explicit user decision recorded in
// `IdentityOverrides` (`MergeMouseDevices` / `SplitMouseDevice` on the D-Bus
// interface), which always wins over the heuristic for that specific pair.

/// Marketing/connection-mode tokens stripped before comparing model names.
/// Deliberately excludes anything that could denote a genuinely different
/// hardware revision (e.g. "hero", "se", "plus") — stripping those would
/// risk silently merging two different real devices, which is a worse
/// failure mode than leaving a duplicate card on screen.
const NAME_MERGE_STOPWORDS: &[&str] = &[
    "wireless",
    "lightspeed",
    "bluetooth",
    "receiver",
    "dongle",
    "gaming",
    "mouse",
    "keyboard",
    "headset",
    "usb",
    "rf",
    "2.4ghz",
    "2.4g",
];

/// Reduce a device name to a comparison key for the merge heuristic, or
/// `None` if the name is too generic to safely merge on. Requires at least
/// one remaining token that looks like a real model designator (contains a
/// digit) — otherwise two unrelated devices sharing a vague name like
/// "Logitech Wireless Mouse" would wrongly collapse into one card.
fn merge_key_slug(name: &str) -> Option<String> {
    let tokens: Vec<String> = name
        .split_whitespace()
        .map(|t| t.to_lowercase())
        .filter(|t| !NAME_MERGE_STOPWORDS.contains(&t.as_str()))
        .collect();

    let has_model_designator = tokens.iter().any(|t| t.chars().any(|c| c.is_ascii_digit()));
    if tokens.len() < 2 || !has_model_designator {
        return None;
    }
    Some(tokens.join("-"))
}

/// Parse a `"{vid:04x}:{pid:04x}"` member string.
fn parse_member(s: &str) -> Option<(u16, u16)> {
    let (v, p) = s.split_once(':')?;
    Some((
        u16::from_str_radix(v, 16).ok()?,
        u16::from_str_radix(p, 16).ok()?,
    ))
}

fn format_member(vid: u16, pid: u16) -> String {
    format!("{vid:04x}:{pid:04x}")
}

/// Parse the body of a `"ratbag:"` id (after stripping that prefix) into its
/// member `(vid, pid)` list. A legacy single-link id `"046d:c08d"` yields one
/// member; a merged id `"merged:046d:c08d+046d:407f"` yields all of them.
fn parse_id_members(id_body: &str) -> Vec<(u16, u16)> {
    if let Some(rest) = id_body.strip_prefix("merged:") {
        rest.split('+').filter_map(parse_member).collect()
    } else {
        parse_member(id_body).into_iter().collect()
    }
}

/// A single ratbagd device object, before cross-transport grouping.
struct RawRatbagDevice {
    sysname: String,
    vid: u16,
    pid: u16,
    name: String,
    model: String,
    dpi: Option<u32>,
    polling_rate: Option<u32>,
    dpi_options: Option<Vec<u32>>,
}

/// Union-find grouping of raw ratbagd devices believed to be the same
/// physical mouse — by explicit user override first, then by the name
/// heuristic. Returns groups as index lists into `raw`.
fn group_raw_devices(raw: &[RawRatbagDevice], overrides: &IdentityOverrides) -> Vec<Vec<usize>> {
    let n = raw.len();
    let mut parent: Vec<usize> = (0..n).collect();

    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]);
        }
        parent[x]
    }
    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra] = rb;
        }
    }

    for i in 0..n {
        for j in (i + 1)..n {
            let key_i = format_member(raw[i].vid, raw[i].pid);
            let key_j = format_member(raw[j].vid, raw[j].pid);

            if overrides.is_force_merge(&key_i, &key_j) {
                union(&mut parent, i, j);
                continue;
            }
            if overrides.is_force_split(&key_i, &key_j) {
                continue;
            }
            if raw[i].vid == raw[j].vid {
                if let (Some(a), Some(b)) =
                    (merge_key_slug(&raw[i].name), merge_key_slug(&raw[j].name))
                {
                    if a == b {
                        union(&mut parent, i, j);
                    }
                }
            }
        }
    }

    let mut groups: std::collections::HashMap<usize, Vec<usize>> = Default::default();
    for i in 0..n {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }
    groups.into_values().collect()
}

/// Build the public `DeviceInfo` for one group of raw ratbagd devices. Field
/// values (dpi/polling_rate/name/model) come from whichever member is
/// currently "live" (has an active resolution reporting real values), so a
/// disconnected/phantom link in a merged group never shadows real data.
fn build_device_info(raw: &[RawRatbagDevice], group: &[usize]) -> DeviceInfo {
    let mut idxs = group.to_vec();
    idxs.sort_by_key(|&i| (raw[i].vid, raw[i].pid));
    let primary_idx = idxs
        .iter()
        .copied()
        .find(|&i| raw[i].dpi.is_some())
        .unwrap_or(idxs[0]);
    let primary = &raw[primary_idx];

    let (id, linked_ids) = if idxs.len() == 1 {
        (format!("ratbag:{}", format_member(primary.vid, primary.pid)), None)
    } else {
        let members: Vec<String> = idxs
            .iter()
            .map(|&i| format_member(raw[i].vid, raw[i].pid))
            .collect();
        (
            format!("ratbag:merged:{}", members.join("+")),
            Some(members),
        )
    };

    DeviceInfo {
        id,
        name: primary.name.clone(),
        model: primary.model.clone(),
        device_type: DeviceType::Mouse,
        vid: primary.vid,
        pid: primary.pid,
        linked_ids,
        dpi: primary.dpi,
        polling_rate: primary.polling_rate,
        dpi_options: primary.dpi_options.clone(),
        battery_level: None,
        battery_charging: None,
        sidetone: None,
        chatmix: None,
        capabilities: None,
        eq_presets: None,
        eq_meta: None,
    }
}

// ── RatbagManager ────────────────────────────────────────────────────────────

pub struct RatbagManager {
    conn: Connection,
}

impl RatbagManager {
    pub async fn new() -> Result<Self> {
        let conn = Connection::system()
            .await
            .context("failed to connect to D-Bus system bus")?;
        Ok(Self { conn })
    }

    pub async fn list_devices(&self) -> Vec<DeviceInfo> {
        let raw = self.list_raw_devices().await;
        let overrides = IdentityOverrides::load();
        group_raw_devices(&raw, &overrides)
            .iter()
            .map(|group| build_device_info(&raw, group))
            .collect()
    }

    async fn list_raw_devices(&self) -> Vec<RawRatbagDevice> {
        let manager = {
            let mut last_err = None;
            let mut result = None;
            for _ in 0..3 {
                match ManagerProxy::new(&self.conn).await {
                    Ok(m) => { result = Some(m); break; }
                    Err(e) => {
                        last_err = Some(e);
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }
                }
            }
            match result {
                Some(m) => m,
                None => {
                    tracing::warn!("ratbagd not available after 3 attempts: {}", last_err.unwrap());
                    return vec![];
                }
            }
        };

        let paths = match manager.devices().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("ratbagd devices property failed: {e}");
                return vec![];
            }
        };

        let mut devices = Vec::new();
        for path in paths {
            if let Some(raw) = self.read_raw_device(&path).await {
                devices.push(raw);
            }
        }
        devices
    }

    async fn read_raw_device(&self, path: &zbus::zvariant::OwnedObjectPath) -> Option<RawRatbagDevice> {
        let dev = DeviceProxy::builder(&self.conn)
            .path(path.as_ref())
            .ok()?
            .build()
            .await
            .ok()?;

        let name = dev.name().await.unwrap_or_default();
        let model_str = dev.model().await.unwrap_or_default();

        // Parse VID/PID from model string "usb:VVVV:PPPP:00"
        let (vid, pid) = parse_model_id(&model_str);

        // Get DPI and polling rate from active profile's active resolution
        let (dpi, polling_rate, dpi_options) = self.read_active_resolution(&dev).await;

        let sysname = path
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or("unknown")
            .to_string();

        Some(RawRatbagDevice {
            sysname,
            vid,
            pid,
            name,
            model: model_str,
            dpi,
            polling_rate,
            dpi_options,
        })
    }

    async fn read_active_resolution(
        &self,
        dev: &DeviceProxy<'_>,
    ) -> (Option<u32>, Option<u32>, Option<Vec<u32>>) {
        let profiles = match dev.profiles().await {
            Ok(p) => p,
            Err(_) => return (None, None, None),
        };

        for profile_path in &profiles {
            let profile = match ProfileProxy::builder(&self.conn)
                .path(profile_path.as_ref())
                .ok()
                .and(None::<ProfileProxy>)
            {
                Some(p) => p,
                None => {
                    let Ok(p) = ProfileProxy::builder(&self.conn)
                        .path(profile_path.as_ref())
                        .unwrap()
                        .build()
                        .await
                    else {
                        continue;
                    };
                    p
                }
            };

            if !profile.is_active().await.unwrap_or(false) {
                continue;
            }

            let res_paths = match profile.resolutions().await {
                Ok(r) => r,
                Err(_) => continue,
            };

            for res_path in &res_paths {
                let Ok(res) = ResolutionProxy::builder(&self.conn)
                    .path(res_path.as_ref())
                    .unwrap()
                    .build()
                    .await
                else {
                    continue;
                };

                if !res.is_active().await.unwrap_or(false) {
                    continue;
                }

                let dpi = res.resolution().await.ok().map(|(x, _)| x);
                let rate = res.report_rate().await.ok();
                let dpi_list = res
                    .resolutions()
                    .await
                    .ok()
                    .map(|v| v.into_iter().map(|(x, _)| x).collect());

                return (dpi, rate, dpi_list);
            }
        }
        (None, None, None)
    }

    /// Resolve a device id (legacy single-link `"vid:pid"` or merged
    /// `"merged:vid:pid+vid:pid"`) back to whichever ratbagd sysname is
    /// currently backing it. Re-scans on every call rather than caching —
    /// sysnames are route-scoped and can change across replugs/reboots, and
    /// for a merged id more than one member may currently be enumerated
    /// (see the module doc comment), so this always re-derives which link is
    /// actually live. Prefers a member that's reporting real resolution data
    /// (i.e. actually responding) over one that's merely present.
    async fn resolve_sysname_for_id(&self, id_body: &str) -> Result<String> {
        let members = parse_id_members(id_body);
        if members.is_empty() {
            anyhow::bail!("malformed ratbag device id: {id_body}");
        }

        let raw = self.list_raw_devices().await;
        let mut candidates: Vec<&RawRatbagDevice> = raw
            .iter()
            .filter(|d| members.contains(&(d.vid, d.pid)))
            .collect();
        candidates.sort_by_key(|d| d.dpi.is_none());

        candidates
            .into_iter()
            .next()
            .map(|d| d.sysname.clone())
            .ok_or_else(|| anyhow::anyhow!("no ratbagd device currently matches id {id_body}"))
    }

    pub async fn set_dpi(&self, id_body: &str, dpi: u32) -> Result<()> {
        let sysname = self.resolve_sysname_for_id(id_body).await?;
        let path = format!("/org/freedesktop/ratbag1/device/{sysname}");
        let dev = DeviceProxy::builder(&self.conn)
            .path(path.as_str())
            .context("invalid path")?
            .build()
            .await
            .context("DeviceProxy build failed")?;

        let profiles = dev.profiles().await.context("get profiles")?;
        for profile_path in &profiles {
            let profile = ProfileProxy::builder(&self.conn)
                .path(profile_path.as_ref())
                .unwrap()
                .build()
                .await?;
            if !profile.is_active().await.unwrap_or(false) {
                continue;
            }
            let res_paths = profile.resolutions().await?;
            for res_path in &res_paths {
                let res = ResolutionProxy::builder(&self.conn)
                    .path(res_path.as_ref())
                    .unwrap()
                    .build()
                    .await?;
                if res.is_active().await.unwrap_or(false) {
                    res.inner().set_property("Resolution", zvariant::Value::from(dpi)).await?;
                    dev.commit().await?;
                    tracing::info!("Set DPI to {dpi} on {sysname}");
                    return Ok(());
                }
            }
        }
        anyhow::bail!("active resolution not found for {sysname}")
    }

    pub async fn set_polling_rate(&self, id_body: &str, rate: u32) -> Result<()> {
        let sysname = self.resolve_sysname_for_id(id_body).await?;
        let path = format!("/org/freedesktop/ratbag1/device/{sysname}");
        let dev = DeviceProxy::builder(&self.conn)
            .path(path.as_str())
            .context("invalid path")?
            .build()
            .await
            .context("DeviceProxy build failed")?;

        let profiles = dev.profiles().await.context("get profiles")?;
        for profile_path in &profiles {
            let profile = ProfileProxy::builder(&self.conn)
                .path(profile_path.as_ref())
                .unwrap()
                .build()
                .await?;
            if !profile.is_active().await.unwrap_or(false) {
                continue;
            }
            let res_paths = profile.resolutions().await?;
            for res_path in &res_paths {
                let res = ResolutionProxy::builder(&self.conn)
                    .path(res_path.as_ref())
                    .unwrap()
                    .build()
                    .await?;
                if res.is_active().await.unwrap_or(false) {
                    res.inner().set_property("ReportRate", zvariant::Value::from(rate)).await?;
                    dev.commit().await?;
                    tracing::info!("Set polling rate to {rate}Hz on {sysname}");
                    return Ok(());
                }
            }
        }
        anyhow::bail!("active resolution not found for {sysname}")
    }

    /// Manual override: record that the devices behind `id_body_a` and
    /// `id_body_b` are the same physical mouse, regardless of what the name
    /// heuristic concludes. Either id may itself already be a merged id —
    /// every member of one is paired with every member of the other. Takes
    /// effect on the next `list_devices()` call (no live-state mutation).
    pub async fn merge_devices(&self, id_body_a: &str, id_body_b: &str) -> Result<()> {
        let members_a = parse_id_members(id_body_a);
        let members_b = parse_id_members(id_body_b);
        if members_a.is_empty() || members_b.is_empty() {
            anyhow::bail!("malformed device id in merge request");
        }

        let mut overrides = IdentityOverrides::load();
        for &(va, pa) in &members_a {
            for &(vb, pb) in &members_b {
                overrides.add_force_merge(&format_member(va, pa), &format_member(vb, pb));
            }
        }
        overrides.save()
    }

    /// Manual override: decompose a merged device id back into separate
    /// cards, and remember that decision so the name heuristic never
    /// re-merges these specific links on its own again.
    pub async fn split_device(&self, id_body: &str) -> Result<()> {
        let members = parse_id_members(id_body);
        if members.len() < 2 {
            anyhow::bail!("device id {id_body} is not a merged device, nothing to split");
        }

        let mut overrides = IdentityOverrides::load();
        for i in 0..members.len() {
            for j in (i + 1)..members.len() {
                let (va, pa) = members[i];
                let (vb, pb) = members[j];
                overrides.add_force_split(&format_member(va, pa), &format_member(vb, pb));
            }
        }
        overrides.save()
    }
}

fn parse_model_id(model: &str) -> (u16, u16) {
    // Format: "usb:VVVV:PPPP:00" or empty
    let parts: Vec<&str> = model.split(':').collect();
    let vid = parts
        .get(1)
        .and_then(|s| u16::from_str_radix(s, 16).ok())
        .unwrap_or(0);
    let pid = parts
        .get(2)
        .and_then(|s| u16::from_str_radix(s, 16).ok())
        .unwrap_or(0);
    (vid, pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(sysname: &str, vid: u16, pid: u16, name: &str, dpi: Option<u32>) -> RawRatbagDevice {
        RawRatbagDevice {
            sysname: sysname.to_string(),
            vid,
            pid,
            name: name.to_string(),
            model: format!("usb:{vid:04x}:{pid:04x}:0"),
            dpi,
            polling_rate: None,
            dpi_options: None,
        }
    }

    #[test]
    fn merge_key_slug_matches_across_connection_modes() {
        let wireless = merge_key_slug("Logitech G502 LIGHTSPEED Wireless Gaming Mouse");
        let wired = merge_key_slug("Logitech G502");
        assert_eq!(wireless, wired);
        assert_eq!(wireless.as_deref(), Some("logitech-g502"));
    }

    #[test]
    fn merge_key_slug_rejects_generic_names() {
        // No model designator (no digit token) — too generic to safely merge on.
        assert_eq!(merge_key_slug("Logitech Wireless Mouse"), None);
        assert_eq!(merge_key_slug("Mouse"), None);
    }

    #[test]
    fn merge_key_slug_does_not_collapse_different_hardware_revisions() {
        // "hero"/"se" aren't in the stopword list on purpose — a G502 and a
        // G502 HERO are different real hardware and must never merge.
        let base = merge_key_slug("Logitech G502");
        let hero = merge_key_slug("Logitech G502 HERO");
        assert_ne!(base, hero);
    }

    #[test]
    fn grouping_merges_g502_wired_and_wireless_by_name_heuristic() {
        let raw = vec![
            raw("hidraw0", 0x046d, 0xc08d, "Logitech G502 LIGHTSPEED Wireless Gaming Mouse", Some(800)),
            raw("hidraw13", 0x046d, 0x407f, "Logitech G502", None),
        ];
        let overrides = IdentityOverrides::default();
        let groups = group_raw_devices(&raw, &overrides);
        assert_eq!(groups.len(), 1, "expected the two links to merge into one group");

        let info = build_device_info(&raw, &groups[0]);
        assert_eq!(info.id, "ratbag:merged:046d:407f+046d:c08d");
        assert_eq!(info.linked_ids.as_deref(), Some(&["046d:407f".to_string(), "046d:c08d".to_string()][..]));
        // The live link (dpi = Some) must win as primary, even though it
        // sorts second by (vid, pid).
        assert_eq!(info.dpi, Some(800));
        assert_eq!(info.name, "Logitech G502 LIGHTSPEED Wireless Gaming Mouse");
    }

    #[test]
    fn grouping_leaves_different_models_separate() {
        let raw = vec![
            raw("hidraw0", 0x046d, 0xc08d, "Logitech G502 LIGHTSPEED Wireless Gaming Mouse", Some(800)),
            raw("hidraw5", 0x046d, 0xc092, "Logitech G Pro Wireless", Some(1600)),
        ];
        let overrides = IdentityOverrides::default();
        let groups = group_raw_devices(&raw, &overrides);
        assert_eq!(groups.len(), 2, "different mouse models must never auto-merge");
    }

    #[test]
    fn force_split_override_blocks_the_heuristic() {
        let raw = vec![
            raw("hidraw0", 0x046d, 0xc08d, "Logitech G502 LIGHTSPEED Wireless Gaming Mouse", Some(800)),
            raw("hidraw13", 0x046d, 0x407f, "Logitech G502", None),
        ];
        let mut overrides = IdentityOverrides::default();
        overrides.add_force_split("046d:c08d", "046d:407f");
        let groups = group_raw_devices(&raw, &overrides);
        assert_eq!(groups.len(), 2, "an explicit split override must win over the heuristic");
    }

    #[test]
    fn force_merge_override_joins_devices_the_heuristic_would_not() {
        let raw = vec![
            raw("hidraw0", 0x046d, 0xc08d, "Logitech G502 LIGHTSPEED Wireless Gaming Mouse", Some(800)),
            raw("hidraw5", 0x046d, 0xc092, "Logitech G Pro Wireless", Some(1600)),
        ];
        let mut overrides = IdentityOverrides::default();
        overrides.add_force_merge("046d:c08d", "046d:c092");
        let groups = group_raw_devices(&raw, &overrides);
        assert_eq!(groups.len(), 1, "an explicit merge override must join devices the heuristic would keep apart");
    }

    #[test]
    fn parse_id_members_handles_legacy_and_merged_ids() {
        assert_eq!(parse_id_members("046d:c08d"), vec![(0x046d, 0xc08d)]);
        assert_eq!(
            parse_id_members("merged:046d:c08d+046d:407f"),
            vec![(0x046d, 0xc08d), (0x046d, 0x407f)]
        );
        assert_eq!(parse_id_members("not-an-id"), vec![]);
    }
}
