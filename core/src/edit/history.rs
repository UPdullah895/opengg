//! Snapshot-based undo/redo.
//!
//! The edit project is small (a handful of parts and levels), so whole-value
//! snapshots are far simpler and less bug-prone than inverse operations: undo
//! is just "move the cursor back". The only subtlety is continuous gestures
//! (slider drags): every tick produces a new project, but the user expects one
//! undo step per drag. [`History::amend`] coalesces consecutive changes of the
//! same `kind` into a single snapshot for exactly that reason.

/// Maximum number of snapshots kept; the oldest is dropped beyond this so a
/// long editing session can't grow memory without bound.
pub const HISTORY_CAP: usize = 200;

/// Linear undo history of whole-value snapshots with a cursor.
#[derive(Debug, Clone)]
pub struct History<T: Clone + PartialEq> {
    snapshots: Vec<T>,
    cursor: usize,
    /// Kind of the most recent `amend`, used to coalesce drag ticks. Cleared
    /// by every other operation so an unrelated edit (or an undo) always
    /// starts a fresh step.
    last_kind: Option<String>,
}

impl<T: Clone + PartialEq> History<T> {
    /// A history whose only (non-undoable) state is `initial`.
    pub fn new(initial: T) -> Self {
        Self {
            snapshots: vec![initial],
            cursor: 0,
            last_kind: None,
        }
    }

    /// The state at the cursor.
    pub fn current(&self) -> &T {
        &self.snapshots[self.cursor]
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor + 1 < self.snapshots.len()
    }

    /// Record a new discrete step. No-op when `v` equals the current state
    /// (so a click that changes nothing doesn't create an empty undo step).
    /// Drops any redo tail.
    pub fn push(&mut self, v: T) {
        self.last_kind = None;
        self.push_inner(v);
    }

    fn push_inner(&mut self, v: T) -> bool {
        if v == *self.current() {
            return false;
        }
        self.snapshots.truncate(self.cursor + 1);
        self.snapshots.push(v);
        if self.snapshots.len() > HISTORY_CAP {
            self.snapshots.remove(0);
        }
        self.cursor = self.snapshots.len() - 1;
        true
    }

    /// Record a step that coalesces with the previous one when it has the
    /// same `kind` (e.g. `"trackLevel:2"` while dragging a fader): the current
    /// snapshot is replaced instead of a new one being appended, so the whole
    /// drag undoes in one go back to the pre-drag state.
    pub fn amend(&mut self, kind: &str, v: T) {
        if self.cursor > 0 && self.last_kind.as_deref() == Some(kind) {
            self.snapshots.truncate(self.cursor + 1);
            self.snapshots[self.cursor] = v;
            return;
        }
        // Only remember the kind if a step was actually recorded; otherwise
        // the next tick would overwrite the pre-drag snapshot itself.
        if self.push_inner(v) {
            self.last_kind = Some(kind.to_string());
        } else {
            self.last_kind = None;
        }
    }

    /// Step back; returns the new current state, or `None` at the start.
    pub fn undo(&mut self) -> Option<&T> {
        self.last_kind = None;
        if !self.can_undo() {
            return None;
        }
        self.cursor -= 1;
        Some(self.current())
    }

    /// Step forward; returns the new current state, or `None` at the end.
    pub fn redo(&mut self) -> Option<&T> {
        self.last_kind = None;
        if !self.can_redo() {
            return None;
        }
        self.cursor += 1;
        Some(self.current())
    }

    /// Replace everything with a fresh, non-undoable `initial` (e.g. when a
    /// different clip is opened).
    pub fn reset(&mut self, initial: T) {
        *self = Self::new(initial);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_undo_to_initial() {
        let mut h = History::new(0);
        assert!(!h.can_undo());
        h.push(1);
        h.push(2);
        h.push(2); // no-op
        assert_eq!(h.undo(), Some(&1));
        assert_eq!(h.undo(), Some(&0));
        assert_eq!(h.undo(), None);
        assert_eq!(*h.current(), 0);
        assert_eq!(h.redo(), Some(&1));
        assert_eq!(h.redo(), Some(&2));
        assert_eq!(h.redo(), None);
    }

    #[test]
    fn slider_drag_is_one_step() {
        let mut h = History::new(1.0);
        h.amend("level", 0.4);
        h.amend("level", 0.6);
        assert_eq!(*h.current(), 0.6);
        assert_eq!(h.undo(), Some(&1.0));
        assert!(!h.can_undo());
        // A different kind starts a new step.
        h.redo();
        h.amend("speed", 2.0);
        assert_eq!(h.undo(), Some(&0.6));
    }

    #[test]
    fn amend_after_push_starts_new_step() {
        let mut h = History::new(0);
        h.amend("k", 1);
        h.push(5);
        h.amend("k", 6);
        h.amend("k", 7);
        assert_eq!(h.undo(), Some(&5));
        assert_eq!(h.undo(), Some(&1));
    }

    #[test]
    fn noop_amend_does_not_capture_kind() {
        let mut h = History::new(0);
        h.amend("k", 0); // unchanged, nothing recorded
        h.amend("k", 3); // must append, not overwrite the initial state
        assert_eq!(h.undo(), Some(&0));
    }

    #[test]
    fn new_change_after_undo_drops_redo() {
        let mut h = History::new(0);
        h.push(1);
        h.push(2);
        h.undo();
        h.push(9);
        assert!(!h.can_redo());
        assert_eq!(h.undo(), Some(&1));
        assert_eq!(h.redo(), Some(&9));
    }

    #[test]
    fn capped() {
        let mut h = History::new(0);
        for i in 1..=500 {
            h.push(i);
        }
        let mut steps = 0;
        while h.undo().is_some() {
            steps += 1;
        }
        assert_eq!(steps, HISTORY_CAP - 1);
        assert_eq!(*h.current(), 500 - (HISTORY_CAP as i32 - 1));
    }
}
