//! Buffer: ropey-backed text plus the file it came from.

use anyhow::Result;
use ropey::Rope;
use std::{fs, path::PathBuf};

/// A text buffer, optionally tied to a file on disk.
#[derive(Debug, Default)]
pub struct Buffer {
    pub text: Rope,
    pub path: Option<PathBuf>,
    pub dirty: bool,
}

impl Buffer {
    /// Opens `path`, or starts empty if the file doesn't exist yet.
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
        /// Writes the buffer back to its file.
    pub fn save(&mut self) -> Result<()> {
        let Some(p) = &self.path else {
            anyhow::bail!("no file name");
        };
        fs::write(p, self.text.to_string())?;
        self.dirty = false;
        Ok(())
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
}
