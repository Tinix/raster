//! Config: the LuaJIT runtime behind `init.lua` and the global `raster` API.
//!
//! ```lua
//! raster.g.mapleader = " "
//! raster.opt.number = true
//! raster.keymap.set("n", "<leader>w", ":w<CR>", { desc = "Save" })
//! raster.keymap.set("i", "jk", "<Esc>")
//! raster.keymap.set("n", "<leader>h", function() raster.notify("hi") end)
//! raster.keymap.del("n", "<leader>q")
//! raster.cmd("w")            -- run an ex-command
//! raster.feedkeys("dd")      -- type keys (not remapped)
//! raster.notify("message")   -- show a message (print does the same)
//! ```
//!
//! Lua functions can't touch the editor directly while it runs them: they
//! queue [`Action`]s that the editor applies right after the call.

use crate::{
    keymap::{Keymap, MapMode, Rhs},
    keys,
    options::{Options, Value},
    syntax::{Span, Syntax, SyntaxSet},
    theme::{self, Hl, Theme},
};
use mlua::{Function, Lua, Table, Value as LuaValue};
use std::{
    cell::RefCell,
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
    rc::Rc,
};

/// Something a Lua function asked the editor to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// `raster.cmd("w")`
    Cmd(String),
    /// `raster.feedkeys("dd")`, in key notation.
    Feed(String),
    /// `raster.notify(msg)` or `print(...)`.
    Notify(String),
}

/// Editor state the `raster` API reads and changes.
#[derive(Debug, Clone, Default)]
pub struct Shared {
    pub options: Rc<RefCell<Options>>,
    pub keymap: Rc<RefCell<Keymap>>,
    pub theme: Rc<RefCell<Theme>>,
    pub syntaxes: Rc<RefCell<SyntaxSet>>,
}

/// Themes built into the binary (`runtime/themes/`).
const THEMES: &[(&str, &str)] = &[(
    "raster-violet",
    include_str!("../runtime/themes/raster-violet.lua"),
)];

/// Theme used when the config doesn't pick one.
pub const DEFAULT_THEME: &str = "raster-violet";

/// Syntaxes built into the binary (`runtime/syntax/`), defined with the same
/// `raster.syntax.define` API a user config uses.
const SYNTAXES: &[(&str, &str)] = &[
    ("rust", include_str!("../runtime/syntax/rust.lua")),
    ("lua", include_str!("../runtime/syntax/lua.lua")),
    ("python", include_str!("../runtime/syntax/python.lua")),
    ("go", include_str!("../runtime/syntax/go.lua")),
    ("ruby", include_str!("../runtime/syntax/ruby.lua")),
    ("elixir", include_str!("../runtime/syntax/elixir.lua")),
    ("gleam", include_str!("../runtime/syntax/gleam.lua")),
    ("markdown", include_str!("../runtime/syntax/markdown.lua")),
];

pub struct Config {
    lua: Lua,
    /// Folder of the loaded `init.lua`, where `themes/` is looked up.
    config_dir: Rc<RefCell<Option<PathBuf>>>,
    callbacks: Rc<RefCell<Vec<Function>>>,
    actions: Rc<RefCell<Vec<Action>>>,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("callbacks", &self.callbacks.borrow().len())
            .field("actions", &self.actions.borrow())
            .finish_non_exhaustive()
    }
}

/// `$XDG_CONFIG_HOME/raster/init.lua`, or `~/.config/raster/init.lua`.
pub fn default_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("raster").join("init.lua"))
}

/// Lua side of the API: thin wrappers that raise errors at the caller's
/// line (`error(msg, 2)`), so messages point into `init.lua`.
const PRELUDE: &str = r##"
local native = ...
local raster = {}

raster.opt = setmetatable({}, {
  __index = function(_, name)
    local value, err = native.opt_get(name)
    if err then error(err, 2) end
    return value
  end,
  __newindex = function(_, name, value)
    local err = native.opt_set(name, value)
    if err then error(err, 2) end
  end,
})

local globals = {}
raster.g = setmetatable({}, {
  __index = globals,
  __newindex = function(_, name, value)
    if name == "mapleader" then
      local err = native.set_leader(value)
      if err then error(err, 2) end
    end
    globals[name] = value
  end,
})

local function modes(mode)
  if type(mode) == "string" then return { mode } end
  if type(mode) == "table" then return mode end
  error("mode must be a string or a list of strings", 3)
end

raster.keymap = {
  set = function(mode, lhs, rhs, opts)
    opts = opts or {}
    if opts.remap then error("remap = true is not supported yet", 2) end
    if type(lhs) ~= "string" then error("lhs must be a string", 2) end
    if type(rhs) ~= "string" and type(rhs) ~= "function" then
      error("rhs must be a string or a function", 2)
    end
    local err = native.map_set(modes(mode), lhs, rhs, opts.desc)
    if err then error(err, 2) end
  end,
  del = function(mode, lhs)
    local err = native.map_del(modes(mode), lhs)
    if err then error(err, 2) end
  end,
}

raster.colorscheme = function(name)
  local err = native.colorscheme(tostring(name))
  if err then error(err, 2) end
end

