//! Keeps the audio graph the way OpenGG set it up, as the system changes.
//!
//! Before this, the daemon wired the graph once at startup and never looked
//! at it again:
//!
//! - each channel's monitor was linked to whatever the default output was at
//!   boot, so unplugging a headset or switching outputs left every OpenGG
//!   channel playing into a device that was gone (silence until restart);
//! - a PipeWire / pipewire-pulse restart removed the null-sinks (they are
//!   modules inside pipewire-pulse) and nothing recreated them;
//! - saved app → channel rules were applied by the UI once, when it started,
//!   so anything launched later ignored them.
//!
//! This module listens to `pactl subscribe` and, after a short settle delay,
//! re-applies the intended state: sinks exist (unless the user removed
//! them), each output channel's monitor feeds its chosen device (the saved
//! per-channel device when present, else the current default), and new
//! streams land on their saved channel. When the subscription drops (the
//! sound server restarted) it reconnects with backoff and does a full pass,
//! restoring the channel levels it last saw.
//!
//! Planning is pure (see the tests); only [`spawn`] and the `apply_*`
//! helpers touch the system.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, BufReader};

use super::routing;
use super::sinks::{self, SinkManager};
use crate::subprocess;

/// Channels whose monitor is linked to a playback device. Mic is a capture
/// path and is wired by its own loopback.
pub const OUTPUT_CHANNELS: &[&str] = &["Game", "Chat", "Media", "Aux"];
const PORTS: &[&str] = &["FL", "FR"];

/// Events are collected for this long after the first one before acting,
/// so a device plug (card + sinks + sources + server change) is one pass.
const SETTLE: Duration = Duration::from_millis(400);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    New,
    Change,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facility {
    Sink,
    SinkInput,
    Server,
    Module,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub kind: Kind,
    pub facility: Facility,
    pub index: Option<u32>,
}

/// Parse one `pactl subscribe` line: `Event 'new' on sink-input #42`.
/// The child runs under `LC_ALL=C`, so the format is fixed.
pub fn parse_event(line: &str) -> Option<Event> {
    let rest = line.trim().strip_prefix("Event '")?;
    let (kind, rest) = rest.split_once("' on ")?;
    let kind = match kind {
        "new" => Kind::New,
        "change" => Kind::Change,
        "remove" => Kind::Remove,
        _ => return None,
    };
    let (facility, index) = match rest.split_once(" #") {
        Some((f, i)) => (f.trim(), i.trim().parse::<u32>().ok()),
        None => (rest.trim(), None),
    };
    let facility = match facility {
        "sink" => Facility::Sink,
        "sink-input" => Facility::SinkInput,
        "server" => Facility::Server,
        "module" => Facility::Module,
        _ => Facility::Other,
    };
    Some(Event { kind, facility, index })
}

/// What a batch of events asks the reconciler to do.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Work {
    /// Check sinks exist and re-link channel monitors.
    pub relink: bool,
    /// Sink-inputs that appeared; saved rules are applied to these.
    pub new_streams: Vec<u32>,
    /// Apply saved rules to every live stream (on (re)connect).
    pub all_streams: bool,
    /// Re-read channel levels so they can be restored after a restart.
    pub snapshot: bool,
}

impl Work {
    pub fn full() -> Self {
        Work { relink: true, new_streams: Vec::new(), all_streams: true, snapshot: true }
    }

    pub fn is_empty(&self) -> bool {
        !self.relink && self.new_streams.is_empty() && !self.all_streams && !self.snapshot
    }
}

pub fn plan_work(events: &[Event]) -> Work {
    let mut w = Work::default();
    for e in events {
        match (e.facility, e.kind) {
            // A device came or went, or the default output changed.
            (Facility::Sink, Kind::New | Kind::Remove) | (Facility::Server, _) => w.relink = true,
            // Someone unloaded a module — possibly one of ours.
            (Facility::Module, Kind::Remove) => w.relink = true,
            // Volume or mute moved; remember it for restart recovery.
            (Facility::Sink, Kind::Change) => w.snapshot = true,
            (Facility::SinkInput, Kind::New) => {
                if let Some(i) = e.index {
                    if !w.new_streams.contains(&i) {
                        w.new_streams.push(i);
                    }
                }
            }
            _ => {}
        }
    }
    w
}

