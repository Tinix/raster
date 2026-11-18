//! History: snapshot-based undo and redo.
//!
//! Before a change the editor calls [`History::checkpoint`], which saves the
//! whole text (ropey clones share structure, so this is cheap) plus the
//! cursor. One checkpoint is one undo step, so an Insert session, or an
//! operator like `cw` followed by typing, undoes in one go.

use crate::buffer::Buffer;
use ropey::Rope;

/// Cursor position as `(row, col)`.
pub type Cursor = (usize, usize);

#[derive(Debug, Clone)]
struct State {
    text: Rope,
    version: u64,
    cursor: Cursor,
}

#[derive(Debug, Default)]
pub struct History {
    undo: Vec<State>,
    redo: Vec<State>,
}

impl History {
    /// Save the state before a change. Starts a new undo step and drops redo.
    pub fn checkpoint(&mut self, buffer: &Buffer, cursor: Cursor) {
        self.undo.push(State {
            text: buffer.text.clone(),
            version: buffer.version(),
            cursor,
        });
        self.redo.clear();
    }

    /// Drop the last checkpoint if nothing changed since it was taken.
    pub fn discard_if_unchanged(&mut self, buffer: &Buffer) {
        if self
            .undo
            .last()
            .is_some_and(|s| s.version == buffer.version())
        {
            self.undo.pop();
        }
    }

    /// Undo one step. Returns the cursor to restore, or `None` if there is
    /// nothing to undo.
    pub fn undo(&mut self, buffer: &mut Buffer) -> Option<Cursor> {
        let state = self.undo.pop()?;
        self.redo.push(State {
            text: buffer.text.clone(),
            version: buffer.version(),
            cursor: state.cursor,
        });
        buffer.restore(state.text, state.version);
        Some(state.cursor)
    }

    /// Redo one step. Returns the cursor to restore, or `None` if there is
    /// nothing to redo.
    pub fn redo(&mut self, buffer: &mut Buffer) -> Option<Cursor> {
        let state = self.redo.pop()?;
        self.undo.push(State {
            text: buffer.text.clone(),
            version: buffer.version(),
            cursor: state.cursor,
        });
        buffer.restore(state.text, state.version);
        Some(state.cursor)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(b: &Buffer) -> String {
        b.text.to_string()
    }

    #[test]
    fn undo_and_redo_one_change() {
        let mut b = Buffer::default();
        let mut h = History::default();
        h.checkpoint(&b, (0, 0));
        b.insert(0, "abc");

        assert_eq!(h.undo(&mut b), Some((0, 0)));
        assert_eq!(text(&b), "");
        assert!(!b.dirty);
        assert!(h.undo(&mut b).is_none());

        assert_eq!(h.redo(&mut b), Some((0, 0)));
        assert_eq!(text(&b), "abc");
        assert!(b.dirty);
        assert!(h.redo(&mut b).is_none());
    }

    #[test]
    fn one_checkpoint_groups_many_edits() {
        let mut b = Buffer::default();
        let mut h = History::default();
        h.checkpoint(&b, (0, 0));
        b.insert_char(0, 0, 'a');
        b.insert_char(0, 1, 'b');
        b.insert_char(0, 2, 'c');
        h.undo(&mut b);
        assert_eq!(text(&b), "");
    }

    #[test]
    fn new_change_clears_redo() {
        let mut b = Buffer::default();
        let mut h = History::default();
        h.checkpoint(&b, (0, 0));
        b.insert(0, "a");
        h.undo(&mut b);
        assert!(h.can_redo());
        h.checkpoint(&b, (0, 0));
        b.insert(0, "b");
        assert!(!h.can_redo());
    }

    #[test]
    fn unchanged_checkpoint_is_discarded() {
        let mut b = Buffer::default();
        let mut h = History::default();
        h.checkpoint(&b, (0, 0));
        h.discard_if_unchanged(&b);
        assert!(!h.can_undo());

        h.checkpoint(&b, (0, 0));
        b.insert(0, "x");
        h.discard_if_unchanged(&b);
        assert!(h.can_undo());
    }

    #[test]
    fn multiple_steps_undo_in_order() {
        let mut b = Buffer::default();
        let mut h = History::default();
        h.checkpoint(&b, (0, 0));
        b.insert(0, "a");
        h.checkpoint(&b, (0, 1));
        b.insert(1, "b");

        assert_eq!(h.undo(&mut b), Some((0, 1)));
        assert_eq!(text(&b), "a");
        assert_eq!(h.undo(&mut b), Some((0, 0)));
        assert_eq!(text(&b), "");
        h.redo(&mut b);
        h.redo(&mut b);
        assert_eq!(text(&b), "ab");
    }
}
