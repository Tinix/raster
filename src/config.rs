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
};
use mlua::{Function, Lua, Table, Value as LuaValue};
use std::{
    cell::RefCell,
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

pub struct Config {
    lua: Lua,
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
    /// A runtime whose `raster` API edits the shared `options` and `keymap`.
    pub fn new(options: Rc<RefCell<Options>>, keymap: Rc<RefCell<Keymap>>) -> Result<Self, String> {
        let lua = Lua::new();
        let callbacks: Rc<RefCell<Vec<Function>>> = Rc::default();
        let actions: Rc<RefCell<Vec<Action>>> = Rc::default();
        let native = native_api(&lua, options, keymap, callbacks.clone(), actions.clone())
            .map_err(lua_error)?;
        lua.load(PRELUDE)
            .set_name("=raster")
            .call::<()>(native)
            .map_err(lua_error)?;
        Ok(Self {
            lua,
            callbacks,
            actions,
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
        let options = Rc::new(RefCell::new(Options::default()));
        let keymap = Rc::new(RefCell::new(Keymap::default()));
        let cfg = Config::new(options.clone(), keymap.clone()).unwrap();
        Env {
            cfg,
            options,
            keymap,
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
}