raster.theme = {
  -- Change one group of the current theme, like nvim_set_hl.
  set = function(group, spec)
    if type(group) ~= "string" or type(spec) ~= "table" then
      error("usage: raster.theme.set(group, { fg = ..., bg = ..., bold = ... })", 2)
    end
    local err = native.theme_set(group, spec)
    if err then error(err, 2) end
  end,
  name = function() return native.theme_name() end,
}

raster.syntax = {
  define = function(name, spec)
    if type(name) ~= "string" or type(spec) ~= "table" then
      error("usage: raster.syntax.define(name, { extensions = { ... }, ... })", 2)
    end
    local err = native.syntax_define(name, spec)
    if err then error(err, 2) end
  end,
}

raster.cmd = function(cmd) native.action("cmd", tostring(cmd)) end
raster.feedkeys = function(keys) native.action("feed", tostring(keys)) end
raster.notify = function(msg) native.action("notify", tostring(msg)) end

print = function(...)
  local parts = {}
  for i = 1, select("#", ...) do parts[#parts + 1] = tostring((select(i, ...))) end
  native.action("notify", table.concat(parts, "\t"))
end

_G.raster = raster
"##;

impl Config {
    /// A runtime whose `raster` API edits the `shared` state. Defines the
    /// built-in syntaxes and switches to the default theme.
    pub fn new(shared: &Shared) -> Result<Self, String> {
        let lua = Lua::new();
        let callbacks: Rc<RefCell<Vec<Function>>> = Rc::default();
        let actions: Rc<RefCell<Vec<Action>>> = Rc::default();
        let config_dir: Rc<RefCell<Option<PathBuf>>> = Rc::default();
        let native = native_api(
            &lua,
            shared.options.clone(),
            shared.keymap.clone(),
            callbacks.clone(),
            actions.clone(),
        )
        .map_err(lua_error)?;
        ui_api(&lua, &native, shared, config_dir.clone()).map_err(lua_error)?;
        lua.load(PRELUDE)
            .set_name("=raster")
            .call::<()>(native)
            .map_err(lua_error)?;
        let config = Self {
            lua,
            config_dir,
            callbacks,
            actions,
        };
        for (name, src) in SYNTAXES {
            config.exec(src, &format!("runtime/syntax/{name}.lua"))?;
        }
        config.colorscheme(DEFAULT_THEME)?;
        Ok(config)
    }

    /// Switch to theme `name` (`:colorscheme name`).
    pub fn colorscheme(&self, name: &str) -> Result<(), String> {
        let f: Function = self
            .lua
            .load("return raster.colorscheme")
            .eval()
            .map_err(lua_error)?;
        f.call::<()>(name).map_err(|e| {
            // The error is raised at this Rust call site: drop the location.
            let msg = lua_error(e);
            match msg.split_once("]:") {
                Some((at, rest)) if at.starts_with('[') => rest
                    .split_once(": ")
                    .map_or(msg.clone(), |(_, m)| m.to_owned()),
                _ => msg,
            }
        })
    }

    /// Run Lua source. `name` shows up in error messages.
    pub fn exec(&self, src: &str, name: &str) -> Result<(), String> {
        self.lua
            .load(src)
            .set_name(format!("={name}"))
            .exec()
            .map_err(lua_error)
    }

    /// Run a config file. Its `lua/` folder is added to `package.path`, so
    /// `require("keymaps")` loads `lua/keymaps.lua`, like nvim.
    pub fn load_file(&self, path: &Path) -> Result<(), String> {
        let src = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some(dir) = path.parent() {
            *self.config_dir.borrow_mut() = Some(dir.to_path_buf());
            let lua_dir = dir.join("lua");
            let package: Table = self.lua.globals().get("package").map_err(lua_error)?;
            let old: String = package.get("path").map_err(lua_error)?;
            let new = format!("{0}/?.lua;{0}/?/init.lua;{old}", lua_dir.display());
            package.set("path", new).map_err(lua_error)?;
        }
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        self.exec(&src, &name)
    }

    /// Call the Lua function of a mapping.
    pub fn call(&self, id: usize) -> Result<(), String> {
        let f = self.callbacks.borrow().get(id).cloned();
        let Some(f) = f else {
            return Err(format!("no Lua function #{id}"));
        };
        f.call::<()>(()).map_err(lua_error)
    }

    /// Actions queued by Lua since the last call.
    pub fn take_actions(&self) -> Vec<Action> {
        std::mem::take(&mut *self.actions.borrow_mut())
    }
}

/// Short message for a Lua error: the first line, without Rust's wrappers.
fn lua_error(e: mlua::Error) -> String {
    let e = match e {
        mlua::Error::CallbackError { cause, .. } => (*cause).clone(),
        e => e,
    };
    let text = match &e {
        mlua::Error::RuntimeError(msg) | mlua::Error::SyntaxError { message: msg, .. } => {
            msg.clone()
        }
        other => other.to_string(),
    };
    text.lines().next().unwrap_or_default().to_owned()
}

/// Rust primitives behind the prelude. Each returns an error string (or
/// nil) instead of raising, so the prelude can raise at the right line.
fn native_api(
    lua: &Lua,
    options: Rc<RefCell<Options>>,
    keymap: Rc<RefCell<Keymap>>,
    callbacks: Rc<RefCell<Vec<Function>>>,
    actions: Rc<RefCell<Vec<Action>>>,
) -> mlua::Result<Table> {
    let t = lua.create_table()?;

    let opts = options.clone();
    t.set(
        "opt_get",
        lua.create_function(move |lua, name: String| {
            Ok(match opts.borrow().get(&name) {
                Some(Value::Bool(b)) => (LuaValue::Boolean(b), None),
                Some(Value::Int(n)) => (LuaValue::Number(n as f64), None),
                Some(Value::Str(s)) => (LuaValue::String(lua.create_string(&s)?), None),
                None => (LuaValue::Nil, Some(format!("unknown option '{name}'"))),
            })
        })?,
    )?;

    t.set(
        "opt_set",
        lua.create_function(move |_, (name, value): (String, LuaValue)| {
            let value = match value {
                LuaValue::Boolean(b) => Value::Bool(b),
                LuaValue::Integer(n) => Value::Int(n),
                LuaValue::Number(f) if f.fract() == 0.0 => Value::Int(f as i64),
                LuaValue::String(s) => Value::Str(s.to_str()?.to_owned()),
                other => {
                    return Ok(Some(format!(
                        "option '{name}' can't be set to a {}",
                        other.type_name()
                    )));
                }
            };
            Ok(options.borrow_mut().set(&name, value).err())
        })?,
    )?;

    let km = keymap.clone();
    t.set(
        "set_leader",
        lua.create_function(move |_, value: LuaValue| {
            let LuaValue::String(s) = value else {
                return Ok(Some("mapleader must be a string".to_owned()));
            };
            let s = s.to_str()?.to_owned();
            let leader = km.borrow().leader;
            Ok(match keys::parse_keys(&s, leader) {
                Ok(k) if k.len() == 1 => {
                    km.borrow_mut().leader = k[0];
                    None
                }
                Ok(_) => Some(format!("mapleader must be a single key, got {s:?}")),
                Err(e) => Some(e),
            })
        })?,
    )?;

    let km = keymap.clone();
    t.set(
        "map_set",
        lua.create_function(
            move |_, (modes, lhs, rhs, desc): (Vec<String>, String, LuaValue, Option<String>)| {
                let result = (|| {
                    let modes = parse_modes(&modes)?;
                    let lhs = keys::parse(&lhs)?;
                    let rhs = match rhs {
                        LuaValue::String(s) => {
                            Rhs::Keys(keys::parse(&s.to_str().map_err(|e| e.to_string())?)?)
                        }
                        LuaValue::Function(f) => {
                            let mut cbs = callbacks.borrow_mut();
                            cbs.push(f);
                            Rhs::Lua(cbs.len() - 1)
                        }
                        _ => return Err("rhs must be a string or a function".to_owned()),
                    };
                    km.borrow_mut().set(&modes, lhs, rhs, desc)
                })();
                Ok(result.err())
            },
        )?,
    )?;

    t.set(
        "map_del",
        lua.create_function(move |_, (modes, lhs): (Vec<String>, String)| {
            let result = (|| {
                let modes = parse_modes(&modes)?;
                let tokens = keys::parse(&lhs)?;
                keymap
                    .borrow_mut()
                    .del(&modes, &tokens)
                    .map_err(|_| format!("no mapping for {lhs:?}"))
            })();
            Ok(result.err())
        })?,
    )?;

    t.set(
        "action",
        lua.create_function(move |_, (kind, arg): (String, String)| {
            actions.borrow_mut().push(match kind.as_str() {
                "cmd" => Action::Cmd(arg),
                "feed" => Action::Feed(arg),
                _ => Action::Notify(arg),
            });
            Ok(())
        })?,
    )?;

    Ok(t)
}

/// Theme and syntax primitives: `colorscheme`, `theme_set`, `theme_name`,
/// `syntax_define`.
fn ui_api(
    lua: &Lua,
    t: &Table,
    shared: &Shared,
    config_dir: Rc<RefCell<Option<PathBuf>>>,
) -> mlua::Result<()> {
    let theme = shared.theme.clone();
    t.set(
        "colorscheme",
        lua.create_function(move |lua, name: String| {
            let dir = config_dir.borrow().clone();
            Ok(match load_theme(lua, dir.as_deref(), &name) {
                Ok(new) => {
                    *theme.borrow_mut() = new;
                    None
                }
                Err(e) => Some(e),
            })
        })?,
    )?;

    let theme = shared.theme.clone();
    t.set(
        "theme_set",
        lua.create_function(move |_, (group, spec): (String, Table)| {
            Ok(match parse_hl(&spec) {
                Ok(hl) => {
                    theme.borrow_mut().set(&group, hl);
                    None
                }
                Err(e) => Some(format!("{group}: {e}")),
            })
        })?,
    )?;

    let theme = shared.theme.clone();
    t.set(
        "theme_name",
        lua.create_function(move |_, ()| Ok(theme.borrow().name.clone()))?,
    )?;

    let syntaxes = shared.syntaxes.clone();
    t.set(
        "syntax_define",
        lua.create_function(move |_, (name, spec): (String, Table)| {
            Ok(match parse_syntax(&name, &spec) {
                Ok(syntax) => {
                    syntaxes.borrow_mut().define(syntax);
                    None
                }
                Err(e) => Some(format!("syntax {name:?}: {e}")),
            })
        })?,
    )?;
    Ok(())
}

/// Load theme `name`: `default`, then `<config>/themes/<name>.lua`, then the
/// built-in ones. A theme file returns a table of groups. Groups it leaves
/// out keep their `default` style.
fn load_theme(lua: &Lua, dir: Option<&Path>, name: &str) -> Result<Theme, String> {
    if name == "default" {
        return Ok(Theme::fallback());
    }
    let user = dir
        .map(|d| d.join("themes").join(format!("{name}.lua")))
        .filter(|p| p.exists());
    let (src, chunk) = if let Some(path) = user {
        let src = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        (src, format!("themes/{name}.lua"))
    } else if let Some((_, src)) = THEMES.iter().find(|(n, _)| *n == name) {
        ((*src).to_owned(), format!("runtime/themes/{name}.lua"))
    } else {
        return Err(format!("E185: Cannot find color scheme '{name}'"));
    };
    let value: LuaValue = lua
        .load(&src)
        .set_name(format!("={chunk}"))
        .eval()
        .map_err(lua_error)?;
    let LuaValue::Table(table) = value else {
        return Err(format!("{chunk}: must return a table of groups"));
    };
    let mut theme = Theme::fallback();
    theme.name = name.to_owned();
    for pair in table.pairs::<String, Table>() {
        let (group, spec) = pair.map_err(|_| format!("{chunk}: every group must be a table"))?;
        let hl = parse_hl(&spec).map_err(|e| format!("{chunk}: {group}: {e}"))?;
        theme.set(&group, hl);
    }
    Ok(theme)
}

/// Check that `t` only uses `allowed` keys, so typos are errors.
fn check_keys(t: &Table, allowed: &[&str]) -> Result<(), String> {
    for pair in t.pairs::<LuaValue, LuaValue>() {
        let (k, _) = pair.map_err(lua_error)?;
        if let LuaValue::String(k) = &k {
            let k = k.to_string_lossy();
            if !allowed.contains(&k.as_str()) {
                return Err(format!(
                    "unknown key '{k}' (expected one of: {})",
                    allowed.join(", ")
                ));
            }
        }
    }
    Ok(())
}

/// `{ fg = "#fff", bg = "red", bold = true }` or `{ link = "Keyword" }`.
fn parse_hl(spec: &Table) -> Result<Hl, String> {
    check_keys(spec, &["fg", "bg", "bold", "italic", "underline", "link"])?;
    let get_str = |k: &str| {
        spec.get::<Option<String>>(k)
            .map_err(|_| format!("'{k}' must be a string"))
    };
    let get_bool = |k: &str| {
        spec.get::<Option<bool>>(k)
            .map(Option::unwrap_or_default)
            .map_err(|_| format!("'{k}' must be a boolean"))
    };
    if let Some(link) = get_str("link")? {
        return Ok(Hl::Link(link));
    }
    let style = theme::make_style(
        get_str("fg")?.as_deref(),
        get_str("bg")?.as_deref(),
        get_bool("bold")?,
        get_bool("italic")?,
        get_bool("underline")?,
    )?;
    Ok(Hl::Style(style))
}

/// A string or a list of strings.
fn strings(v: LuaValue, key: &str) -> Result<Vec<String>, String> {
    let bad = || format!("'{key}' must be a string or a list of strings");
    match v {
        LuaValue::Nil => Ok(Vec::new()),
        LuaValue::String(s) => Ok(vec![s.to_str().map_err(|_| bad())?.to_owned()]),
        LuaValue::Table(t) => t
            .sequence_values::<String>()
            .collect::<mlua::Result<_>>()
            .map_err(|_| bad()),
        _ => Err(bad()),
    }
}

/// `{ prefix = group }`, longest prefix first.
fn prefix_rules(t: Option<Table>, key: &str) -> Result<Vec<(String, String)>, String> {
    let Some(t) = t else {
        return Ok(Vec::new());
    };
    let mut rules: Vec<(String, String)> = t
        .pairs::<String, String>()
        .collect::<mlua::Result<_>>()
        .map_err(|_| format!("'{key}' must map prefixes to group names"))?;
    rules.sort_by_key(|(p, _)| std::cmp::Reverse(p.chars().count()));
    Ok(rules)
}

/// Turn a `raster.syntax.define` table into a [`Syntax`].
fn parse_syntax(name: &str, t: &Table) -> Result<Syntax, String> {
    check_keys(
        t,
        &[
            "extensions",
            "filenames",
            "line_comment",
            "spans",
            "words",
            "sigils",
            "line_rules",
            "ident_end",
            "capitalized",
            "call",
            "definers",
            "macro_bang",
            "numbers",
            "operators",
        ],
    )?;
    let value = |k: &str| t.get::<LuaValue>(k).map_err(lua_error);
    let opt_str = |k: &str| {
        t.get::<Option<String>>(k)
            .map_err(|_| format!("'{k}' must be a string"))
    };
    let table = |k: &str| {
        t.get::<Option<Table>>(k)
            .map_err(|_| format!("'{k}' must be a table"))
    };

    let mut spans = Vec::new();
    if let Some(list) = table("spans")? {
        for (n, item) in list.sequence_values::<Table>().enumerate() {
            let item = item.map_err(|_| format!("spans[{}] must be a table", n + 1))?;
            spans.push(parse_span(&item).map_err(|e| format!("spans[{}]: {e}", n + 1))?);
        }
    }

    let mut words = HashMap::new();
    if let Some(groups) = table("words")? {
        for pair in groups.pairs::<String, LuaValue>() {
            let (group, list) =
                pair.map_err(|_| "'words' must map group names to lists".to_owned())?;
            for w in strings(list, &format!("words.{group}"))? {
                words.insert(w, group.clone());
            }
        }
    }

    Ok(Syntax {
        name: name.to_owned(),
        extensions: strings(value("extensions")?, "extensions")?,
        filenames: strings(value("filenames")?, "filenames")?,
        line_comments: strings(value("line_comment")?, "line_comment")?,
        spans,
        words,
        sigils: prefix_rules(table("sigils")?, "sigils")?,
        line_rules: prefix_rules(table("line_rules")?, "line_rules")?,
        ident_end: opt_str("ident_end")?.unwrap_or_default(),
        capitalized: opt_str("capitalized")?,
        call: opt_str("call")?,
        definers: strings(value("definers")?, "definers")?,
        macro_bang: opt_str("macro_bang")?,
        numbers: t
            .get::<Option<bool>>("numbers")
            .map_err(|_| "'numbers' must be a boolean".to_owned())?
            .unwrap_or(true),
        operators: opt_str("operators")?.unwrap_or_default(),
    })
}

/// `{ open, close, group, escape = "\\", multiline = true, max_len = 4 }`.
fn parse_span(t: &Table) -> Result<Span, String> {
    check_keys(t, &["escape", "multiline", "max_len"])?;
    let pos = |i: usize, what: &str| {
        t.get::<String>(i)
            .map_err(|_| format!("item {i} ({what}) must be a string"))
    };
    let escape = t
        .get::<Option<String>>("escape")
        .map_err(|_| "'escape' must be a string".to_owned())?
        .map(|e| {
            let mut chars = e.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Ok(c),
                _ => Err("'escape' must be one char".to_owned()),
            }
        })
        .transpose()?;
    let open = pos(1, "open")?;
    if open.is_empty() {
        return Err("open must not be empty".into());
    }
    Ok(Span {
        open,
        close: pos(2, "close")?,
        group: pos(3, "group")?,
        escape,
        multiline: t
            .get::<Option<bool>>("multiline")
            .map_err(|_| "'multiline' must be a boolean".to_owned())?
            .unwrap_or(false),
        max_len: t
            .get::<Option<usize>>("max_len")
            .map_err(|_| "'max_len' must be a number".to_owned())?,
    })
}

