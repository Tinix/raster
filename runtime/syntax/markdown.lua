raster.syntax.define("markdown", {
  extensions = { "md", "markdown" },
  numbers = false,
  line_rules = { ["#"] = "Title", [">"] = "Comment" },
  spans = {
    { "```", "```", "String", multiline = true },
    -- Inline spans only count when they close on the same line, so a
    -- `* item` list marker is not italic.
    { "`", "`", "String", max_len = 500 },
    { "**", "**", "Bold", max_len = 500 },
    { "__", "__", "Bold", max_len = 500 },
    { "*", "*", "Italic", max_len = 500 },
    { "[", "]", "Function", max_len = 500 },
  },
})
