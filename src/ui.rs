//! UI: widgets drawn around the buffer.

use crate::mode::Mode;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Everything the status bar shows.
#[derive(Debug, Clone, Copy)]
pub struct Status<'a> {
    pub mode: Mode,
    pub file_name: Option<&'a str>,
    pub dirty: bool,
    /// Zero-based cursor row.
    pub row: usize,
    /// Zero-based cursor column.
    pub col: usize,
}

/// Background color of the mode pill.
pub fn mode_color(mode: Mode) -> Color {
    match mode {
        Mode::Normal => Color::Magenta,
        Mode::Insert => Color::Green,
        Mode::Command => Color::Yellow,
    }
}

/// Build the status bar line for a terminal `width` columns wide:
/// `[ MODE ] name [+]` on the left, `row:col` (one-based) on the right.
pub fn status_line(status: Status, width: u16) -> Line<'static> {
    let pill = Span::styled(
        format!(" {} ", status.mode.label()),
        Style::default()
            .fg(Color::Black)
            .bg(mode_color(status.mode))
            .add_modifier(Modifier::BOLD),
    );
    let name = format!(" {}", status.file_name.unwrap_or("[No Name]"));
    let dirty = if status.dirty { " [+]" } else { "" };
    let pos = format!("{}:{} ", status.row + 1, status.col + 1);

    let used = pill.width() + name.chars().count() + dirty.chars().count() + pos.chars().count();
    let gap = (width as usize).saturating_sub(used);

    let bar = Style::default().bg(Color::DarkGray).fg(Color::White);
    Line::from(vec![
        pill,
        Span::styled(name, bar.add_modifier(Modifier::BOLD)),
        Span::styled(dirty.to_owned(), bar.fg(Color::Yellow)),
        Span::styled(" ".repeat(gap), bar),
        Span::styled(pos, bar),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn shows_mode_name_dirty_and_position() {
        let status = Status {
            mode: Mode::Insert,
            file_name: Some("main.rs"),
            dirty: true,
            row: 2,
            col: 4,
        };
        let line = status_line(status, 40);
        let s = text(&line);
        assert!(s.starts_with(" INSERT  main.rs [+]"));
        assert!(s.ends_with("3:5 "));
        assert_eq!(line.width(), 40);
        assert_eq!(line.spans[0].style.bg, Some(Color::Green));
    }

    #[test]
    fn unnamed_clean_buffer() {
        let status = Status {
            mode: Mode::Normal,
            file_name: None,
            dirty: false,
            row: 0,
            col: 0,
        };
        let s = text(&status_line(status, 10));
        assert!(s.contains("[No Name]"));
        assert!(!s.contains("[+]"));
    }
}
