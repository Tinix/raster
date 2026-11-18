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

/// Expand tabs in `line` to spaces, with tab stops every `tabstop` columns.
pub fn expand_tabs(line: &str, tabstop: usize) -> String {
    let mut out = String::with_capacity(line.len());
    let mut width = 0;
    for c in line.chars() {
        if c == '\t' {
            let n = tabstop - width % tabstop;
            out.extend(std::iter::repeat_n(' ', n));
            width += n;
        } else {
            out.push(c);
            width += 1;
        }
    }
    out
}

/// Screen column of char `col` in `line` once tabs are expanded.
pub fn display_col(line: &str, col: usize, tabstop: usize) -> usize {
    line.chars().take(col).fold(0, |w, c| {
        if c == '\t' {
            w + tabstop - w % tabstop
        } else {
            w + 1
        }
    })
}

/// Width of the line-number gutter (number plus one space), or 0 when
/// neither `number` nor `relativenumber` is set. At least 3 digits, like Vim.
pub fn gutter_width(number: bool, relativenumber: bool, line_count: usize) -> usize {
    if !number && !relativenumber {
        return 0;
    }
    line_count.max(1).to_string().len().max(3) + 1
}

/// Gutter text for `row`. With `relativenumber`, other lines show their
/// distance to the cursor line, and the cursor line shows its absolute
/// number if `number` is set (0 otherwise), like Vim.
pub fn line_number(
    row: usize,
    cursor: usize,
    number: bool,
    relativenumber: bool,
    width: usize,
) -> String {
    let n = if relativenumber && row != cursor {
        row.abs_diff(cursor)
    } else if relativenumber && !number {
        0
    } else {
        row + 1
    };
    format!("{n:>w$} ", w = width.saturating_sub(1))
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

    #[test]
    fn tabs_expand_to_tab_stops() {
        assert_eq!(expand_tabs("\tx", 4), "    x");
        assert_eq!(expand_tabs("ab\tc", 4), "ab  c");
        assert_eq!(expand_tabs("abcd\te", 4), "abcd    e");
        assert_eq!(display_col("\tx", 1, 4), 4);
        assert_eq!(display_col("ab\tc", 3, 4), 4);
        assert_eq!(display_col("abc", 2, 8), 2);
    }

    #[test]
    fn gutter() {
        assert_eq!(gutter_width(false, false, 100), 0);
        assert_eq!(gutter_width(true, false, 5), 4);
        assert_eq!(gutter_width(false, true, 12345), 6);
        assert_eq!(line_number(4, 0, true, false, 4), "  5 ");
        assert_eq!(line_number(4, 2, true, true, 4), "  2 ");
        assert_eq!(line_number(2, 2, true, true, 4), "  3 ");
        assert_eq!(line_number(2, 2, false, true, 4), "  0 ");
    }
}
