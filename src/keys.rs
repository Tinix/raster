//! Keys: a normalized key type and Vim key notation (`<C-s>`, `<leader>q`).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A key press without the parts that don't matter for mappings
/// (key kind, key state, and the Shift flag on chars).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl Key {
    pub const fn new(code: KeyCode, mods: KeyModifiers) -> Self {
        Self { code, mods }
    }

    pub const fn char(c: char) -> Self {
        Self::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    pub fn event(self) -> KeyEvent {
        KeyEvent::new(self.code, self.mods)
    }
}

impl From<KeyEvent> for Key {
    fn from(e: KeyEvent) -> Self {
        let mut mods =
            e.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        let code = match e.code {
            // Shift is already in the char itself: `A`, `$`, `?`.
            KeyCode::Char(c) => {
                mods.remove(KeyModifiers::SHIFT);
                KeyCode::Char(c)
            }
            // Some terminals send Shift+Tab as BackTab.
            KeyCode::BackTab => {
                mods.insert(KeyModifiers::SHIFT);
                KeyCode::Tab
            }
            code => code,
        };
        Self { code, mods }
    }
}

/// One element of a mapping's left-hand side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Token {
    Key(Key),
    /// `<leader>`, resolved against the leader key when matching.
    Leader,
}

/// Parse Vim key notation, e.g. `":w<CR>"`, `"<leader>ff"`, `"<S-A-t>"`.
/// `<Nop>` parses to nothing. Names are case-insensitive.
pub fn parse(notation: &str) -> Result<Vec<Token>, String> {
    let mut out = Vec::new();
    let mut rest = notation;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(end) = rest.find('>')
            && end > 1
        {
            let name = &rest[1..end];
            match special(name) {
                Some(Special::Leader) => out.push(Token::Leader),
                Some(Special::Nop) => {}
                Some(Special::Key(k)) => out.push(Token::Key(k)),
                None => return Err(format!("unknown key <{name}> in {notation:?}")),
            }
            rest = &rest[end + 1..];
        } else {
            out.push(Token::Key(Key::char(c)));
            rest = &rest[c.len_utf8()..];
        }
    }
    Ok(out)
}

/// Parse notation into plain keys, replacing `<leader>` with `leader`.
pub fn parse_keys(notation: &str, leader: Key) -> Result<Vec<Key>, String> {
    Ok(parse(notation)?
        .into_iter()
        .map(|t| match t {
            Token::Key(k) => k,
            Token::Leader => leader,
        })
        .collect())
}

enum Special {
    Key(Key),
    Leader,
    Nop,
}

fn special(name: &str) -> Option<Special> {
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "leader" => return Some(Special::Leader),
        "nop" => return Some(Special::Nop),
        _ => {}
    }

    // Modifier prefixes: `C-`, `S-`, `A-` / `M-`, in any order.
    let mut mods = KeyModifiers::NONE;
    let mut base = name;
    while base.len() > 2 && base.as_bytes()[1] == b'-' {
        match base.as_bytes()[0].to_ascii_lowercase() {
            b'c' => mods |= KeyModifiers::CONTROL,
            b's' => mods |= KeyModifiers::SHIFT,
            b'a' | b'm' => mods |= KeyModifiers::ALT,
            _ => return None,
        }
        base = &base[2..];
    }

    let mut chars = base.chars();
    let code = match (chars.next(), chars.next()) {
        (Some(c), None) => {
            // `<S-j>` is `J`; `<C-s>` is Ctrl plus lowercase `s`.
            let shifted = mods.contains(KeyModifiers::SHIFT);
            mods.remove(KeyModifiers::SHIFT);
            let c = if shifted {
                c.to_ascii_uppercase()
            } else if mods.contains(KeyModifiers::CONTROL) {
                c.to_ascii_lowercase()
            } else {
                c
            };
            KeyCode::Char(c)
        }
        _ => {
            let code = named(&base.to_ascii_lowercase())?;
            if let KeyCode::Char(_) = code {
                mods.remove(KeyModifiers::SHIFT);
            }
            code
        }
    };
    Some(Special::Key(Key::new(code, mods)))
}

