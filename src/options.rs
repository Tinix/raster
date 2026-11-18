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

/// Full name of an option, accepting Vim's short names (`nu`, `ts`).
pub fn canonical(name: &str) -> Option<&'static str> {
    let full = match name {
        "nu" => "number",
        "rnu" => "relativenumber",
        "so" => "scrolloff",
        "ts" => "tabstop",
        "sw" => "shiftwidth",
        "et" => "expandtab",
        "ai" => "autoindent",
        "tm" => "timeoutlen",
        other => other,
    };
    NAMES.iter().copied().find(|n| *n == full)
}

impl Options {
    /// Run the arguments of `:set`, like Vim: `nu`, `nonu`, `nu!`, `invnu`,
    /// `nu?`, `nu&`, `ts=4`, `ts:4`, several separated by spaces. Returns
    /// the text to show (for queries), or the first error.
    pub fn set_from_ex(&mut self, args: &str) -> Result<Option<String>, String> {
        let mut shown = Vec::new();
        if args.trim().is_empty() {
            // Like Vim, `:set` alone lists the options that differ from the defaults.
            let defaults = Options::default();
            for name in NAMES {
                if self.get(name) != defaults.get(name) {
                    shown.push(self.show(name));
                }
            }
            return Ok(Some(if shown.is_empty() {
                "--- Options ---".into()
            } else {
                shown.join("  ")
            }));
        }
        for arg in args.split_whitespace() {
            if let Some(text) = self.set_arg(arg)? {
                shown.push(text);
            }
        }
        Ok((!shown.is_empty()).then(|| shown.join("  ")))
    }

    fn set_arg(&mut self, arg: &str) -> Result<Option<String>, String> {
        let unknown = || format!("E518: Unknown option: {arg}");
        if let Some((name, value)) = arg.split_once(['=', ':']) {
            let name = canonical(name).ok_or_else(unknown)?;
            if let Some(Value::Bool(_)) = self.get(name) {
                return Err(format!("E474: Invalid argument: {arg}"));
            }
            let n: i64 = value
                .parse()
                .map_err(|_| format!("E521: Number required after =: {arg}"))?;
            self.set(name, Value::Int(n))
                .map_err(|_| format!("E474: Invalid argument: {arg}"))?;
            return Ok(None);
        }
        if let Some(name) = arg.strip_suffix('?') {
            return Ok(Some(self.show(canonical(name).ok_or_else(unknown)?)));
        }
        if let Some(name) = arg.strip_suffix('&') {
            let name = canonical(name).ok_or_else(unknown)?;
            let default = Options::default().get(name).expect("known option");
            self.set(name, default).expect("default is valid");
            return Ok(None);
        }
        let (name, toggle) = match arg.strip_suffix('!') {
            Some(n) => (n, true),
            None => match arg.strip_prefix("inv") {
                Some(n) if canonical(n).is_some() => (n, true),
                _ => (arg, false),
            },
        };
        if let Some(name) = canonical(name) {
            return match self.get(name) {
                Some(Value::Bool(b)) => {
                    self.set(name, Value::Bool(if toggle { !b } else { true }))
                        .expect("bool");
                    Ok(None)
                }
                // `:set ts` shows a number option, like `:set ts?`.
                _ if !toggle => Ok(Some(self.show(name))),
                _ => Err(format!("E474: Invalid argument: {arg}")),
            };
        }
        if let Some(name) = arg.strip_prefix("no").and_then(canonical)
            && let Some(Value::Bool(_)) = self.get(name)
        {
            self.set(name, Value::Bool(false)).expect("bool");
            return Ok(None);
        }
        Err(unknown())
    }

    /// `number` / `nonumber` / `tabstop=8`, as `:set` shows it.
    fn show(&self, name: &str) -> String {
        match self.get(name) {
            Some(Value::Bool(true)) => format!("  {name}"),
            Some(Value::Bool(false)) => format!("no{name}"),
            Some(v) => format!("  {name}={v}"),
            None => String::new(),
        }
    }

    /// Set option `name`. Errors name the option and the expected type.
    pub fn set(&mut self, name: &str, value: Value) -> Result<(), String> {
        let name = canonical(name).unwrap_or(name);
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
        let name = canonical(name)?;
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

    fn ex(o: &mut Options, args: &str) -> Result<Option<String>, String> {
        o.set_from_ex(args)
    }

    #[test]
    fn set_number_long_and_short() {
        let mut o = Options::default();
        assert_eq!(ex(&mut o, "number"), Ok(None));
        assert!(o.number);
        ex(&mut o, "nonu").unwrap();
        assert!(!o.number);
        ex(&mut o, "nu").unwrap();
        assert!(o.number);
        ex(&mut o, "nonumber").unwrap();
        assert!(!o.number);
    }

    #[test]
    fn set_toggle_query_and_reset() {
        let mut o = Options::default();
        ex(&mut o, "nu!").unwrap();
        assert!(o.number);
        ex(&mut o, "invnu").unwrap();
        assert!(!o.number);
        assert_eq!(ex(&mut o, "nu?"), Ok(Some("nonumber".into())));
        ex(&mut o, "rnu").unwrap();
        assert_eq!(
            ex(&mut o, "relativenumber?"),
            Ok(Some("  relativenumber".into()))
        );
        ex(&mut o, "rnu&").unwrap();
        assert!(!o.relativenumber);
    }

    #[test]
    fn set_numbers() {
        let mut o = Options::default();
        ex(&mut o, "ts=4 sw:2 so=3").unwrap();
        assert_eq!((o.tabstop, o.shiftwidth, o.scrolloff), (4, 2, 3));
        assert_eq!(ex(&mut o, "ts"), Ok(Some("  tabstop=4".into())));
        assert_eq!(
            ex(&mut o, "ts? nu?"),
            Ok(Some("  tabstop=4  nonumber".into()))
        );
        ex(&mut o, "ts&").unwrap();
        assert_eq!(o.tabstop, 8);
    }

    #[test]
    fn set_errors_like_vim() {
        let mut o = Options::default();
        assert_eq!(ex(&mut o, "foo"), Err("E518: Unknown option: foo".into()));
        assert_eq!(ex(&mut o, "nots"), Err("E518: Unknown option: nots".into()));
        assert_eq!(
            ex(&mut o, "nu=1"),
            Err("E474: Invalid argument: nu=1".into())
        );
        assert_eq!(
            ex(&mut o, "ts=abc"),
            Err("E521: Number required after =: ts=abc".into())
        );
        assert_eq!(
            ex(&mut o, "ts=0"),
            Err("E474: Invalid argument: ts=0".into())
        );
        assert_eq!(ex(&mut o, "ts!"), Err("E474: Invalid argument: ts!".into()));
    }

    #[test]
    fn set_alone_lists_changed_options() {
        let mut o = Options::default();
        assert_eq!(ex(&mut o, ""), Ok(Some("--- Options ---".into())));
        ex(&mut o, "nu ts=2").unwrap();
        assert_eq!(ex(&mut o, ""), Ok(Some("  number    tabstop=2".into())));
    }

    #[test]
    fn short_names_work_everywhere() {
        let mut o = Options::default();
        o.set("nu", Value::Bool(true)).unwrap();
        assert_eq!(o.get("nu"), Some(Value::Bool(true)));
        assert_eq!(canonical("ts"), Some("tabstop"));
        assert_eq!(canonical("mouse"), None);
    }
}
