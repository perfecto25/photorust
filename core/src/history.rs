//! Undo/redo.
//!
//! Photoshop's History panel is a linear list of states with a movable cursor:
//! stepping back and then performing a new action discards everything after the
//! cursor. That is exactly what this models.
//!
//! States are full snapshots of the layer stack **and the canvas size**. The
//! size matters because Crop and Canvas Size change it: restoring the pixels
//! without the dimensions they were taken at would leave the document
//! inconsistent. That is memory-hungry but
//! simple and always correct; the stack is bounded by both a step count and a
//! byte budget, and evicts from the oldest end. Replacing snapshots with tile
//! deltas is the obvious later optimisation.

use crate::layer::LayerStack;

/// One entry in the History panel.
pub struct HistoryState {
    /// Label shown in the panel, e.g. "Brush Tool" or "New Layer".
    pub name: String,
    pub stack: LayerStack,
    /// Canvas size when the state was recorded.
    pub size: (u32, u32),
}

impl HistoryState {
    fn byte_size(&self) -> usize {
        self.stack.byte_size() + self.name.len()
    }
}

/// A bounded, linear undo history.
pub struct History {
    states: Vec<HistoryState>,
    /// Index of the state the document currently reflects.
    cursor: usize,
    /// Maximum entries, matching Photoshop's "History States" preference.
    max_states: usize,
    /// Soft cap on total snapshot memory.
    max_bytes: usize,
    /// When true, the next `push_coalescing` acts as a plain `push`.
    coalesce_sealed: bool,
}

impl History {
    /// Photoshop's factory default is 20 history states.
    pub const DEFAULT_MAX_STATES: usize = 20;
    /// 1 GiB of snapshots before older states start being dropped.
    pub const DEFAULT_MAX_BYTES: usize = 1024 * 1024 * 1024;

    /// Create a history seeded with the document's opening state.
    pub fn new(initial: LayerStack, size: (u32, u32)) -> Self {
        Self {
            states: vec![HistoryState {
                name: "Open".to_string(),
                stack: initial,
                size,
            }],
            cursor: 0,
            max_states: Self::DEFAULT_MAX_STATES,
            max_bytes: Self::DEFAULT_MAX_BYTES,
            coalesce_sealed: false,
        }
    }

    pub fn set_max_states(&mut self, n: usize) {
        // Always keep at least the current state.
        self.max_states = n.max(1);
        self.evict();
    }

    pub fn max_states(&self) -> usize {
        self.max_states
    }

    pub fn set_max_bytes(&mut self, n: usize) {
        self.max_bytes = n;
        self.evict();
    }

