//! Keymap: user and default key mappings, per mode, like `vim.keymap.set`.
//!
//! Mappings sit in front of Vim's built-in keys. Raster's Dvorak motions are
//! default Normal-mode mappings (`t` → `j`, `n` → `k`, `s` → `l`), exactly as
//! in the author's nvim config, so after an operator `t` is still Vim's
//! "till" (`dt,`), and `init.lua` can change or delete any default.
//!
//! Right-hand sides are never remapped (nvim's default `noremap`).

use crate::keys::{self, Key, Token};
use std::collections::HashMap;

/// Mode a mapping applies in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapMode {
    /// `"n"`.
    Normal,
    /// `"i"`.
    Insert,
    /// `"o"`: after an operator, before its motion.
    OperatorPending,
    /// `"v"` / `"x"`: stored now, active once Visual mode exists.
    Visual,
}

impl MapMode {
    /// Parse a Vim mode string: `"n"`, `"i"`, `"o"`, `"v"`, `"x"`, or `""`
    /// (Normal, Visual and Operator-pending, like `:map`).
    pub fn parse(mode: &str) -> Result<Vec<MapMode>, String> {
        Ok(match mode {
            "n" => vec![MapMode::Normal],
            "i" => vec![MapMode::Insert],
            "o" => vec![MapMode::OperatorPending],
            "v" | "x" => vec![MapMode::Visual],
            "" => vec![MapMode::Normal, MapMode::Visual, MapMode::OperatorPending],
            other => return Err(format!("unsupported mode {other:?} (use n, i, o, v or x)")),
        })
    }
}

/// What a mapping runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rhs {
    /// Keys fed to the editor as if typed (not remapped). Empty = `<Nop>`.
    Keys(Vec<Token>),
    /// A Lua function, by its id in the config runtime.
    Lua(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    pub lhs: Vec<Token>,
    pub rhs: Rhs,
    pub desc: Option<String>,
}

/// Result of matching typed keys against the mappings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Match {
    /// The keys are the start of a longer mapping: wait for more (or for
    /// `timeoutlen`).
    Wait,
    /// The first `len` keys form a mapping.
    Mapped { rhs: Rhs, len: usize },
    /// The first key starts no mapping: hand it to the editor as is.
    Unmapped,
}

#[derive(Debug, Clone)]
pub struct Keymap {
    maps: HashMap<MapMode, Vec<Mapping>>,
    /// Key that `<leader>` stands for (`raster.g.mapleader`).
    pub leader: Key,
}

impl Default for Keymap {
    /// Raster's defaults, taken from the author's nvim config.
    fn default() -> Self {
        let mut km = Self::empty();
        let defaults: &[(&str, &str, &str, &str)] = &[
            // `vim.keymap.set({ "n", "v" }, "<Space>", "<Nop>")`
            ("n", "<Space>", "<Nop>", "Leader key"),
            ("v", "<Space>", "<Nop>", "Leader key"),
            // Dvorak motions: htns.
            ("n", "t", "j", "Down (Dvorak)"),
            ("n", "n", "k", "Up (Dvorak)"),
            ("n", "s", "l", "Right (Dvorak)"),
            ("v", "t", "j", "Down (Dvorak)"),
            ("v", "n", "k", "Up (Dvorak)"),
            ("v", "s", "l", "Right (Dvorak)"),
            ("n", "<C-s>", ":w<CR>", "Save file"),
            ("n", "<leader>q", ":q<CR>", "Quit"),
            ("n", "<leader>Q", ":qa!<CR>", "Quit all"),
        ];
        for (mode, lhs, rhs, desc) in defaults {
            km.set_keys(mode, lhs, rhs, Some(desc))
                .expect("valid default mapping");
        }
        km
    }
}

impl Keymap {
    /// No mappings at all, leader `<Space>`.
    pub fn empty() -> Self {
        Self {
            maps: HashMap::new(),
            leader: Key::char(' '),
        }
    }

    /// Add or replace a mapping in each of `modes`.
    pub fn set(
        &mut self,
        modes: &[MapMode],
        lhs: Vec<Token>,
        rhs: Rhs,
        desc: Option<String>,
    ) -> Result<(), String> {
        if lhs.is_empty() {
            return Err("empty left-hand side".into());
        }
        for mode in modes {
            let list = self.maps.entry(*mode).or_default();
            list.retain(|m| m.lhs != lhs);
            list.push(Mapping {
                lhs: lhs.clone(),
                rhs: rhs.clone(),
                desc: desc.clone(),
            });
        }
        Ok(())
    }

