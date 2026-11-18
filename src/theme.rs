//! Theme: highlight groups (`Normal`, `Comment`, `ModeInsert`, ...) and
//! their colors, like nvim's highlight groups.
//!
//! Themes are plain Lua tables (see `runtime/themes/`). A group is either a
//! style (`{ fg = "#c792ea", bold = true }`) or a link to another group
//! (`{ link = "Keyword" }`).

use ratatui::style::{Color, Modifier, Style};
use std::collections::HashMap;

/// Groups the editor draws with. Themes may define others; they are kept and
/// can be linked to.
pub const GROUPS: &[(&str, &str)] = &[
    ("Normal", "text and background"),
    ("LineNr", "line numbers"),
    ("CursorLineNr", "line number of the cursor line"),
    ("StatusLine", "status bar"),
    ("StatusModified", "the [+] marker in the status bar"),
    ("ModeNormal", "mode pill in Normal mode"),
    ("ModeInsert", "mode pill in Insert mode"),
    ("ModeCommand", "mode pill in Command mode"),
    ("Comment", "comments"),
    ("String", "strings and chars"),
    ("Number", "numbers"),
    ("Constant", "true, false, nil, atoms, symbols"),
    ("Keyword", "keywords"),
    ("Type", "types and modules"),
    ("Function", "function names and calls"),
    ("Operator", "operators"),
    ("PreProc", "attributes, decorators, macros"),
    ("Special", "special variables (@ivar, $global)"),
    ("Title", "Markdown headings"),
    ("Bold", "Markdown **bold**"),
    ("Italic", "Markdown *italic*"),
];

/// One highlight group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hl {
    Style(Style),
    Link(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    groups: HashMap<String, Hl>,
}

impl Default for Theme {
    fn default() -> Self {
        Self::fallback()
    }
}

impl Theme {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            groups: HashMap::new(),
        }
    }

    /// The `default` theme: the terminal's own 16 colors, no background.
    /// Used without a config, and by `:colorscheme default`.
    pub fn fallback() -> Self {
        let mut t = Self::new("default");
        let fg = |c| Style::default().fg(c);
        let pill = |c| {
            Style::default()
                .fg(Color::Black)
                .bg(c)
                .add_modifier(Modifier::BOLD)
        };
        let bar = Style::default().fg(Color::White).bg(Color::DarkGray);
        for (name, style) in [
            ("LineNr", fg(Color::DarkGray)),
            ("CursorLineNr", fg(Color::Yellow)),
            ("StatusLine", bar),
            ("StatusModified", bar.fg(Color::Yellow)),
            ("ModeNormal", pill(Color::Magenta)),
            ("ModeInsert", pill(Color::Green)),
            ("ModeCommand", pill(Color::Yellow)),
            (
                "Comment",
                fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            ),
            ("String", fg(Color::Green)),
            ("Number", fg(Color::Yellow)),
            ("Constant", fg(Color::Yellow)),
            ("Keyword", fg(Color::Magenta)),
            ("Type", fg(Color::Cyan)),
            ("Function", fg(Color::Blue)),
            ("Operator", fg(Color::Cyan)),
            ("PreProc", fg(Color::Red)),
            ("Special", fg(Color::Red)),
            ("Title", fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            ("Bold", Style::default().add_modifier(Modifier::BOLD)),
            ("Italic", Style::default().add_modifier(Modifier::ITALIC)),
        ] {
            t.set(name, Hl::Style(style));
        }
        t
    }

    pub fn set(&mut self, group: &str, hl: Hl) {
        self.groups.insert(group.to_owned(), hl);
    }

    /// Style of `group`, following links. Undefined groups have no style.
    pub fn style(&self, group: &str) -> Style {
        let mut name = group;
        // Links can't go deeper than this (and cycles end here).
        for _ in 0..16 {
            match self.groups.get(name) {
                Some(Hl::Style(s)) => return *s,
                Some(Hl::Link(to)) => name = to,
                None => return Style::default(),
            }
        }
        Style::default()
    }
}