    /// Number of entries currently in the panel.
    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor + 1 < self.states.len()
    }

    /// Label of the state that [`History::undo`] would step back *from*.
    /// Drives the "Undo Brush Tool" menu text.
    pub fn undo_name(&self) -> Option<&str> {
        if self.can_undo() {
            Some(&self.states[self.cursor].name)
        } else {
            None
        }
    }

    /// Label of the state [`History::redo`] would step forward to.
    pub fn redo_name(&self) -> Option<&str> {
        if self.can_redo() {
            Some(&self.states[self.cursor + 1].name)
        } else {
            None
        }
    }

    pub fn state_names(&self) -> Vec<&str> {
        self.states.iter().map(|s| s.name.as_str()).collect()
    }

    /// Record a new state on top of the current cursor position.
    ///
    /// Anything after the cursor is discarded — the redo branch is lost, which
    /// is what Photoshop does.
    pub fn push(&mut self, name: impl Into<String>, stack: LayerStack, size: (u32, u32)) {
        self.states.truncate(self.cursor + 1);
        self.states.push(HistoryState {
            name: name.into(),
            stack,
            size,
        });
        self.cursor = self.states.len() - 1;
        self.evict();
    }

    /// Like [`push`](Self::push), but if the entry at the cursor already has
    /// the same name, replace it instead of adding a new state. This is how
    /// Photoshop coalesces rapid slider-driven changes (opacity, fill) into a
    /// single undo step.
    pub fn push_coalescing(
        &mut self,
        name: impl Into<String>,
        stack: LayerStack,
        size: (u32, u32),
    ) {
        let name = name.into();
        if !self.coalesce_sealed
            && self.cursor > 0
            && self.states[self.cursor].name == name
        {
            self.states[self.cursor].stack = stack;
            self.states[self.cursor].size = size;
        } else {
            self.coalesce_sealed = false;
            self.push(name, stack, size);
        }
    }

    pub fn seal_coalescing(&mut self) {
        self.coalesce_sealed = true;
    }

    /// Step back one state, returning the state to restore.
    pub fn undo(&mut self) -> Option<&HistoryState> {
        if !self.can_undo() {
            return None;
        }
        self.cursor -= 1;
        Some(&self.states[self.cursor])
    }

    /// Step forward one state.
    pub fn redo(&mut self) -> Option<&HistoryState> {
        if !self.can_redo() {
            return None;
        }
        self.cursor += 1;
        Some(&self.states[self.cursor])
    }

    /// Jump directly to a state, as clicking a row in the History panel does.
    pub fn jump_to(&mut self, index: usize) -> Option<&HistoryState> {
        if index >= self.states.len() {
            return None;
        }
        self.cursor = index;
        Some(&self.states[self.cursor])
    }

    /// The stack at the cursor.
    pub fn current(&self) -> &LayerStack {
        &self.states[self.cursor].stack
    }

    /// The canvas size at the cursor.
    pub fn current_size(&self) -> (u32, u32) {
        self.states[self.cursor].size
    }

    /// Total bytes held by all snapshots.
    pub fn byte_size(&self) -> usize {
        self.states.iter().map(|s| s.byte_size()).sum()
    }

    /// Drop the oldest states until both limits are satisfied.
    ///
    /// The state at the cursor is never evicted — without it the document
    /// would have nothing to restore from.
    fn evict(&mut self) {
        while self.states.len() > self.max_states && self.cursor > 0 {
            self.states.remove(0);
            self.cursor -= 1;
        }
        while self.byte_size() > self.max_bytes && self.states.len() > 1 && self.cursor > 0 {
            self.states.remove(0);
            self.cursor -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Rgba8;
    use crate::layer::Layer;

    fn stack_with_name(name: &str) -> LayerStack {
        let mut s = LayerStack::new();
        let id = s.allocate_id();
        s.push(Layer::new_filled(id, name, 4, 4, Rgba8::WHITE));
        s
    }

    #[test]
    fn starts_with_a_single_open_state() {
        let h = History::new(LayerStack::new(), (16, 16));
        assert_eq!(h.len(), 1);
        assert_eq!(h.cursor(), 0);
        assert!(!h.can_undo());
        assert!(!h.can_redo());
        assert_eq!(h.state_names(), vec!["Open"]);
    }

    #[test]
    fn push_advances_the_cursor() {
        let mut h = History::new(LayerStack::new(), (16, 16));
        h.push("New Layer", stack_with_name("a"), (16, 16));
        assert_eq!(h.len(), 2);
        assert_eq!(h.cursor(), 1);
        assert!(h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn undo_then_redo_returns_to_the_same_state() {
        let mut h = History::new(LayerStack::new(), (16, 16));
        h.push("New Layer", stack_with_name("a"), (16, 16));

        assert_eq!(h.undo().unwrap().stack.len(), 0);
        assert_eq!(h.cursor(), 0);
        assert!(h.can_redo());

        assert_eq!(h.redo().unwrap().stack.len(), 1);
        assert_eq!(h.cursor(), 1);
    }

    #[test]
    fn undo_at_the_start_returns_none() {
        let mut h = History::new(LayerStack::new(), (16, 16));
        assert!(h.undo().is_none());
        assert_eq!(h.cursor(), 0);
    }

    #[test]
    fn redo_at_the_end_returns_none() {
        let mut h = History::new(LayerStack::new(), (16, 16));
        h.push("x", stack_with_name("a"), (16, 16));
        assert!(h.redo().is_none());
    }

    #[test]
    fn pushing_after_undo_discards_the_redo_branch() {
        let mut h = History::new(LayerStack::new(), (16, 16));
        h.push("first", stack_with_name("a"), (16, 16));
        h.push("second", stack_with_name("b"), (16, 16));
        h.undo();
        assert!(h.can_redo());

        h.push("third", stack_with_name("c"), (16, 16));
        assert!(!h.can_redo(), "redo branch survived a new action");
        assert_eq!(h.state_names(), vec!["Open", "first", "third"]);
    }

    #[test]
    fn undo_and_redo_names_describe_the_right_steps() {
        let mut h = History::new(LayerStack::new(), (16, 16));
        assert_eq!(h.undo_name(), None);

        h.push("Brush Tool", stack_with_name("a"), (16, 16));
        assert_eq!(h.undo_name(), Some("Brush Tool"));
        assert_eq!(h.redo_name(), None);

        h.undo();
        assert_eq!(h.redo_name(), Some("Brush Tool"));
        assert_eq!(h.undo_name(), None);
    }

    #[test]
    fn jump_to_moves_the_cursor_anywhere() {
        let mut h = History::new(LayerStack::new(), (16, 16));
        h.push("a", stack_with_name("a"), (16, 16));
        h.push("b", stack_with_name("b"), (16, 16));

        assert!(h.jump_to(0).is_some());
        assert_eq!(h.cursor(), 0);
        assert!(h.jump_to(2).is_some());
        assert_eq!(h.cursor(), 2);
        assert!(h.jump_to(99).is_none());
        assert_eq!(h.cursor(), 2, "failed jump moved the cursor");
    }

    #[test]
    fn state_count_limit_evicts_the_oldest() {
        let mut h = History::new(LayerStack::new(), (16, 16));
        h.set_max_states(3);
        for i in 0..5 {
            h.push(format!("s{}", i), stack_with_name("x"), (16, 16));
        }
        assert_eq!(h.len(), 3);
        // The oldest entries, including "Open", were dropped.
        assert_eq!(h.state_names(), vec!["s2", "s3", "s4"]);
        assert_eq!(h.cursor(), 2);
    }

    #[test]
    fn eviction_keeps_the_cursor_pointing_at_the_same_state() {
        let mut h = History::new(LayerStack::new(), (16, 16));
        h.set_max_states(2);
        h.push("a", stack_with_name("a"), (16, 16));
        h.push("b", stack_with_name("b"), (16, 16));
        // The cursor must still address the newest state.
        assert_eq!(h.state_names()[h.cursor()], "b");
    }

    #[test]
    fn max_states_is_never_below_one() {
        let mut h = History::new(LayerStack::new(), (16, 16));
        h.set_max_states(0);
        assert_eq!(h.max_states(), 1);
        assert!(!h.is_empty());
    }

    #[test]
    fn byte_budget_evicts_old_snapshots() {
        let mut h = History::new(LayerStack::new(), (16, 16));
        // Each pushed stack holds one 64x64 RGBA layer = 16 KiB.
        let big = || {
            let mut s = LayerStack::new();
            let id = s.allocate_id();
            s.push(Layer::new_filled(id, "big", 64, 64, Rgba8::WHITE));
            s
        };
        h.set_max_bytes(40 * 1024);
        for _ in 0..6 {
            h.push("big", big(), (16, 16));
        }
        assert!(h.byte_size() <= 40 * 1024, "budget exceeded: {}", h.byte_size());
        assert!(h.len() >= 1);
    }

    #[test]
    fn current_reflects_the_cursor() {
        let mut h = History::new(LayerStack::new(), (16, 16));
        h.push("a", stack_with_name("a"), (16, 16));
        assert_eq!(h.current().len(), 1);
        h.undo();
        assert_eq!(h.current().len(), 0);
    }
}
