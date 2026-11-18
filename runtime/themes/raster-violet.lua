-- raster-violet: Raster's default theme. A dark violet base with soft
-- pastel accents. Needs a terminal with true color (24-bit) support.
local c = {
  bg = "#1b1726",
  bg_alt = "#272136",
  fg = "#e2dcf2",
  muted = "#71678c",
  subtle = "#4b4362",
  violet = "#b893f5",
  purple = "#cba6f7",
  pink = "#f5a8d4",
  blue = "#8fb3ff",
  cyan = "#8be0e8",
  green = "#a8e6a3",
  yellow = "#f3d18b",
  orange = "#f8a477",
  red = "#f38ba8",
}

return {
  -- Editor UI
  Normal = { fg = c.fg, bg = c.bg },
  LineNr = { fg = c.subtle },
  CursorLineNr = { fg = c.violet, bold = true },
  StatusLine = { fg = c.fg, bg = c.bg_alt },
  StatusModified = { fg = c.yellow, bg = c.bg_alt, bold = true },
  ModeNormal = { fg = c.bg, bg = c.violet, bold = true },
  ModeInsert = { fg = c.bg, bg = c.green, bold = true },
  ModeCommand = { fg = c.bg, bg = c.yellow, bold = true },

  -- Syntax
  Comment = { fg = c.muted, italic = true },
  String = { fg = c.green },
  Number = { fg = c.orange },
  Constant = { fg = c.orange },
  Keyword = { fg = c.purple, bold = true },
  Type = { fg = c.pink },
  Function = { fg = c.blue },
  Operator = { fg = c.cyan },
  PreProc = { fg = c.red },
  Special = { fg = c.yellow },

  -- Markdown
  Title = { fg = c.violet, bold = true },
  Bold = { bold = true },
  Italic = { italic = true },
}
