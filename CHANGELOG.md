# Changelog

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

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
