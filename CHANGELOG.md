# Changelog

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Word motions `w` `b` `e` and `W` `B` `E`, plus `^`, `gg`, `G`, `{count}G`,
  `f{char}` and `F{char}`. Motions take counts (`3w`, `5t`).
- `I`, `A`, `o` and `O` to enter Insert mode (`o`/`O` keep the indent).
- Operators `d`, `c` and `y` with any motion or the `iw` / `aw` / `iW` / `aW`
  text objects; `dd`, `cc`, `yy`, `x`, `D` and `C`; counts on both sides
  (`2d3w`).
- `p` / `P` put from the unnamed register, charwise or linewise.
- `.` repeats the last change, including the text typed after it.
- `u` / `<C-r>` undo and redo; an Insert session is one undo step, and
  undoing back to the saved state clears the dirty marker.
- Operator-pending key parser (`vim::parser`) that later steps can extend.
- `init.lua` config, run with LuaJIT from `~/.config/raster/init.lua` (or
  `$XDG_CONFIG_HOME/raster/`), with `require` from its `lua/` folder.
  `ras --clean` skips it and `ras -u <file>` picks another one.
- `raster.opt`: `number`, `relativenumber`, `scrolloff`, `tabstop`,
  `shiftwidth`, `expandtab`, `autoindent` and `timeoutlen`.
- `raster.g.mapleader`, and `raster.keymap.set` / `raster.keymap.del` for
  Normal, Insert, Operator-pending and (stored) Visual mappings, with key or
  Lua function right-hand sides. Half-typed mappings wait `timeoutlen`.
- `raster.cmd`, `raster.feedkeys` and `raster.notify` (and `print`) for Lua
  functions.
- `:set` with Vim's syntax: `:set nu` / `:set number` shows line numbers,
  plus `nonu`, `nu!`, `invnu`, `nu?`, `nu&`, `ts=4` and several at once.
  Short option names also work in `raster.opt`.
- Line-number gutter, tabs drawn at `tabstop` width, `<Tab>` in Insert mode,
  and autoindent on `Enter`.
- Vim's `t{char}` / `T{char}` (till) and `S`.

- `ras` binary: open a file (or an empty buffer) full screen in the terminal.
- Rope-backed buffer with open, save, insert and backspace.
- Normal, Insert and Command modes.
- Dvorak-friendly motions `h` `t` `n` `s`, arrow keys, `0` and `$`.
- `i` / `a` to enter Insert mode; typing, `Enter` and `Backspace` in Insert.
- Ex-commands `:w`, `:q`, `:q!`, `:qa!`, `:wq` and `:x`, with an error when
  quitting with unsaved changes.
- `<C-s>` to save, `<Space>q` / `<Space>Q` to quit / force quit.
- Status bar with a colored mode pill, file name, dirty marker and cursor position.
- `docs/keymaps.md`: default keys taken from the author's nvim config, with
  the roadmap step for each.

### Changed

- The Dvorak motions `t` `n` `s`, `<C-s>` and `<Space>q` / `<Space>Q` are now
  default mappings on top of Vim's keys, as in the author's nvim config. After
  an operator, `t` is Vim's till (`dt,`), not down.

### Fixed

- The cursor is placed correctly on lines with tabs.

- The final newline of a file no longer shows up as an extra empty line.
  It is written back on save, like Vim.
