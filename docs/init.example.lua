-- init.lua: example Raster config, ported from the author's nvim config.
-- Copy it to ~/.config/raster/init.lua (or $XDG_CONFIG_HOME/raster/init.lua).
-- Files in ~/.config/raster/lua/ can be loaded with require("name").

-- Leader key (nvim: vim.g.mapleader = " ").
raster.g.mapleader = " "

-- Options (nvim: lua/config/options.lua). Only these exist so far;
-- setting any other name is an error.
raster.opt.number = true
raster.opt.relativenumber = false
raster.opt.expandtab = true
raster.opt.shiftwidth = 2
raster.opt.tabstop = 2
raster.opt.timeoutlen = 500
-- raster.opt.scrolloff = 0
-- raster.opt.autoindent = true

-- Raster's defaults already match nvim's lua/config/keymaps.lua:
--   <Space> -> <Nop>, t/n/s -> j/k/l (Dvorak, Normal and Visual),
--   <C-s> -> :w, <leader>q -> :q, <leader>Q -> :qa!
-- So `dt,` is Vim's "delete till ,". To use Vim's t/n/s everywhere instead:
-- for _, key in ipairs({ "t", "n", "s" }) do
--   raster.keymap.del({ "n", "v" }, key)
-- end

-- More mappings work the same as vim.keymap.set:
raster.keymap.set("n", "<leader>w", ":w<CR>", { desc = "Save file" })
raster.keymap.set("n", "<leader>x", ":x<CR>", { desc = "Save and quit" })

-- A Lua function as the right-hand side:
raster.keymap.set("n", "<leader>?", function()
  raster.notify("Raster: <leader>w save, <leader>q quit")
end, { desc = "Show a hint" })
