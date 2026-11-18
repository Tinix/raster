# Keymaps

Raster's default keys are taken from the author's Neovim config
(`~/.config/nvim`, Lua, Dvorak layout), so they feel familiar from day one.
This file is the source of truth for which mappings exist and which roadmap step
brings each one in.

**Leader:** `<Space>` (`vim.g.mapleader = " "`, also `maplocalleader`).
`<Space>` itself does nothing in Normal mode.

Status: **done** = in Raster today; **step N** = planned for roadmap step N;
**later** = wanted but not on a step yet.

Roadmap steps: 1 skeleton · 2 operators, motions, undo/redo ·
3 `init.lua` / `raster.keymap` (done) · 4 Lua themes · 5 buffers, tabs, splits,
status bar · 6 pickers (fzf + ripgrep) · 7 floating terminal · 8 flow.

## How Raster's keys are layered

Like nvim, Raster has two layers:

1. **Vim's built-in keys** (`h j k l`, `t` = till, `n` = next match,
   `s` = substitute, ...), handled by the key parser.
2. **Mappings** on top of them. Raster's defaults are the mappings from the
   nvim config below. `init.lua` can change or delete any of them
   (`raster.keymap.set` / `raster.keymap.del`, see [config.md](config.md)).

Mappings are per mode, so a Normal-mode mapping does not apply after an
operator: `dt,` is Vim's "delete till `,`", exactly as in nvim.

## Dvorak motions (default mappings)

| nvim mapping | Modes | Action | Raster |
|---|---|---|---|
| `h` → `h` | n, v | left | done (built-in `h`) |
| `t` → `j` | n, v | down | done (n) · v stored until Visual mode |
| `n` → `k` | n, v | up | done (n) · v stored until Visual mode |
| `s` → `l` | n, v | right | done (n) · v stored until Visual mode |
| `<Space>` → `<Nop>` | n, v | leader only | done |

Because the maps are Normal and Visual only:

| Key | In Normal mode | After an operator (`d`, `c`, `y`) |
|---|---|---|
| `t` | down (mapped) | till char: `dt,` (built-in) |
| `T` | till char backward (built-in, not mapped) | same |
| `n` | up (mapped) | next match: not implemented yet (search) |
| `s` | right (mapped) | nothing (Vim has no `s` motion) |
| `S` | change whole line (built-in, not mapped) | — |

## Vim editing core (built-ins, step 2)

Everything here takes a count (`3w`, `5t`, `2dd`, `d3w`, `2d3w` = 6 words).
`j` / `k` / `l` also work in Normal mode, like in nvim.

| Key | Action | Raster |
|---|---|---|
| `w` `b` `e` / `W` `B` `E` | word / WORD forward, back, end | done |
| `0` `^` `$` | line start, first non-blank, line end | done |
| `gg` / `G` / `{count}G` | first line / last line / line N | done |
| `f{char}` / `F{char}` | find char forward / backward on the line | done |
| `t{char}` / `T{char}` | till char forward / backward (`t` only after an operator, see above) | done (step 3) |
| `i` `a` `I` `A` | Insert at cursor, after, first non-blank, line end | done |
| `o` / `O` | open line below / above (keeps indent) | done |
| `Esc` | back to Normal | done |
| `Enter` / `Backspace` (Insert) | break line / delete back | done |
| `d` `c` `y` + motion | delete, change, yank (`dw`, `ce`, `y$`, `dfx`, `dgg`) | done |
| `iw` `aw` / `iW` `aW` | inner / a word or WORD (`diw`, `caw`) | done |
| `dd` `cc` `yy` | delete, change (keeps indent), yank lines | done |
| `x` / `D` / `C` | `dl` / `d$` / `c$` | done |
| `S` | `cc` (`s` is the Dvorak right) | done (step 3) |
| `p` / `P` | put after / before (unnamed register) | done |
| `.` | repeat last change, a count replaces its count | done |
| `u` / `<C-r>` | undo / redo; each Insert session is one step | done |
| `:` | Command mode | done |
| `;` `,` | repeat last `f` `F` `t` `T` | later |
| `/` `?` `n` `N` | search (`n` is up in Normal mode, as in nvim) | later |
| `v` `V` | Visual mode | later |
| `"{reg}` | named registers; nvim uses `clipboard=unnamedplus` | later |

## Files and quitting

| nvim key | Action | Raster |
|---|---|---|
| `<C-s>` | `:w` save file | done (default mapping) |
| `<leader>q` | `:q` quit | done (default mapping; refuses if unsaved) |
| `<leader>Q` | `:qa!` quit all, discard changes | done (default mapping; single buffer, acts as `:q!`) |
| `:w` `:q` `:q!` `:qa!` `:wq` `:x` | ex-commands | done |
| `:set nu` `:set nonu` `:set ts=4` … | set options (see [config.md](config.md)) | done |

