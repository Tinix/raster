//! Buffer: ropey-backed text plus the file it came from.

use anyhow::Result;
use ropey::Rope;
use std::{fs, path::PathBuf};

#[derive(Debug, Default)]
pub struct Buffer {
    pub text: Rope,
    pub path: Option<PathBuf>,
    pub dirty: bool,
}

impl Buffer {
    pub fn open(path: Option<String>) -> Result<Self> {
        let Some(p) = path.map(PathBuf::from) else {
            return Ok(Self::default());
        };
        let text = if p.exists() {
            Rope::from_str(&fs::read_to_string(&p)?)
        } else {
            Rope::new()
        };
        Ok(Self { text, path: Some(p), dirty: false })
    }

    pub fn save(&mut self) -> Result<()> {
        let Some(p) = &self.path else {
            anyhow::bail!("no file name");
        };
        fs::write(p, self.text.to_string())?;
        self.dirty = false;
        Ok(())
    }


    pub fn line_count(&self) -> usize {
        self.text.len_lines()
    }


    pub fn line_len(&self, row: usize) -> usize {
        let line = self.text.line(row);
        let len = line.len_chars();
        if len > 0 && line.char(len - 1) == '\n' { len - 1 } else { len }
    }

    pub fn insert_char(&mut self, row: usize, col: usize, ch: char) {
        let idx = self.text.line_to_char(row) + col;
        self.text.insert_char(idx, ch);
        self.dirty = true;
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
        self.dirty = true;
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
        b.text.insert(0, "hello\nworld\n");
        b.save().unwrap();

        let reopened = Buffer::open(Some(path.display().to_string())).unwrap();
        assert_eq!(reopened.text.to_string(), "hello\nworld\n");
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
    fn backspace_at_start_does_nothing() {
        let mut b = Buffer::default();
        assert_eq!(b.backspace(0, 0), (0, 0));
    }
}
