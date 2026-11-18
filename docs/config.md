# Configuration

Raster runs `init.lua` at startup with LuaJIT. The API is one global table,
`raster`, shaped like nvim's `vim.opt`, `vim.g` and `vim.keymap`, so nvim
configs port over almost line by line. A full example lives in
[init.example.lua](init.example.lua).

## Where the config lives

| Path | |
|---|---|
| `$XDG_CONFIG_HOME/raster/init.lua` | default, when `XDG_CONFIG_HOME` is set |
| `~/.config/raster/init.lua` | default otherwise |
| `~/.config/raster/lua/*.lua` | modules for `require("name")`, like nvim |

```sh
ras file.txt                  # loads the default init.lua if it exists
ras -u path/to/init.lua file  # loads this file instead
ras --clean file              # no config: built-in defaults only
```

If `init.lua` has an error, Raster still starts. The error shows on the
message line with its file and line (`init.lua:3: unknown option 'mouse'`),
and everything before that line still applies.

## `raster.opt`

```lua
raster.opt.number = true
print(raster.opt.tabstop)  --> 8
```

| Option | Type | Default | Effect |
|---|---|---|---|
| `number` | boolean | `false` | line numbers in a gutter |
| `relativenumber` | boolean | `false` | line numbers relative to the cursor line |
| `scrolloff` | integer | `0` | lines of context kept above and below the cursor |
| `tabstop` | integer ≥ 1 | `8` | display width of a tab |
| `expandtab` | boolean | `false` | `<Tab>` in Insert mode inserts spaces |
| `shiftwidth` | integer | `8` | indent width (for the indent operators, later) |
| `autoindent` | boolean | `true` | `Enter`, `o`, `O` and `cc` keep the indent |
| `timeoutlen` | integer (ms) | `1000` | wait for the rest of a mapped key sequence |

Defaults are nvim's. Setting an unknown option or a value of the wrong type
is an error. Vim's short names work too: `nu`, `rnu`, `so`, `ts`, `sw`, `et`,
`ai`, `tm` (`raster.opt.nu = true`).

## `:set`

The same options can be changed while editing, with Vim's syntax:

| Command | Effect |
|---|---|
| `:set nu` / `:set number` | turn a boolean option on (here: show line numbers) |
| `:set nonu` / `:set nonumber` | turn it off |
| `:set nu!` / `:set invnu` | toggle it |
| `:set nu?` | show its value (`number` or `nonumber`) |
| `:set ts=4` / `:set ts:4` | set a number option |
| `:set ts` / `:set ts?` | show a number option |
| `:set nu&` | back to the default |
| `:set nu rnu ts=2` | several at once |
| `:set` | list the options that differ from the defaults |

`:se` is short for `:set`. Errors use Vim's codes (`E518: Unknown option`).

## `raster.g`

A table for global variables. `raster.g.mapleader` sets the key that
`<leader>` stands for (default `<Space>`). It must be a single key, either
literal (`" "`, `","`) or in notation (`"<Space>"`).

`<leader>` is resolved when a key is typed, not when a mapping is
defined, so changing `mapleader` also moves the default `<leader>q` and
`<leader>Q`.

## `raster.keymap`

```lua
raster.keymap.set(mode, lhs, rhs, opts)
raster.keymap.del(mode, lhs)
```

- **`mode`**: `"n"` Normal, `"i"` Insert, `"o"` operator-pending, `"v"` / `"x"`
  Visual (stored until Visual mode exists), `""` for n + v + o, or a list
  like `{ "n", "v" }`.
- **`lhs`**: Vim key notation: `"<leader>ff"`, `"<C-s>"`, `"<S-A-t>"`, `"jk"`.
- **`rhs`**: keys to type (`":w<CR>"`, `"<Esc>"`, `"<Nop>"`) or a Lua
  function. Keys from `rhs` are not remapped (nvim's `noremap`, the
  default for `vim.keymap.set`).
- **`opts`**: `desc` is stored; `silent` and `noremap` are accepted and do
  nothing (their behavior is always on). `remap = true` is an error for now.

When one mapping is the start of another (`<Space>` and `<leader>q`), Raster
waits up to `timeoutlen` for the next key, like nvim.

### Default mappings

These come from the author's nvim config. Delete or override any of them.

| Mode | lhs | rhs |
|---|---|---|
| n, v | `<Space>` | `<Nop>` |
| n, v | `t` / `n` / `s` | `j` / `k` / `l` (Dvorak) |
| n | `<C-s>` | `:w<CR>` |
| n | `<leader>q` | `:q<CR>` |
| n | `<leader>Q` | `:qa!<CR>` |

To use Vim's `t` (till), `n` (next match) and `s` (substitute) everywhere:

```lua
for _, key in ipairs({ "t", "n", "s" }) do
  raster.keymap.del({ "n", "v" }, key)
end
```

### Key notation

`<CR>` `<Enter>` `<Esc>` `<Tab>` `<BS>` `<Del>` `<Space>` `<lt>` `<Bar>`
`<Bslash>` `<Up>` `<Down>` `<Left>` `<Right>` `<Home>` `<End>` `<PageUp>`
`<PageDown>` `<Insert>` `<F1>`…`<F12>` `<Nop>` `<leader>`, with
modifiers `C-`, `S-`, `A-` / `M-` (`<C-S-j>`). Names are case-insensitive.

## Calling the editor from Lua

| Function | |
|---|---|
| `raster.cmd("w")` | run an ex-command |
| `raster.feedkeys("dd")` | type keys (notation, not remapped) |
| `raster.notify("msg")` | show a message; `print(...)` does the same |

Lua functions queue these requests, and the editor runs them right after the
function returns. Reading editor state from Lua (cursor, lines) comes later.
