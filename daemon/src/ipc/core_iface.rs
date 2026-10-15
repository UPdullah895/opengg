//! D-Bus interface: org.opengg.Daemon.Core
//!
//! Served at `/org/opengg/Daemon`. Lets a client ask the running daemon who it
//! is and whether it is healthy, instead of discovering a stale daemon (kept
//! alive across a package upgrade) through an opaque `UnknownMethod`.
//!
//! * `GetVersion() -> s`: the daemon's package version, e.g. `"0.2.1"`.
//! * `GetApiVersion() -> u`: integer bumped only for breaking D-Bus changes;
//!   compare it against what the client expects.
//! * `GetDiagnostics() -> s`: JSON with version, uptime, config problem, module
//!   states, tool paths and missing tools with install hints, pipewire-pulse
//!   reachability, which OpenGG_* sinks exist or are missing, audio health
//!   (links, mic loopback), hotkey state and `/dev/input` readability.
//!
//! The same JSON is printed by `openggd --diagnose` without starting the
//! service. All three methods are read-only.

use zbus::interface;

pub struct CoreInterface;

#[interface(name = "org.opengg.Daemon.Core")]
impl CoreInterface {
    async fn get_version(&self) -> String {
        crate::diag::version().to_string()
    }

    async fn get_api_version(&self) -> u32 {
        crate::diag::API_VERSION
    }

    async fn get_diagnostics(&self) -> String {
        // Spawns pactl: keep it off the async workers.
        tokio::task::spawn_blocking(crate::diag::collect_pretty)
            .await
            .unwrap_or_else(|e| format!("{{\"error\":\"diagnostics task failed: {e}\"}}"))
    }
}