    /// `set` from notation strings, with a key-sequence right-hand side.
    pub fn set_keys(
        &mut self,
        mode: &str,
        lhs: &str,
        rhs: &str,
        desc: Option<&str>,
    ) -> Result<(), String> {
        let modes = MapMode::parse(mode)?;
        let rhs = Rhs::Keys(keys::parse(rhs)?);
        self.set(&modes, keys::parse(lhs)?, rhs, desc.map(Into::into))
    }

    /// Remove a mapping. Errors if it does not exist in any of `modes`.
    pub fn del(&mut self, modes: &[MapMode], lhs: &[Token]) -> Result<(), String> {
        let mut found = false;
        for mode in modes {
            if let Some(list) = self.maps.get_mut(mode) {
                let before = list.len();
                list.retain(|m| m.lhs != lhs);
                found |= list.len() != before;
            }
        }
        if found {
            Ok(())
        } else {
            Err("no such mapping".into())
        }
    }

    /// Mappings of `mode`, in definition order.
    pub fn mappings(&self, mode: MapMode) -> &[Mapping] {
        self.maps.get(&mode).map_or(&[], Vec::as_slice)
    }

    /// Plain keys of a token sequence, with `<leader>` resolved.
    pub fn resolve(&self, tokens: &[Token]) -> Vec<Key> {
        tokens
            .iter()
            .map(|t| match t {
                Token::Key(k) => *k,
                Token::Leader => self.leader,
            })
            .collect()
    }

    /// The mapping whose lhs is exactly `keys`, and whether a longer one
    /// starts with `keys`.
    fn lookup(&self, mode: MapMode, keys: &[Key]) -> (Option<&Mapping>, bool) {
        let mut exact = None;
        let mut longer = false;
        for m in self.mappings(mode) {
            let lhs = self.resolve(&m.lhs);
            if lhs == keys {
                exact = Some(m);
            } else if lhs.len() > keys.len() && lhs.starts_with(keys) {
                longer = true;
            }
        }
        (exact, longer)
    }