## Lines and indent

| nvim key | Mode | Action | Raster |
|---|---|---|---|
| `<S-A-j>` / `<C-S-j>` / `<S-A-t>` | n, v | move line or selection down | later |
| `<S-A-k>` / `<C-S-k>` / `<S-A-n>` | n, v | move line or selection up | later |
| `<` / `>` | v | indent and keep the selection (`<gv` / `>gv`) | later (Visual) |
| `p` | v | paste over selection without yanking (`"_dP`) | later (Visual) |

## Windows and buffers

| nvim key | Action | Raster |
|---|---|---|
| `<C-h>` `<C-j>` `<C-k>` `<C-l>` | move to left / lower / upper / right window | step 5 |
| `<C-Up>` / `<C-Down>` | window height +2 / −2 | step 5 |
| `<C-Left>` / `<C-Right>` | window width −2 / +2 | step 5 |
| `<S-l>` / `<S-h>` | next / previous buffer | step 5 |

Note: `<C-h>`/`<C-j>`/`<C-k>`/`<C-l>` keep the QWERTY `hjkl` letters in the
nvim config, unlike the Dvorak `htns` motions. Raster keeps them as-is.

## Pickers (Telescope in nvim)

| nvim key | Action | Raster |
|---|---|---|
| `<leader>ff` | find files | step 6 |
| `<leader>fg` | live grep | step 6 |
| `<leader>fb` | find buffers | step 6 |
| `<leader>fh` | help tags | step 6 |

## File tree and jump list

| nvim key | Action | Raster |
|---|---|---|
| `<C-e>` | toggle file tree (nvim-tree) | step 6 (as a file picker / explorer) |
| `<leader>ha` | harpoon: add file | step 6 |
| `<leader>hm` | harpoon: quick menu | step 6 |
| `<leader>h1`…`<leader>h4` | harpoon: go to file 1–4 | step 6 |

## Plugin-only mappings (not planned)

These belong to nvim plugins that cover features Raster leaves out (Git UI,
AI chat, LSP completion). Users can bind them with `raster.keymap` if a
plugin provides the feature.

| nvim key | Action |
|---|---|
| `<leader>gs` `gc` `gp` `gl` `gd` | Fugitive: status, commit, push, pull, diff |
| `<leader>aa` `ar` `at`, `<leader>ae` (v) | Avante: ask, refresh, toggle, edit |
| `<leader>cc`, `<leader>c{e,g,t,k,d,a,o,s,f,x,r,l}` (v) | ChatGPT commands |
| `<leader>ct` | Copilot toggle (also used by ChatGPT translate in Visual) |
| `<C-Space>` / `<CR>` (Insert, cmp) | completion: trigger / confirm |

## Future `emacs_buffers` keyset (step 6, not implemented)

From `~/.config/emacs/init.el` (Evil, leader `SPC`). It mirrors the nvim
mappings. These are the buffer, window and tab bindings to reuse:

| Emacs key | Command | Action |
|---|---|---|
| `L` / `H` | `next-buffer` / `previous-buffer` | next / previous buffer |
| `<leader>fb` | `consult-buffer` | buffer picker |
| `<leader>ff` | `tinix-find-files` | find files |
| `<leader>fg` | `consult-ripgrep` | grep |
| `C-h` `C-j` `C-k` `C-l` | `evil-window-left/down/up/right` | move between windows |
| `C-<up>` / `C-<down>` | `evil-window-increase/decrease-height 2` | resize height |
| `C-<left>` / `C-<right>` | `evil-window-decrease/increase-width 2` | resize width |
| `<leader>tn` / `<leader>tc` | `tab-bar-new-tab` / `tab-bar-close-tab` | new / close tab |
| `<leader>w` | `evil-write-all` | write all buffers (`:wa`) |
| `<leader>q` / `<leader>Q` | `evil-quit` / `tinix-quit-all-force` | quit / quit all, force |
| `C-s` | `save-buffer` | save |
| `-` | `dired-jump` | open the file's directory |
| `<leader>ha` `hm` `h1`…`h4` | harpoon | add, menu, jump 1–4 |
| `C-e` | `treemacs` | file tree |

Window splits open below and to the right (`evil-split-window-below`,
`evil-vsplit-window-right`, matching nvim's `splitbelow` / `splitright`).

Other Emacs-only bindings worth noting: `<tab>` = `%` (jump to matching
bracket, Normal and Visual), `<f1>` = compile, `M-RET` / `<leader>la` `lr` `lf`
= eglot (LSP, out of scope).