/// Parse a color: `"#rrggbb"`, `"#rgb"`, one of the 16 terminal color names
/// (`"red"`, `"darkgray"`, `"lightblue"`, ...), or `"none"`.
pub fn parse_color(s: &str) -> Result<Color, String> {
    let bad = || format!("invalid color {s:?} (use \"#rrggbb\" or a color name)");
    if let Some(hex) = s.strip_prefix('#') {
        let digits: Vec<u8> = hex
            .chars()
            .map(|c| c.to_digit(16).map(|d| d as u8))
            .collect::<Option<_>>()
            .ok_or_else(bad)?;
        return match digits.as_slice() {
            [r, g, b] => Ok(Color::Rgb(r * 17, g * 17, b * 17)),
            [r1, r2, g1, g2, b1, b2] => Ok(Color::Rgb(r1 * 16 + r2, g1 * 16 + g2, b1 * 16 + b2)),
            _ => Err(bad()),
        };
    }
    Ok(
        match s.to_ascii_lowercase().replace(['_', ' '], "").as_str() {
            "none" | "reset" => Color::Reset,
            "black" => Color::Black,
            "red" => Color::Red,
            "green" => Color::Green,
            "yellow" => Color::Yellow,
            "blue" => Color::Blue,
            "magenta" => Color::Magenta,
            "cyan" => Color::Cyan,
            "gray" | "grey" => Color::Gray,
            "darkgray" | "darkgrey" => Color::DarkGray,
            "lightred" => Color::LightRed,
            "lightgreen" => Color::LightGreen,
            "lightyellow" => Color::LightYellow,
            "lightblue" => Color::LightBlue,
            "lightmagenta" => Color::LightMagenta,
            "lightcyan" => Color::LightCyan,
            "white" => Color::White,
            _ => return Err(bad()),
        },
    )
}

/// Build a style from its parts (as read from a Lua table).
pub fn make_style(
    fg: Option<&str>,
    bg: Option<&str>,
    bold: bool,
    italic: bool,
    underline: bool,
) -> Result<Style, String> {
    let mut style = Style::default();
    if let Some(c) = fg {
        style = style.fg(parse_color(c)?);
    }
    if let Some(c) = bg {
        style = style.bg(parse_color(c)?);
    }
    for (on, m) in [
        (bold, Modifier::BOLD),
        (italic, Modifier::ITALIC),
        (underline, Modifier::UNDERLINED),
    ] {
        if on {
            style = style.add_modifier(m);
        }
    }
    Ok(style)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(parse_color("#c792ea"), Ok(Color::Rgb(0xc7, 0x92, 0xea)));
        assert_eq!(parse_color("#fff"), Ok(Color::Rgb(255, 255, 255)));
        assert_eq!(parse_color("DarkGray"), Ok(Color::DarkGray));
        assert_eq!(parse_color("light_blue"), Ok(Color::LightBlue));
        assert_eq!(parse_color("NONE"), Ok(Color::Reset));
        assert!(parse_color("#12345").is_err());
        assert!(parse_color("#gggggg").is_err());
        assert!(parse_color("violetish").is_err());
    }

    #[test]
    fn styles() {
        let s = make_style(Some("#000000"), Some("red"), true, false, true).unwrap();
        assert_eq!(s.fg, Some(Color::Rgb(0, 0, 0)));
        assert_eq!(s.bg, Some(Color::Red));
        assert!(
            s.add_modifier
                .contains(Modifier::BOLD | Modifier::UNDERLINED)
        );
        assert!(!s.add_modifier.contains(Modifier::ITALIC));
        assert!(make_style(Some("nope"), None, false, false, false).is_err());
    }

    #[test]
    fn links_resolve_and_cycles_stop() {
        let mut t = Theme::new("t");
        let red = Style::default().fg(Color::Red);
        t.set("Keyword", Hl::Style(red));
        t.set("Repeat", Hl::Link("Keyword".into()));
        t.set("Loop", Hl::Link("Repeat".into()));
        assert_eq!(t.style("Loop"), red);
        t.set("A", Hl::Link("B".into()));
        t.set("B", Hl::Link("A".into()));
        assert_eq!(t.style("A"), Style::default());
        assert_eq!(t.style("Missing"), Style::default());
    }

    #[test]
    fn fallback_defines_every_ui_group_but_normal() {
        let t = Theme::fallback();
        assert_eq!(t.name, "default");
        assert_eq!(t.style("ModeInsert").bg, Some(Color::Green));
        assert_eq!(t.style("Normal"), Style::default());
        for (group, _) in GROUPS.iter().skip(1) {
            assert_ne!(t.style(group), Style::default(), "{group}");
        }
    }
}
