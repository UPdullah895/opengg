//! "Log on change, not on every poll."
//!
//! Several loops (stream scan every 3 s, GetDevices polls, the audio
//! reconciler) hit the same failure over and over. Logging each repeat buries
//! whatever else is wrong and makes pasted logs useless. `changed(key, state)`
//! answers "is this a different state than last time for `key`?" so callers
//! log the transition (ok -> failing, failing -> recovered, one failure reason
//! -> another) and stay quiet in between.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static STATES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

/// True when `state` differs from the last state recorded for `key` (always
/// true the first time). Records `state` as the new current one.
pub fn changed(key: &str, state: &str) -> bool {
    let map = STATES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = match map.lock() {
        Ok(m) => m,
        Err(poisoned) => poisoned.into_inner(),
    };
    match map.get(key) {
        Some(prev) if prev == state => false,
        _ => {
            map.insert(key.to_string(), state.to_string());
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_only_on_transitions() {
        assert!(changed("t.statelog.a", "ok"));
        assert!(!changed("t.statelog.a", "ok"));
        assert!(changed("t.statelog.a", "err: boom"));
        assert!(!changed("t.statelog.a", "err: boom"));
        assert!(changed("t.statelog.a", "ok"));
        // Keys are independent.
        assert!(changed("t.statelog.b", "ok"));
    }
}