/// The device a channel should play through: its saved device while that
/// device exists, else the default output. Never an OpenGG sink — linking a
/// monitor into another OpenGG sink would loop or double the audio.
pub fn target_device<'a>(
    channel: &str,
    overrides: &'a HashMap<String, String>,
    default_sink: Option<&'a str>,
    sinks: &HashSet<String>,
) -> Option<&'a str> {
    let usable = |d: &&str| !d.is_empty() && !d.starts_with("OpenGG_") && sinks.contains(*d);
    overrides.get(channel).map(String::as_str).filter(usable).or(default_sink.filter(usable))
}

#[derive(Debug, Default, PartialEq)]
pub struct LinkPlan {
    pub add: Vec<(String, String)>,
    pub remove: Vec<(String, String)>,
}

/// Links to create and to remove so each output channel's monitor feeds
/// exactly its target device. Only links into other real (non-OpenGG)
/// playback ports are removed; captures reading a monitor are left alone.
/// A link from a monitor into any OpenGG sink is always removed: it loops a
/// channel into itself or doubles it into another. Otherwise a channel with
/// no usable target is left as it is.
pub fn plan_links(
    overrides: &HashMap<String, String>,
    default_sink: Option<&str>,
    sinks: &HashSet<String>,
    links: &HashSet<(String, String)>,
) -> LinkPlan {
    let mut plan = LinkPlan::default();
    for ch in OUTPUT_CHANNELS {
        let target = target_device(ch, overrides, default_sink, sinks);
        for port in PORTS {
            let src = format!("OpenGG_{ch}:monitor_{port}");
            let want = target.map(|t| format!("{t}:playback_{port}"));
            for (s, d) in links {
                if s != &src || Some(d) == want.as_ref() {
                    continue;
                }
                let Some((node, p)) = d.rsplit_once(':') else { continue };
                if !p.starts_with("playback_") {
                    continue;
                }
                let into_opengg = node.starts_with("OpenGG_");
                let into_other_device = target.is_some() && sinks.contains(node);
                if into_opengg || into_other_device {
                    plan.remove.push((s.clone(), d.clone()));
                }
            }
            if let Some(want) = want {
                if !links.contains(&(src.clone(), want.clone())) {
                    plan.add.push((src, want));
                }
            }
        }
    }
    plan.remove.sort();
    plan
}

/// Saved preferences the reconciler applies. Both live in the UI's
/// `ui-settings.json`, which stays the single source of truth.
#[derive(Debug, Default, PartialEq)]
pub struct Prefs {
    /// `mixer.devices`: channel → sink name.
    pub devices: HashMap<String, String>,
    /// `mixer.appRules`: lowercased binary or app name → channel.
    pub rules: HashMap<String, String>,
}