fn named(name: &str) -> Option<KeyCode> {
    Some(match name {
        "space" => KeyCode::Char(' '),
        "lt" => KeyCode::Char('<'),
        "bar" => KeyCode::Char('|'),
        "bslash" => KeyCode::Char('\\'),
        "cr" | "enter" | "return" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "bs" => KeyCode::Backspace,
        "del" => KeyCode::Delete,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "insert" => KeyCode::Insert,
        f if f.starts_with('f') => {
            KeyCode::F(f[1..].parse().ok().filter(|n| (1..=12).contains(n))?)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, mods: KeyModifiers) -> Token {
        Token::Key(Key::new(code, mods))
    }

    fn ch(c: char) -> Token {
        Token::Key(Key::char(c))
    }

    #[test]
    fn plain_chars() {
        assert_eq!(parse("dd").unwrap(), vec![ch('d'), ch('d')]);
        assert_eq!(parse("").unwrap(), vec![]);
    }

    #[test]
    fn special_keys() {
        assert_eq!(
            parse(":w<CR>").unwrap(),
            vec![ch(':'), ch('w'), key(KeyCode::Enter, KeyModifiers::NONE)]
        );
        assert_eq!(
            parse("<Esc>").unwrap(),
            vec![key(KeyCode::Esc, KeyModifiers::NONE)]
        );
        assert_eq!(parse("<space>").unwrap(), vec![ch(' ')]);
        assert_eq!(parse("<lt>").unwrap(), vec![ch('<')]);
        assert_eq!(
            parse("<F5>").unwrap(),
            vec![key(KeyCode::F(5), KeyModifiers::NONE)]
        );
        assert_eq!(parse("<Nop>").unwrap(), vec![]);
    }

    #[test]
    fn modifiers() {
        assert_eq!(
            parse("<C-s>").unwrap(),
            vec![key(KeyCode::Char('s'), KeyModifiers::CONTROL)]
        );
        assert_eq!(
            parse("<C-S>").unwrap(),
            vec![key(KeyCode::Char('s'), KeyModifiers::CONTROL)]
        );
        assert_eq!(parse("<S-l>").unwrap(), vec![ch('L')]);
        assert_eq!(
            parse("<S-A-t>").unwrap(),
            vec![key(KeyCode::Char('T'), KeyModifiers::ALT)]
        );
        assert_eq!(
            parse("<M-x>").unwrap(),
            vec![key(KeyCode::Char('x'), KeyModifiers::ALT)]
        );
        assert_eq!(
            parse("<C-S-j>").unwrap(),
            vec![key(KeyCode::Char('J'), KeyModifiers::CONTROL)]
        );
        assert_eq!(
            parse("<C-Up>").unwrap(),
            vec![key(KeyCode::Up, KeyModifiers::CONTROL)]
        );
        assert_eq!(
            parse("<S-Tab>").unwrap(),
            vec![key(KeyCode::Tab, KeyModifiers::SHIFT)]
        );
    }

    #[test]
    fn leader() {
        assert_eq!(
            parse("<leader>ff").unwrap(),
            vec![Token::Leader, ch('f'), ch('f')]
        );
        assert_eq!(
            parse_keys("<Leader>q", Key::char(',')).unwrap(),
            vec![Key::char(','), Key::char('q')]
        );
    }

    #[test]
    fn lone_angle_brackets_are_literal() {
        assert_eq!(parse("<").unwrap(), vec![ch('<')]);
        assert_eq!(parse("<>").unwrap(), vec![ch('<'), ch('>')]);
        assert_eq!(parse("a<b").unwrap(), vec![ch('a'), ch('<'), ch('b')]);
    }

    #[test]
    fn unknown_names_fail() {
        assert!(parse("<Foo>").is_err());
        assert!(parse("<X-a>").is_err());
        assert!(parse("<F13>").is_err());
    }

    #[test]
    fn events_are_normalized() {
        let shifted = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        assert_eq!(Key::from(shifted), Key::char('G'));
        let back_tab = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        assert_eq!(
            Key::from(back_tab),
            Key::new(KeyCode::Tab, KeyModifiers::SHIFT)
        );
        let ctrl = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(Key::from(ctrl).event().modifiers, KeyModifiers::CONTROL);
    }
}
