//! Trailing-edge coalescing for rapid per-key updates (volume slider drags).
//!
//! The previous debounce was leading-edge only: it applied the first value and
//! silently DROPPED any value arriving within the next 100 ms. A slider drag
//! ends with exactly such a value, so the sink was left at a stale step while
//! the UI showed the release position.
//!
//! This one never loses the last value: the first update after a quiet period
//! is applied immediately (the slider feels live), values arriving inside the
//! interval overwrite a single pending slot, and a short-lived flusher thread
//! applies whatever is pending once the interval has elapsed. At most one apply
//! per key per interval, and the final value always wins.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type ApplyFn = dyn Fn(&str, f32) -> Result<(), String> + Send + Sync;

#[derive(Default)]
struct Slot {
    last_apply: Option<Instant>,
    pending: Option<f32>,
    /// A flusher thread is alive for this key.
    flusher: bool,
}

struct Inner {
    interval: Duration,
    slots: Mutex<HashMap<String, Slot>>,
    apply: Box<ApplyFn>,
}

#[derive(Clone)]
pub struct Coalescer {
    inner: Arc<Inner>,
}

impl Coalescer {
    pub fn new(
        interval: Duration,
        apply: impl Fn(&str, f32) -> Result<(), String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                interval,
                slots: Mutex::new(HashMap::new()),
                apply: Box::new(apply),
            }),
        }
    }

    /// Apply now if the key has been quiet for the interval (returning the
    /// apply result), otherwise remember `value` for the flusher and return Ok.
    /// Failures of a deferred apply are logged by the flusher (rate-limited).
    pub fn submit(&self, key: &str, value: f32) -> Result<(), String> {
        let mut slots = lock(&self.inner.slots);
        let slot = slots.entry(key.to_string()).or_default();
        let now = Instant::now();
        let quiet = slot
            .last_apply
            .is_none_or(|t| now.duration_since(t) >= self.inner.interval);

        if quiet && !slot.flusher {
            slot.last_apply = Some(now);
            slot.pending = None;
            drop(slots);
            return (self.inner.apply)(key, value);
        }

        slot.pending = Some(value);
        if !slot.flusher {
            slot.flusher = true;
            let inner = Arc::clone(&self.inner);
            let key = key.to_string();
            std::thread::spawn(move || flush(inner, key));
        }
        Ok(())
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match m.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    }
}

fn flush(inner: Arc<Inner>, key: String) {
    loop {
        // Sleep out whatever is left of the interval since the last apply.
        let wait = {
            let slots = lock(&inner.slots);
            slots
                .get(&key)
                .and_then(|s| s.last_apply)
                .map(|t| inner.interval.saturating_sub(t.elapsed()))
                .unwrap_or_default()
        };
        std::thread::sleep(wait);

        let value = {
            let mut slots = lock(&inner.slots);
            let slot = slots.entry(key.clone()).or_default();
            match slot.pending.take() {
                Some(v) => {
                    slot.last_apply = Some(Instant::now());
                    v
                }
                None => {
                    // Nothing arrived during the last interval: retire.
                    slot.flusher = false;
                    return;
                }
            }
        };
        if let Err(e) = (inner.apply)(&key, value) {
            if crate::statelog::changed(&format!("audio.deferred-apply.{key}"), &e) {
                tracing::warn!("Deferred volume apply for {key} failed: {e}");
            }
        }
        // Loop: one more interval to catch a value that arrived mid-apply.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorder() -> (Arc<Mutex<Vec<f32>>>, Coalescer) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s = Arc::clone(&seen);
        let c = Coalescer::new(Duration::from_millis(40), move |_, v| {
            s.lock().unwrap().push(v);
            Ok(())
        });
        (seen, c)
    }

    #[test]
    fn last_value_of_a_burst_is_always_applied() {
        let (seen, c) = recorder();
        for i in 1..=30 {
            c.submit("Game", i as f32).unwrap();
        }
        std::thread::sleep(Duration::from_millis(250));
        let seen = seen.lock().unwrap();
        assert_eq!(seen.first(), Some(&1.0), "first value applies immediately");
        assert_eq!(seen.last(), Some(&30.0), "final value must not be dropped: {seen:?}");
        assert!(seen.len() < 10, "burst should be coalesced, got {seen:?}");
    }

    #[test]
    fn keys_do_not_interfere() {
        let (seen, c) = recorder();
        c.submit("Game", 1.0).unwrap();
        c.submit("Chat", 2.0).unwrap();
        assert_eq!(seen.lock().unwrap().len(), 2, "both channels apply immediately");
    }

    #[test]
    fn leading_apply_error_is_returned_to_the_caller() {
        let c = Coalescer::new(Duration::from_millis(40), |_, _| Err("pactl down".into()));
        assert_eq!(c.submit("Game", 0.5), Err("pactl down".into()));
    }

    #[test]
    fn quiet_period_applies_immediately_again() {
        let (seen, c) = recorder();
        c.submit("Game", 1.0).unwrap();
        std::thread::sleep(Duration::from_millis(120));
        c.submit("Game", 2.0).unwrap();
        assert_eq!(*seen.lock().unwrap(), vec![1.0, 2.0]);
    }
}
