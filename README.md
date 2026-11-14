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
key hints, panes and a floating terminal## Roadmap

## Roadmap

- [ ] Open/save files, Normal/Insert/Command modes, basic motions
- [ ] Operators and motions (`dw`, `ciw`, `yy`, `p`), undo/redo
- [ ] `init.lua` with the `raster.opt` / `raster.keymap` API
- [ ] Lua themes, starting with `raster-violet`
- [ ] Buffers, tabs, splits and the status bar
- [ ] File and text pickers (fzf + ripgrep)
- [ ] Floating terminal
- [ ] flow v0.1 plugin manager.

## License

MIT © 2026 Daniel Tinivella ([@Tinix](https://github.com/Tinix))
