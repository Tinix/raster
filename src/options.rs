//! Options: editor settings, set from `init.lua` through `raster.opt`.

use std::fmt;

/// A value as Lua passes it in.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Str(String),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s:?}"),
        }
    }
}

/// All options, with Neovim's defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// Show line numbers.
    pub number: bool,
    /// Show line numbers relative to the cursor line.
    pub relativenumber: bool,
    /// Minimum lines kept above and below the cursor.
    pub scrolloff: usize,
    /// Display width of a tab.
    pub tabstop: usize,
    /// Indent width (used by the indent operators in a later step).
    pub shiftwidth: usize,
    /// `<Tab>` in Insert mode inserts spaces.
    pub expandtab: bool,
    /// New lines copy the indent of the current line.
    pub autoindent: bool,
    /// Milliseconds to wait for the rest of a mapped key sequence.
    pub timeoutlen: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            number: false,
            relativenumber: false,
            scrolloff: 0,
            tabstop: 8,
            shiftwidth: 8,
            expandtab: false,
            autoindent: true,
            timeoutlen: 1000,
        }
    }
}

/// Names `raster.opt` accepts, for errors and docs.
pub const NAMES: &[&str] = &[
    "number",
    "relativenumber",
    "scrolloff",
    "tabstop",
    "shiftwidth",
    "expandtab",
    "autoindent",
    "timeoutlen",
];

impl Options {
    /// Set option `name`. Errors name the option and the expected type.
    pub fn set(&mut self, name: &str, value: Value) -> Result<(), String> {
        fn flag(name: &str, v: &Value) -> Result<bool, String> {
            match v {
                Value::Bool(b) => Ok(*b),
                other => Err(format!("option '{name}' expects a boolean, got {other}")),
            }
        }
        fn count(name: &str, v: &Value, min: i64) -> Result<i64, String> {
            match v {
                Value::Int(n) if *n >= min => Ok(*n),
                other => Err(format!(
                    "option '{name}' expects an integer >= {min}, got {other}"
                )),
            }
        }
        match name {
            "number" => self.number = flag(name, &value)?,
            "relativenumber" => self.relativenumber = flag(name, &value)?,
            "expandtab" => self.expandtab = flag(name, &value)?,
            "autoindent" => self.autoindent = flag(name, &value)?,
            "scrolloff" => self.scrolloff = count(name, &value, 0)? as usize,
            "tabstop" => self.tabstop = count(name, &value, 1)? as usize,
            "shiftwidth" => self.shiftwidth = count(name, &value, 0)? as usize,
            "timeoutlen" => self.timeoutlen = count(name, &value, 0)? as u64,
            _ => return Err(format!("unknown option '{name}'")),
        }
        Ok(())
    }

    /// Read option `name`.
    pub fn get(&self, name: &str) -> Option<Value> {
        Some(match name {
            "number" => Value::Bool(self.number),
            "relativenumber" => Value::Bool(self.relativenumber),
            "expandtab" => Value::Bool(self.expandtab),
            "autoindent" => Value::Bool(self.autoindent),
            "scrolloff" => Value::Int(self.scrolloff as i64),
            "tabstop" => Value::Int(self.tabstop as i64),
            "shiftwidth" => Value::Int(self.shiftwidth as i64),
            "timeoutlen" => Value::Int(self.timeoutlen as i64),
            _ => return None,
        })
    }

    /// Indent width, falling back to `tabstop` when `shiftwidth` is 0 (like Vim).
    pub fn indent_width(&self) -> usize {
        if self.shiftwidth == 0 {
            self.tabstop
        } else {
            self.shiftwidth
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_neovim() {
        let o = Options::default();
        assert!(!o.number);
        assert!(o.autoindent);
        assert_eq!((o.tabstop, o.shiftwidth, o.timeoutlen), (8, 8, 1000));
    }

    #[test]
    fn set_and_get() {
        let mut o = Options::default();
        o.set("number", Value::Bool(true)).unwrap();
        o.set("tabstop", Value::Int(2)).unwrap();
        o.set("timeoutlen", Value::Int(500)).unwrap();
        assert_eq!(o.get("number"), Some(Value::Bool(true)));
        assert_eq!(o.get("tabstop"), Some(Value::Int(2)));
        assert_eq!(o.timeoutlen, 500);
    }

    #[test]
    fn every_name_round_trips() {
        let mut o = Options::default();
        for name in NAMES {
            let v = o.get(name).unwrap();
            o.set(name, v).unwrap();
        }
        assert_eq!(o, Options::default());
    }

    #[test]
    fn errors() {
        let mut o = Options::default();
        assert_eq!(
            o.set("mouse", Value::Str("a".into())),
            Err("unknown option 'mouse'".into())
        );
        assert!(
            o.set("number", Value::Int(1))
                .unwrap_err()
                .contains("expects a boolean")
        );
        assert!(
            o.set("tabstop", Value::Int(0))
                .unwrap_err()
                .contains(">= 1")
        );
        assert!(o.set("scrolloff", Value::Bool(true)).is_err());
        assert_eq!(o.get("mouse"), None);
    }

    #[test]
    fn indent_width_falls_back_to_tabstop() {
        let mut o = Options::default();
        o.set("shiftwidth", Value::Int(0)).unwrap();
        o.set("tabstop", Value::Int(4)).unwrap();
        assert_eq!(o.indent_width(), 4);
    }
}