    /// Match the keys typed so far. With `timed_out`, stop waiting for
    /// longer mappings and take the longest one that is complete.
    pub fn matches(&self, mode: MapMode, pending: &[Key], timed_out: bool) -> Match {
        if pending.is_empty() {
            return Match::Unmapped;
        }
        let (exact, longer) = self.lookup(mode, pending);
        if longer && !timed_out {
            return Match::Wait;
        }
        if let Some(m) = exact {
            return Match::Mapped {
                rhs: m.rhs.clone(),
                len: pending.len(),
            };
        }
        // The last key broke the sequence: fall back to the longest
        // complete mapping at the start.
        (1..pending.len())
            .rev()
            .find_map(|n| {
                self.lookup(mode, &pending[..n]).0.map(|m| Match::Mapped {
                    rhs: m.rhs.clone(),
                    len: n,
                })
            })
            .unwrap_or(Match::Unmapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};

    fn k(s: &str) -> Vec<Key> {
        keys::parse_keys(s, Key::char(' ')).unwrap()
    }

    fn rhs(s: &str) -> Rhs {
        Rhs::Keys(keys::parse(s).unwrap())
    }

    fn mapped(s: &str, len: usize) -> Match {
        Match::Mapped { rhs: rhs(s), len }
    }

    #[test]
    fn mode_strings() {
        assert_eq!(MapMode::parse("n").unwrap(), vec![MapMode::Normal]);
        assert_eq!(MapMode::parse("x").unwrap(), vec![MapMode::Visual]);
        assert_eq!(MapMode::parse("").unwrap().len(), 3);
        assert!(MapMode::parse("c").is_err());
    }

    #[test]
    fn defaults_follow_the_nvim_config() {
        let km = Keymap::default();
        let n = MapMode::Normal;
        assert_eq!(km.matches(n, &k("t"), false), mapped("j", 1));
        assert_eq!(km.matches(n, &k("n"), false), mapped("k", 1));
        assert_eq!(km.matches(n, &k("s"), false), mapped("l", 1));
        assert_eq!(km.matches(n, &k("<C-s>"), false), mapped(":w<CR>", 1));
        assert_eq!(km.matches(n, &k("<leader>Q"), false), mapped(":qa!<CR>", 2));
        // Not in Operator-pending mode: `dt,` is Vim's till.
        assert_eq!(
            km.matches(MapMode::OperatorPending, &k("t"), false),
            Match::Unmapped
        );
        assert_eq!(km.matches(MapMode::Insert, &k("t"), false), Match::Unmapped);
        assert!(!km.mappings(MapMode::Visual).is_empty());
    }

    #[test]
    fn unmapped_keys_pass_through() {
        let km = Keymap::default();
        assert_eq!(km.matches(MapMode::Normal, &k("w"), false), Match::Unmapped);
        assert_eq!(km.matches(MapMode::Normal, &[], false), Match::Unmapped);
    }

    #[test]
    fn prefix_waits_then_resolves() {
        let km = Keymap::default();
        let n = MapMode::Normal;
        // `<Space>` is both `<Nop>` and the start of `<leader>q`.
        assert_eq!(km.matches(n, &k(" "), false), Match::Wait);
        assert_eq!(km.matches(n, &k(" q"), false), mapped(":q<CR>", 2));
        // Timeout: take the complete `<Space>` mapping.
        assert_eq!(km.matches(n, &k(" "), true), mapped("<Nop>", 1));
        // A key that breaks the sequence: `<Space>` runs, then `x` is retried.
        assert_eq!(km.matches(n, &k(" x"), false), mapped("<Nop>", 1));
    }

    #[test]
    fn prefix_without_complete_mapping_is_unmapped() {
        let mut km = Keymap::empty();
        km.set_keys("n", "<leader>ff", ":w<CR>", None).unwrap();
        let n = MapMode::Normal;
        assert_eq!(km.matches(n, &k(" f"), false), Match::Wait);
        assert_eq!(km.matches(n, &k(" fx"), false), Match::Unmapped);
        assert_eq!(km.matches(n, &k(" f"), true), Match::Unmapped);
        assert_eq!(km.matches(n, &k(" ff"), false), mapped(":w<CR>", 3));
    }

    #[test]
    fn set_replaces_and_del_removes() {
        let mut km = Keymap::default();
        km.set_keys("n", "t", "gg", None).unwrap();
        assert_eq!(km.matches(MapMode::Normal, &k("t"), false), mapped("gg", 1));
        assert_eq!(
            km.mappings(MapMode::Normal)
                .iter()
                .filter(|m| m.lhs == keys::parse("t").unwrap())
                .count(),
            1
        );

        km.del(&[MapMode::Normal], &keys::parse("t").unwrap())
            .unwrap();
        assert_eq!(km.matches(MapMode::Normal, &k("t"), false), Match::Unmapped);
        assert!(
            km.del(&[MapMode::Normal], &keys::parse("t").unwrap())
                .is_err()
        );
        assert!(km.set_keys("n", "", "x", None).is_err());
    }

    #[test]
    fn leader_is_resolved_when_matching() {
        let km = Keymap {
            leader: Key::char(','),
            ..Keymap::default()
        };
        assert_eq!(
            km.matches(MapMode::Normal, &k(",q"), false),
            mapped(":q<CR>", 2)
        );
        // `<Space>` itself stays mapped to `<Nop>`, but no longer waits.
        assert_eq!(
            km.matches(MapMode::Normal, &k(" "), false),
            mapped("<Nop>", 1)
        );
    }

    #[test]
    fn modifier_keys_match() {
        let km = Keymap::default();
        let ctrl_s = [Key::new(KeyCode::Char('s'), KeyModifiers::CONTROL)];
        assert_eq!(
            km.matches(MapMode::Normal, &ctrl_s, false),
            mapped(":w<CR>", 1)
        );
    }

    #[test]
    fn lua_rhs() {
        let mut km = Keymap::empty();
        km.set(
            &[MapMode::Insert],
            keys::parse("jk").unwrap(),
            Rhs::Lua(7),
            Some("Escape".into()),
        )
        .unwrap();
        assert_eq!(km.matches(MapMode::Insert, &k("j"), false), Match::Wait);
        assert_eq!(
            km.matches(MapMode::Insert, &k("jk"), false),
            Match::Mapped {
                rhs: Rhs::Lua(7),
                len: 2
            }
        );
        assert_eq!(
            km.mappings(MapMode::Insert)[0].desc.as_deref(),
            Some("Escape")
        );
    }
}
