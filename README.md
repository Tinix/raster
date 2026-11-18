# Raster Editor

**Raster — a blazing-fast, Lua-extensible text editor built in Rust.**

> **Status:** early development. Raster is being built in the open, step by step.
> Expect breaking changes until `v1.0`.

## Fast

Written from scratch in Rust on top of a rope data structure, with a terminal UI.
No runtime, no garbage collector, no startup lag.

## Extensible

Configuration and plugins in Lua through a single global `raster` API,
plus **flow**, a built-in plugin manager. Themes are plain Lua tables.

## Familiar

Vim-style editing with Normal, Insert and Command modes, Dvorak-friendly
default motions, and a Zellij-inspired interface: tab bar, colored status bar,
key hints, panes and a floating terminal.

## Usage

```sh
cargo run --release -- path/to/file   # or: ras [--clean] [-u init.lua] path/to/file
```

## Keys

Defaults follow a Dvorak-friendly Vim layout. Leader is `<Space>`.
The full mapping and what comes next is in [docs/keymaps.md](docs/keymaps.md).

| Mode | Key | Action |
|---|---|---|
| Normal | `h` `t` `n` `s` / arrows | left, down, up, right (`j` `k` `l` work too) |
| Normal | `w` `b` `e` / `W` `B` `E` | word / WORD forward, back, end |
| Normal | `0` `^` `$` | line start, first non-blank, line end |
| Normal | `gg` / `G` / `{count}G` | first / last / Nth line |
| Normal | `f{char}` / `F{char}` | find char forward / backward |
| Normal | `T{char}`, and `t{char}` after an operator (`dt,`) | till char |
| Normal | `i` `a` `I` `A` `o` `O` | enter Insert (cursor, after, line start/end, new line) |
| Normal | `d` `c` `y` + motion or `iw` `aw` | delete, change, yank |
| Normal | `dd` `cc` `yy`, `x` `D` `C`, `S` | linewise ops, delete char, to end of line, change line |
| Normal | `p` / `P` | put after / before |
| Normal | `.` | repeat last change |
| Normal | `u` / `<C-r>` | undo / redo |
| Normal | `:` | Command mode |
| Normal | `<C-s>` | save |
| Normal | `<Space>q` / `<Space>Q` | quit / quit discarding changes |
| Insert | any char, `Enter`, `Backspace` | type, break line, delete |
| Insert | `Esc` | back to Normal |
| Command | `:w` | write |
| Command | `:q` / `:q!` | quit / quit discarding changes |
| Command | `:wq` / `:x` | write and quit |
| Command | `Esc` | cancel |

Motions and operators take counts: `3w`, `5t`, `2dd`, `d3w`.

The Dvorak keys, `<C-s>` and `<Space>q`/`Q` are default mappings on top of
Vim's keys, as in the author's nvim config. Change them in `init.lua`.

## Configuration

Raster runs `~/.config/raster/init.lua` at startup (LuaJIT). The API mirrors
nvim's:

```lua
raster.g.mapleader = " "
raster.opt.number = true
raster.opt.tabstop = 2
raster.keymap.set("n", "<leader>w", ":w<CR>", { desc = "Save file" })
raster.keymap.set("i", "jk", "<Esc>")
raster.keymap.set("n", "<leader>?", function() raster.notify("hi") end)
```

Run `ras --clean` to skip the config, or `ras -u file.lua` to use another
one. See [docs/config.md](docs/config.md) for every option and function, and
[docs/init.example.lua](docs/init.example.lua) for a full example.

## Roadmap

- [x] Open/save files, Normal/Insert/Command modes, basic motions
- [x] Operators and motions (`dw`, `ciw`, `yy`, `p`), undo/redo
- [x] `init.lua` with the `raster.opt` / `raster.keymap` API
- [ ] Lua themes, starting with `raster-violet`
- [ ] Buffers, tabs, splits and the status bar
- [ ] File and text pickers (fzf + ripgrep)
- [ ] Floating terminal
- [ ] flow v0.1 plugin manager.

## License

MIT © 2026 Daniel Tinivella ([@Tinix](https://github.com/Tinix))