pub fn parse_prefs(json: &str) -> Prefs {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Prefs::default();
    };
    let strings = |o: &serde_json::Value, lower: bool| -> HashMap<String, String> {
        o.as_object()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| {
                        let v = v.as_str()?.trim();
                        let k = k.trim();
                        (!k.is_empty() && !v.is_empty())
                            .then(|| (if lower { k.to_lowercase() } else { k.to_string() }, v.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    Prefs { devices: strings(&v["mixer"]["devices"], false), rules: strings(&v["mixer"]["appRules"], true) }
}

fn load_prefs() -> Prefs {
    let path = dirs::config_dir().unwrap_or_default().join("opengg/ui-settings.json");
    std::fs::read_to_string(path).map(|s| parse_prefs(&s)).unwrap_or_default()
}

/// The channel a stream belongs on by the saved rules, matched on binary
/// first (stable), then display name (what older rules were keyed by).
/// Only playback channels qualify.
pub fn rule_target<'a>(rules: &'a HashMap<String, String>, binary: &str, name: &str) -> Option<&'a str> {
    let hit = [binary, name]
        .iter()
        .map(|k| k.trim().to_lowercase())
        .filter(|k| !k.is_empty())
        .find_map(|k| rules.get(&k))?;
    OUTPUT_CHANNELS.contains(&hit.as_str()).then_some(hit.as_str())
}

/// Reconciler state that outlives one subscription.
#[derive(Default)]
struct State {
    /// Last seen level per OpenGG sink, restored if the sink is recreated.
    levels: HashMap<String, (f32, bool)>,
}

fn sink_names() -> HashSet<String> {
    sinks::read_live_sink_state().into_keys().collect()
}

/// Run one batch of work. Blocking (subprocesses); call off the runtime.
fn apply(mgr: &SinkManager, state: &Mutex<State>, work: &Work) {
    let prefs = load_prefs();
    if work.relink {
        let mut names = sink_names();
        let missing: Vec<&str> = sinks::SINK_CHANNELS
            .iter()
            .copied()
            .filter(|ch| !names.contains(&format!("OpenGG_{ch}")))
            .collect();
        if !missing.is_empty() && mgr.is_active() {
            tracing::warn!("OpenGG sinks missing ({}) — recreating", missing.join(", "));
            if let Err(e) = mgr.ensure_created() {
                tracing::warn!("Recreating sinks failed: {e}");
            }
            restore_levels(state, &missing);
            names = sink_names();
        }
        if mgr.is_active() {
            relink(&prefs, &names);
        }
    }
    if work.snapshot || work.relink {
        let live = sinks::read_live_sink_state();
        let mut st = state.lock().unwrap();
        for ch in sinks::SINK_CHANNELS {
            let name = format!("OpenGG_{ch}");
            if let Some(l) = live.get(&name) {
                st.levels.insert(name, *l);
            }
        }
    }
    if work.all_streams || !work.new_streams.is_empty() {
        apply_rules(&prefs.rules, if work.all_streams { None } else { Some(&work.new_streams) });
    }
}

fn relink(prefs: &Prefs, sinks: &HashSet<String>) {
    let default = sinks::get_default_sink_name();
    let links = match sinks::parse_pw_links() {
        Ok(l) => l,
        Err(e) => {
            tracing::debug!("Reconcile: cannot read links ({e})");
            return;
        }
    };
    let plan = plan_links(&prefs.devices, default.as_deref(), sinks, &links);
    for (s, d) in &plan.remove {
        let _ = subprocess::command("pw-link").args(["-d", s, d]).output();
        tracing::info!("Reconcile: unlinked {s} → {d}");
    }
    for (s, d) in &plan.add {
        match subprocess::command("pw-link").args([s, d]).output() {
            Ok(o) if o.status.success() => tracing::info!("Reconcile: linked {s} → {d}"),
            Ok(o) => tracing::debug!("Reconcile: pw-link {s} → {d}: {}", String::from_utf8_lossy(&o.stderr).trim()),
            Err(e) => tracing::debug!("Reconcile: pw-link spawn failed: {e}"),
        }
    }
}

fn restore_levels(state: &Mutex<State>, channels: &[&str]) {
    let levels = state.lock().unwrap().levels.clone();
    for ch in channels {
        let name = format!("OpenGG_{ch}");
        let Some(&(vol, muted)) = levels.get(&name) else { continue };
        sinks::write_sink_volume(ch, vol);
        let _ = subprocess::command("pactl")
            .args(["set-sink-mute", &name, if muted { "1" } else { "0" }])
            .output();
    }
}

fn apply_rules(rules: &HashMap<String, String>, only: Option<&[u32]>) {
    if rules.is_empty() {
        return;
    }
    let Ok(streams) = routing::list_streams() else { return };
    for s in streams {
        if only.is_some_and(|ids| !ids.contains(&s.index)) {
            continue;
        }
        let Some(target) = rule_target(rules, &s.binary, &s.app_name) else { continue };
        if s.channel == target {
            continue;
        }
        if let Err(e) = routing::route_stream(s.index, target) {
            tracing::debug!("Reconcile: rule {} → {target} not applied: {e}", s.app_name);
        }
    }
}

/// Start the reconciler. Runs for the life of the daemon.
pub fn spawn(mgr: Arc<SinkManager>) {
    tokio::spawn(async move {
        let state = Arc::new(Mutex::new(State::default()));
        let mut backoff = Duration::from_secs(1);
        loop {
            let started = Instant::now();
            match subscribe_once(&mgr, &state).await {
                Ok(()) => tracing::warn!("pactl subscribe ended (sound server restarted?) — reconnecting"),
                Err(e) => tracing::warn!("pactl subscribe failed: {e} — retrying"),
            }
            if started.elapsed() > Duration::from_secs(60) {
                backoff = Duration::from_secs(1);
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    });
}

async fn run_work(mgr: &Arc<SinkManager>, state: &Arc<Mutex<State>>, work: Work) {
    let (mgr, state) = (Arc::clone(mgr), Arc::clone(state));
    if let Err(e) = tokio::task::spawn_blocking(move || apply(&mgr, &state, &work)).await {
        tracing::error!("Reconcile pass panicked: {e}");
    }
}

async fn subscribe_once(mgr: &Arc<SinkManager>, state: &Arc<Mutex<State>>) -> anyhow::Result<()> {
    let mut child = subprocess::tokio_command("pactl")
        .arg("subscribe")
        .env("LC_ALL", "C")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("no stdout"))?;
    let mut lines = BufReader::new(stdout).lines();

    // Anything may have changed while we were not listening — including a
    // restart that took our sinks with it.
    run_work(mgr, state, Work::full()).await;

    while let Some(line) = lines.next_line().await? {
        let mut events: Vec<Event> = parse_event(&line).into_iter().collect();
        let settle = tokio::time::sleep(SETTLE);
        tokio::pin!(settle);
        let mut ended = false;
        loop {
            tokio::select! {
                _ = &mut settle => break,
                next = lines.next_line() => match next? {
                    Some(l) => events.extend(parse_event(&l)),
                    None => { ended = true; break }
                },
            }
        }
        let work = plan_work(&events);
        if !work.is_empty() {
            run_work(mgr, state, work).await;
        }
        if ended {
            break;
        }
    }
    let _ = child.wait().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn links(items: &[(&str, &str)]) -> HashSet<(String, String)> {
        items.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    #[test]
    fn parses_subscribe_lines() {
        assert_eq!(
            parse_event("Event 'new' on sink-input #42"),
            Some(Event { kind: Kind::New, facility: Facility::SinkInput, index: Some(42) })
        );
        assert_eq!(
            parse_event("Event 'change' on server #4294967295").map(|e| e.facility),
            Some(Facility::Server)
        );
        assert_eq!(parse_event("Event 'remove' on card #3").map(|e| e.facility), Some(Facility::Other));
        assert_eq!(parse_event("garbage"), None);
        assert_eq!(parse_event("Event 'exploded' on sink #1"), None);
    }

    #[test]
    fn plans_work_from_events() {
        let ev = |l: &str| parse_event(l).unwrap();
        let w = plan_work(&[
            ev("Event 'new' on sink-input #7"),
            ev("Event 'new' on sink-input #7"),
            ev("Event 'change' on sink #2"),
        ]);
        assert_eq!(w, Work { relink: false, new_streams: vec![7], all_streams: false, snapshot: true });
        assert!(plan_work(&[ev("Event 'remove' on sink #2")]).relink);
        assert!(plan_work(&[ev("Event 'change' on server #-1")]).relink);
        assert!(plan_work(&[ev("Event 'remove' on module #30")]).relink);
        // Stream volume changes and removals need nothing.
        assert!(plan_work(&[ev("Event 'change' on sink-input #7"), ev("Event 'remove' on sink-input #7")]).is_empty());
    }

    #[test]
    fn target_prefers_a_present_override_then_the_default() {
        let sinks = set(&["hdmi", "usb", "OpenGG_Game"]);
        let mut o = HashMap::new();
        o.insert("Chat".to_string(), "usb".to_string());
        o.insert("Game".to_string(), "unplugged".to_string());
        o.insert("Aux".to_string(), "OpenGG_Game".to_string());
        assert_eq!(target_device("Chat", &o, Some("hdmi"), &sinks), Some("usb"));
        // The saved device is gone: fall back to the default.
        assert_eq!(target_device("Game", &o, Some("hdmi"), &sinks), Some("hdmi"));
        // Never into another OpenGG sink.
        assert_eq!(target_device("Aux", &o, Some("hdmi"), &sinks), Some("hdmi"));
        assert_eq!(target_device("Media", &o, Some("OpenGG_Game"), &sinks), None);
        assert_eq!(target_device("Media", &o, None, &sinks), None);
    }

    #[test]
    fn default_change_moves_links_off_the_old_device() {
        let sinks = set(&["speakers", "headset", "OpenGG_Game", "OpenGG_Chat", "OpenGG_Media", "OpenGG_Aux"]);
        let mut l = vec![];
        for ch in OUTPUT_CHANNELS {
            for p in PORTS {
                l.push((format!("OpenGG_{ch}:monitor_{p}"), format!("speakers:playback_{p}")));
            }
        }
        // A capture reading the Game monitor must be left alone.
        l.push(("OpenGG_Game:monitor_FL".into(), "gsr-capture:input_FL".into()));
        let existing: HashSet<(String, String)> = l.into_iter().collect();
        let plan = plan_links(&HashMap::new(), Some("headset"), &sinks, &existing);
        assert_eq!(plan.add.len(), 8);
        assert!(plan.add.iter().all(|(_, d)| d.starts_with("headset:playback_")));
        assert_eq!(plan.remove.len(), 8);
        assert!(plan.remove.iter().all(|(_, d)| d.starts_with("speakers:playback_")));
    }

    #[test]
    fn settled_graph_plans_nothing() {
        let sinks = set(&["speakers", "usb"]);
        let mut o = HashMap::new();
        o.insert("Chat".to_string(), "usb".to_string());
        let mut l = vec![];
        for ch in OUTPUT_CHANNELS {
            let dev = if *ch == "Chat" { "usb" } else { "speakers" };
            for p in PORTS {
                l.push((format!("OpenGG_{ch}:monitor_{p}"), format!("{dev}:playback_{p}")));
            }
        }
        let existing: HashSet<(String, String)> = l.into_iter().collect();
        assert_eq!(plan_links(&o, Some("speakers"), &sinks, &existing), LinkPlan::default());
    }

    #[test]
    fn monitor_loops_are_always_removed() {
        let existing = links(&[
            ("OpenGG_Game:monitor_FL", "OpenGG_Game:playback_FL"),
            ("OpenGG_Chat:monitor_FR", "OpenGG_Media:playback_FR"),
        ]);
        // Even with no device to link to.
        let plan = plan_links(&HashMap::new(), Some("OpenGG_Game"), &set(&["OpenGG_Game", "OpenGG_Media"]), &existing);
        assert!(plan.add.is_empty());
        assert_eq!(plan.remove.len(), 2);
    }

    #[test]
    fn no_device_means_no_changes() {
        let existing = links(&[("OpenGG_Game:monitor_FL", "speakers:playback_FL")]);
        // "speakers" vanished and there is no default: leave the graph alone.
        assert_eq!(plan_links(&HashMap::new(), None, &set(&[]), &existing), LinkPlan::default());
    }

    #[test]
    fn prefs_and_rules() {
        let p = parse_prefs(
            r#"{"settings":{},"mixer":{"devices":{"Chat":"usb","Game":""},
                "appRules":{"Discord":"Chat","vlc":"Media","weird":"Mic","bad":5}}}"#,
        );
        assert_eq!(p.devices.len(), 1);
        assert_eq!(p.devices["Chat"], "usb");
        assert_eq!(rule_target(&p.rules, "", "discord"), Some("Chat"));
        assert_eq!(rule_target(&p.rules, "VLC", "VLC media player"), Some("Media"));
        // Binary wins over name.
        assert_eq!(rule_target(&p.rules, "vlc", "Discord"), Some("Media"));
        // Mic is not a playback channel.
        assert_eq!(rule_target(&p.rules, "weird", ""), None);
        assert_eq!(rule_target(&p.rules, "firefox", "Firefox"), None);
        assert_eq!(parse_prefs("not json"), Prefs::default());
    }

    #[test]
    fn sink_levels_from_json() {
        let json = br#"[{"name":"OpenGG_Game","mute":false,
            "volume":{"front-left":{"value":39322,"value_percent":"60%","db":"-13.31 dB"},
                      "front-right":{"value":39322,"value_percent":"60%","db":"-13.31 dB"}}},
            {"name":"OpenGG_Chat","mute":true,"volume":{"mono":{"value_percent":"150%"}}},
            {"name":"broken"}]"#;
        let m = sinks::parse_sink_levels(json);
        assert_eq!(m.len(), 2);
        assert!((m["OpenGG_Game"].0 - 0.6).abs() < 1e-6 && !m["OpenGG_Game"].1);
        assert_eq!(m["OpenGG_Chat"], (1.5, true));
        assert!(sinks::parse_sink_levels(b"nope").is_empty());
    }
}