fn parse_modes(modes: &[String]) -> Result<Vec<MapMode>, String> {
    let mut out = Vec::new();
    for m in modes {
        out.extend(MapMode::parse(m)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Match;
    use crate::keys::Key;

    struct Env {
        cfg: Config,
        options: Rc<RefCell<Options>>,
        keymap: Rc<RefCell<Keymap>>,
    }

    fn env() -> Env {
        let shared = Shared::default();
        let cfg = Config::new(&shared).unwrap();
        Env {
            cfg,
            options: shared.options,
            keymap: shared.keymap,
        }
    }

    fn keys(s: &str) -> Vec<Key> {
        keys::parse_keys(s, Key::char(' ')).unwrap()
    }

    #[test]
    fn options_set_and_get() {
        let e = env();
        e.cfg
            .exec(
                "raster.opt.number = true\nraster.opt.tabstop = 2\nraster.opt.timeoutlen = 500\nassert(raster.opt.tabstop == 2)",
                "init.lua",
            )
            .unwrap();
        let o = e.options.borrow();
        assert!(o.number);
        assert_eq!((o.tabstop, o.timeoutlen), (2, 500));
    }

    #[test]
    fn option_errors_point_at_the_line() {
        let e = env();
        let err = e
            .cfg
            .exec("\nraster.opt.mouse = 'a'", "init.lua")
            .unwrap_err();
        assert_eq!(err, "init.lua:2: unknown option 'mouse'");
        let err = e.cfg.exec("raster.opt.number = 1", "init.lua").unwrap_err();
        assert!(
            err.starts_with("init.lua:1: option 'number' expects a boolean"),
            "{err}"
        );
        let err = e
            .cfg
            .exec("local x = raster.opt.nope", "init.lua")
            .unwrap_err();
        assert_eq!(err, "init.lua:1: unknown option 'nope'");
    }

    #[test]
    fn syntax_errors_are_reported() {
        let e = env();
        let err = e
            .cfg
            .exec("raster.opt.number = = true", "init.lua")
            .unwrap_err();
        assert!(err.starts_with("init.lua:1:"), "{err}");
    }

    #[test]
    fn mapleader() {
        let e = env();
        e.cfg
            .exec(
                "raster.g.mapleader = ','\nassert(raster.g.mapleader == ',')",
                "init.lua",
            )
            .unwrap();
        assert_eq!(e.keymap.borrow().leader, Key::char(','));
        // Setting it again still goes through (the proxy table stays empty).
        e.cfg
            .exec("raster.g.mapleader = '<Space>'", "init.lua")
            .unwrap();
        assert_eq!(e.keymap.borrow().leader, Key::char(' '));
        assert!(e.cfg.exec("raster.g.mapleader = 'ab'", "init.lua").is_err());
        e.cfg
            .exec(
                "raster.g.anything = 42\nassert(raster.g.anything == 42)",
                "init.lua",
            )
            .unwrap();
    }

    #[test]
    fn keymap_set_with_keys_and_modes() {
        let e = env();
        e.cfg
            .exec(
                r#"
                raster.keymap.set("n", "<leader>w", ":w<CR>", { desc = "Save", silent = true })
                raster.keymap.set({ "n", "v" }, "H", "0")
                raster.keymap.set("i", "jk", "<Esc>")
                "#,
                "init.lua",
            )
            .unwrap();
        let km = e.keymap.borrow();
        assert!(matches!(
            km.matches(MapMode::Normal, &keys(" w"), false),
            Match::Mapped { len: 2, .. }
        ));
        assert!(matches!(
            km.matches(MapMode::Normal, &keys("H"), false),
            Match::Mapped { .. }
        ));
        assert!(
            km.mappings(MapMode::Visual)
                .iter()
                .any(|m| m.lhs == keys::parse("H").unwrap())
        );
        assert_eq!(km.matches(MapMode::Insert, &keys("j"), false), Match::Wait);
        let save = km
            .mappings(MapMode::Normal)
            .iter()
            .find(|m| m.desc.as_deref() == Some("Save"));
        assert!(save.is_some());
    }

    #[test]
    fn keymap_errors() {
        let e = env();
        let err = e
            .cfg
            .exec("raster.keymap.set('q', 'x', 'y')", "init.lua")
            .unwrap_err();
        assert!(err.starts_with("init.lua:1: unsupported mode"), "{err}");
        let err = e
            .cfg
            .exec("raster.keymap.set('n', '<Foo>', 'y')", "init.lua")
            .unwrap_err();
        assert!(err.contains("unknown key <Foo>"), "{err}");
        let err = e
            .cfg
            .exec(
                "raster.keymap.set('n', 'x', 'y', { remap = true })",
                "init.lua",
            )
            .unwrap_err();
        assert!(err.contains("remap"), "{err}");
        let err = e
            .cfg
            .exec("raster.keymap.set('n', 'x', 42)", "init.lua")
            .unwrap_err();
        assert!(err.contains("rhs must be"), "{err}");
    }

    #[test]
    fn keymap_del() {
        let e = env();
        e.cfg
            .exec("raster.keymap.del('n', 't')", "init.lua")
            .unwrap();
        assert_eq!(
            e.keymap
                .borrow()
                .matches(MapMode::Normal, &keys("t"), false),
            Match::Unmapped
        );
        let err = e
            .cfg
            .exec("raster.keymap.del('n', 't')", "init.lua")
            .unwrap_err();
        assert!(err.contains("no mapping"), "{err}");
    }

    #[test]
    fn function_mappings_queue_actions() {
        let e = env();
        e.cfg
            .exec(
                r#"
                raster.keymap.set("n", "<leader>x", function()
                  raster.cmd("w")
                  raster.feedkeys("dd")
                  print("done", 1)
                end)
                "#,
                "init.lua",
            )
            .unwrap();
        let rhs = match e
            .keymap
            .borrow()
            .matches(MapMode::Normal, &keys(" x"), false)
        {
            Match::Mapped {
                rhs: Rhs::Lua(id), ..
            } => id,
            other => panic!("{other:?}"),
        };
        assert!(e.cfg.take_actions().is_empty());
        e.cfg.call(rhs).unwrap();
        assert_eq!(
            e.cfg.take_actions(),
            vec![
                Action::Cmd("w".into()),
                Action::Feed("dd".into()),
                Action::Notify("done\t1".into())
            ]
        );
    }

    #[test]
    fn runtime_errors_in_callbacks() {
        let e = env();
        e.cfg
            .exec(
                "raster.keymap.set('n', 'X', function() error('boom') end)",
                "init.lua",
            )
            .unwrap();
        let err = e.cfg.call(0).unwrap_err();
        assert_eq!(err, "init.lua:1: boom");
        assert!(e.cfg.call(99).is_err());
    }

    #[test]
    fn load_file_and_require() {
        let dir = std::env::temp_dir().join(format!("raster_cfg_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("lua")).unwrap();
        std::fs::write(dir.join("lua").join("opts.lua"), "raster.opt.scrolloff = 3").unwrap();
        std::fs::write(
            dir.join("init.lua"),
            "require('opts')\nraster.opt.number = true",
        )
        .unwrap();

        let e = env();
        e.cfg.load_file(&dir.join("init.lua")).unwrap();
        assert_eq!(e.options.borrow().scrolloff, 3);
        assert!(e.options.borrow().number);
        assert!(e.cfg.load_file(&dir.join("missing.lua")).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn default_path_uses_xdg() {
        let p = default_path().unwrap();
        assert!(p.ends_with("raster/init.lua"));
    }

    fn shared_env() -> (Config, Shared) {
        let shared = Shared::default();
        let cfg = Config::new(&shared).unwrap();
        (cfg, shared)
    }

    /// Highlight `line` with the built-in syntax `lang`, as `[Group:text]` marks.
    fn marks(shared: &Shared, lang: &str, line: &str) -> String {
        let set = shared.syntaxes.borrow();
        let syntax = set.get(lang).unwrap_or_else(|| panic!("no syntax {lang}"));
        let chars: Vec<char> = line.chars().collect();
        let (hl, _) = syntax.highlight_line(&chars, None);
        let mut out = String::new();
        let mut i = 0;
        for (r, g) in hl {
            out.extend(&chars[i..r.start]);
            out.push_str(&format!(
                "[{g}:{}]",
                chars[r.clone()].iter().collect::<String>()
            ));
            i = r.end;
        }
        out.extend(&chars[i..]);
        out
    }

    #[test]
    fn default_theme_is_raster_violet() {
        let (cfg, shared) = shared_env();
        assert_eq!(shared.theme.borrow().name, "raster-violet");
        let kw = shared.theme.borrow().style("Keyword");
        assert_eq!(kw.fg, Some(ratatui::style::Color::Rgb(0xcb, 0xa6, 0xf7)));
        cfg.exec("assert(raster.theme.name() == 'raster-violet')", "t")
            .unwrap();
    }

    #[test]
    fn colorscheme_switches_and_reports_missing() {
        let (cfg, shared) = shared_env();
        cfg.colorscheme("default").unwrap();
        assert_eq!(shared.theme.borrow().name, "default");
        assert_eq!(
            cfg.colorscheme("nope"),
            Err("E185: Cannot find color scheme 'nope'".into())
        );
        let err = cfg
            .exec("\nraster.colorscheme('nope')", "init.lua")
            .unwrap_err();
        assert_eq!(err, "init.lua:2: E185: Cannot find color scheme 'nope'");
    }

    #[test]
    fn theme_set_and_links() {
        let (cfg, shared) = shared_env();
        cfg.exec(
            r##"
            raster.theme.set("Comment", { fg = "#ff0000", bold = true })
            raster.theme.set("Todo", { link = "Comment" })
            "##,
            "init.lua",
        )
        .unwrap();
        let t = shared.theme.borrow();
        assert_eq!(
            t.style("Todo").fg,
            Some(ratatui::style::Color::Rgb(255, 0, 0))
        );
        drop(t);
        let err = cfg
            .exec("raster.theme.set('Comment', { color = 'red' })", "init.lua")
            .unwrap_err();
        assert!(err.contains("Comment: unknown key 'color'"), "{err}");
        let err = cfg
            .exec(
                "raster.theme.set('Comment', { fg = 'violetish' })",
                "init.lua",
            )
            .unwrap_err();
        assert!(err.contains("invalid color"), "{err}");
    }

    #[test]
    fn user_themes_from_the_config_folder() {
        let dir = std::env::temp_dir().join(format!("raster_theme_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("themes")).unwrap();
        std::fs::write(
            dir.join("themes").join("mine.lua"),
            "return { Keyword = { fg = '#010203' }, Function = { link = 'Keyword' } }",
        )
        .unwrap();
        std::fs::write(dir.join("themes").join("broken.lua"), "return 42").unwrap();
        std::fs::write(dir.join("init.lua"), "raster.colorscheme('mine')").unwrap();

        let (cfg, shared) = shared_env();
        cfg.load_file(&dir.join("init.lua")).unwrap();
        let t = shared.theme.borrow().clone();
        assert_eq!(t.name, "mine");
        assert_eq!(
            t.style("Function").fg,
            Some(ratatui::style::Color::Rgb(1, 2, 3))
        );
        // Groups the theme leaves out keep the default style.
        assert_eq!(t.style("ModeInsert"), Theme::fallback().style("ModeInsert"));
        assert_eq!(
            cfg.colorscheme("broken"),
            Err("themes/broken.lua: must return a table of groups".into())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn syntax_define_from_lua() {
        let (cfg, shared) = shared_env();
        cfg.exec(
            r#"
            raster.syntax.define("toy", {
              extensions = { "toy" },
              line_comment = ";",
              spans = { { '"', '"', "String", escape = "\\" } },
              words = { Keyword = { "do", "end" } },
              sigils = { ["@"] = "Special" },
            })
            "#,
            "init.lua",
        )
        .unwrap();
        assert_eq!(
            marks(&shared, "toy", r#"do @x "s" end ; c"#),
            r#"[Keyword:do] [Special:@x] [String:"s"] [Keyword:end] [Comment:; c]"#
        );
        assert!(
            shared
                .syntaxes
                .borrow()
                .detect(Path::new("a.toy"))
                .is_some()
        );
    }

    #[test]
    fn syntax_define_errors() {
        let (cfg, _) = shared_env();
        let err = cfg
            .exec(
                "raster.syntax.define('x', { extension = { 'x' } })",
                "init.lua",
            )
            .unwrap_err();
        assert!(err.contains("unknown key 'extension'"), "{err}");
        let err = cfg
            .exec(
                "raster.syntax.define('x', { spans = { { '\"' } } })",
                "init.lua",
            )
            .unwrap_err();
        assert!(err.contains("spans[1]: item 2 (close)"), "{err}");
        let err = cfg
            .exec(
                "raster.syntax.define('x', { spans = { { 'a', 'b', 'C', escape = 'xy' } } })",
                "init.lua",
            )
            .unwrap_err();
        assert!(err.contains("one char"), "{err}");
        assert!(cfg.exec("raster.syntax.define('x')", "init.lua").is_err());
    }

    #[test]
    fn built_in_syntaxes_are_detected() {
        let (_, shared) = shared_env();
        let set = shared.syntaxes.borrow();
        for (file, lang) in [
            ("main.rs", "rust"),
            ("init.lua", "lua"),
            ("app.py", "python"),
            ("main.go", "go"),
            ("app.rb", "ruby"),
            ("Gemfile", "ruby"),
            ("mix.exs", "elixir"),
            ("app.gleam", "gleam"),
            ("README.md", "markdown"),
        ] {
            assert_eq!(
                set.detect(Path::new(file)).map(|s| s.name.as_str()),
                Some(lang),
                "{file}"
            );
        }
    }

    #[test]
    fn rust_snippet() {
        let (_, s) = shared_env();
        assert_eq!(
            marks(
                &s,
                "rust",
                "pub fn run(x: &'a str) -> Option<u8> { println!(\"{x}\"); None }"
            ),
            "[Keyword:pub] [Keyword:fn] [Function:run](x: [Operator:&]'a [Type:str]) [Operator:-][Operator:>] [Type:Option][Operator:<][Type:u8][Operator:>] { [PreProc:println!]([String:\"{x}\"]); [Constant:None] }"
        );
        assert_eq!(
            marks(&s, "rust", "#[derive(Debug)] // ok"),
            "[PreProc:#[derive(Debug)]] [Comment:// ok]"
        );
    }

    #[test]
    fn lua_snippet() {
        let (_, s) = shared_env();
        assert_eq!(
            marks(&s, "lua", "local function f(x) return x ~= nil end -- c"),
            "[Keyword:local] [Keyword:function] [Function:f](x) [Keyword:return] x [Operator:~][Operator:=] [Constant:nil] [Keyword:end] [Comment:-- c]"
        );
        assert_eq!(
            marks(&s, "lua", "raster.opt.nu = [[x]]"),
            "[Special:raster].opt.nu [Operator:=] [String:[[x]]]"
        );
    }

    #[test]
    fn python_snippet() {
        let (_, s) = shared_env();
        assert_eq!(marks(&s, "python", "@cache"), "[PreProc:@cache]");
        assert_eq!(
            marks(&s, "python", "def run(self): return None  # done"),
            "[Keyword:def] [Function:run]([Special:self]): [Keyword:return] [Constant:None]  [Comment:# done]"
        );
        assert_eq!(
            marks(&s, "python", "x = \"\"\"doc\"\"\""),
            "x [Operator:=] [String:\"\"\"doc\"\"\"]"
        );
    }

    #[test]
    fn go_snippet() {
        let (_, s) = shared_env();
        assert_eq!(
            marks(&s, "go", "func (s *Server) Start() error { return nil }"),
            "[Keyword:func] (s [Operator:*]Server) [Function:Start]() [Type:error] { [Keyword:return] [Constant:nil] }"
        );
        assert_eq!(
            marks(&s, "go", "x := `raw` + 'a'"),
            "x [Operator::][Operator:=] [String:`raw`] [Operator:+] [String:'a']"
        );
    }

    #[test]
    fn ruby_snippet() {
        let (_, s) = shared_env();
        assert_eq!(
            marks(&s, "ruby", "def empty? = @items.empty? && :ok # c"),
            "[Keyword:def] [Function:empty?] [Operator:=] [Special:@items].empty? [Operator:&][Operator:&] [Constant::ok] [Comment:# c]"
        );
        assert_eq!(
            marks(&s, "ruby", "Foo::Bar.new"),
            "[Type:Foo]::[Type:Bar].new"
        );
    }

    #[test]
    fn elixir_snippet() {
        let (_, s) = shared_env();
        assert_eq!(
            marks(&s, "elixir", "defp run(x), do: {:ok, Enum.map(x)} # c"),
            "[Keyword:defp] [Function:run](x), [Keyword:do]: {[Constant::ok], [Type:Enum].[Function:map](x)} [Comment:# c]"
        );
        assert_eq!(
            marks(&s, "elixir", "@moduledoc \"\"\""),
            "[PreProc:@moduledoc] [String:\"\"\"]"
        );
    }

    #[test]
    fn gleam_snippet() {
        let (_, s) = shared_env();
        assert_eq!(
            marks(&s, "gleam", "pub fn main() -> Nil { io.println(\"hi\") }"),
            "[Keyword:pub] [Keyword:fn] [Function:main]() [Operator:-][Operator:>] [Constant:Nil] { io.[Function:println]([String:\"hi\"]) }"
        );
    }

    #[test]
    fn markdown_snippet() {
        let (_, s) = shared_env();
        assert_eq!(marks(&s, "markdown", "## Keys"), "[Title:## Keys]");
        assert_eq!(
            marks(
                &s,
                "markdown",
                "Run `ras` with **care** and *joy* [docs](x)"
            ),
            "Run [String:`ras`] with [Bold:**care**] and [Italic:*joy*] [Function:[docs]](x)"
        );
        // A list marker is not italic, and numbers are plain text.
        assert_eq!(marks(&s, "markdown", "* step 2"), "* step 2");
    }
}
