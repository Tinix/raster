//! Buffer: ropey-backed text plus the file it came from.

use anyhow::Result;
use ropey::Rope;
use std::{fs, ops::Range, path::PathBuf};

#[derive(Debug, Default)]
pub struct Buffer {
    pub text: Rope,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    /// Identifies the current text; changes on every edit and never repeats.
    version: u64,
    /// Last version handed out by `touch`.
    last_version: u64,
    /// Version that was last written to disk.
    saved_version: u64,
}

impl Buffer {
    pub fn open(path: Option<String>) -> Result<Self> {
        let Some(p) = path.map(PathBuf::from) else {
            return Ok(Self::default());
        };
        let text = if p.exists() {
            let mut s = fs::read_to_string(&p)?;
            // Like Vim, the final newline terminates the last line instead of
            // starting an empty one; `save` writes it back.
            if s.ends_with('\n') {
                s.pop();
                if s.ends_with('\r') {
                    s.pop();
                }
            }
            Rope::from_str(&s)
        } else {
            Rope::new()
        };
        Ok(Self {
            text,
            path: Some(p),
            ..Self::default()
        })
    }

    pub fn save(&mut self) -> Result<()> {
        let Some(p) = &self.path else {
            anyhow::bail!("no file name");
        };
        let mut out = self.text.to_string();
        if !out.is_empty() {
            out.push('\n');
        }
        fs::write(p, out)?;
        self.saved_version = self.version;
        self.dirty = false;
        Ok(())
    }

    /// Identifies the current text: equal versions mean equal text.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Record that the text changed.
    fn touch(&mut self) {
        self.last_version += 1;
        self.version = self.last_version;
        self.dirty = true;
    }

    /// Put back text from an earlier `version` (used by undo/redo).
    pub fn restore(&mut self, text: Rope, version: u64) {
        self.text = text;
        self.version = version;
        self.dirty = version != self.saved_version;
    }

    /// Insert `s` at char index `idx`.
    pub fn insert(&mut self, idx: usize, s: &str) {
        if s.is_empty() {
            return;
        }
        self.text.insert(idx, s);
        self.touch();
    }

    /// Remove the chars in `range` and return them.
    pub fn remove(&mut self, range: Range<usize>) -> String {
        if range.is_empty() {
            return String::new();
        }
        let removed = self.text.slice(range.clone()).to_string();
        self.text.remove(range);
        self.touch();
        removed
    }

    /// Text of the chars in `range`.
    pub fn slice(&self, range: Range<usize>) -> String {
        self.text.slice(range).to_string()
    }

    /// Number of chars in the buffer.
    pub fn len_chars(&self) -> usize {
        self.text.len_chars()
    }

    /// Char at `idx`, or `None` past the end.
    pub fn char_at(&self, idx: usize) -> Option<char> {
        (idx < self.text.len_chars()).then(|| self.text.char(idx))
    }

    /// Char index of `(row, col)`.
    pub fn pos_to_idx(&self, row: usize, col: usize) -> usize {
        self.text.line_to_char(row) + col
    }

    /// `(row, col)` of char index `idx`.
    pub fn idx_to_pos(&self, idx: usize) -> (usize, usize) {
        let row = self.text.char_to_line(idx);
        (row, idx - self.text.line_to_char(row))
    }

    /// Char index just past the last char of `row` (before its newline).
    pub fn line_end(&self, row: usize) -> usize {
        self.text.line_to_char(row) + self.line_len(row)
    }

    /// Column of the first non-blank char of `row` (`line_len` if none).
    pub fn first_non_blank(&self, row: usize) -> usize {
        self.text
            .line(row)
            .chars()
            .take(self.line_len(row))
            .position(|c| !c.is_whitespace())
            .unwrap_or(self.line_len(row))
    }

    pub fn line_count(&self) -> usize {
        self.text.len_lines()
    }

    pub fn line_len(&self, row: usize) -> usize {
        let line = self.text.line(row);
        let len = line.len_chars();
        if len > 0 && line.char(len - 1) == '\n' {
            len - 1
        } else {
            len
        }
    }

    pub fn insert_char(&mut self, row: usize, col: usize, ch: char) {
        let idx = self.text.line_to_char(row) + col;
        self.text.insert_char(idx, ch);
        self.touch();
    }

    pub fn backspace(&mut self, row: usize, col: usize) -> (usize, usize) {
        let idx = self.text.line_to_char(row) + col;
        if idx == 0 {
            return (row, col);
        }
        let new = if col == 0 {
            (row - 1, self.line_len(row - 1))
        } else {
            (row, col - 1)
        };
        self.text.remove(idx - 1..idx);
        self.touch();
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_without_path_is_empty() {
        let b = Buffer::open(None).unwrap();
        assert_eq!(b.text.len_chars(), 0);
        assert!(b.path.is_none());
        assert!(!b.dirty);
    }

    #[test]
    fn save_and_reopen_roundtrip() {
        let path = std::env::temp_dir().join("raster_buffer_roundtrip.txt");
        let mut b = Buffer::open(Some(path.display().to_string())).unwrap();
        b.text.insert(0, "hello\nworld");
        b.save().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello\nworld\n");

        let reopened = Buffer::open(Some(path.display().to_string())).unwrap();
        assert_eq!(reopened.text.to_string(), "hello\nworld");
        assert_eq!(reopened.line_count(), 2);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn line_len_ignores_newline() {
        let mut b = Buffer::default();
        b.text.insert(0, "hola\nmundo");
        assert_eq!(b.line_count(), 2);
        assert_eq!(b.line_len(0), 4);
        assert_eq!(b.line_len(1), 5);
    }

    #[test]
    fn insert_and_backspace() {
        let mut b = Buffer::default();
        b.insert_char(0, 0, 'a');
        b.insert_char(0, 1, '\n');
        b.insert_char(1, 0, 'b');
        assert_eq!(b.text.to_string(), "a\nb");
        assert!(b.dirty);

        assert_eq!(b.backspace(1, 0), (0, 1));
        assert_eq!(b.text.to_string(), "ab");
    }

    #[test]
    fn char_index_edits() {
        let mut b = Buffer::default();
        b.insert(0, "hello world");
        assert_eq!(b.remove(5..11), " world");
        assert_eq!(b.text.to_string(), "hello");
        assert_eq!(b.remove(2..2), "");
        assert_eq!(b.slice(1..3), "el");
        assert_eq!(b.char_at(4), Some('o'));
        assert_eq!(b.char_at(5), None);
    }

    #[test]
    fn positions_and_line_helpers() {
        let mut b = Buffer::default();
        b.insert(0, "ab\n  cd");
        assert_eq!(b.pos_to_idx(1, 2), 5);
        assert_eq!(b.idx_to_pos(5), (1, 2));
        assert_eq!(b.line_end(0), 2);
        assert_eq!(b.line_end(1), 7);
        assert_eq!(b.first_non_blank(1), 2);
        assert_eq!(b.first_non_blank(0), 0);
    }

    #[test]
    fn versions_track_changes_and_dirty() {
        let mut b = Buffer::default();
        let v0 = b.version();
        let t0 = b.text.clone();
        b.insert(0, "x");
        assert_ne!(b.version(), v0);
        assert!(b.dirty);
        // Restoring the saved version clears the dirty flag.
        b.restore(t0, v0);
        assert!(!b.dirty);
        // A new edit never reuses an old version.
        b.insert(0, "y");
        assert_ne!(b.version(), v0);
    }

    #[test]
    fn backspace_at_start_does_nothing() {
        let mut b = Buffer::default();
        assert_eq!(b.backspace(0, 0), (0, 0));
    }
}
