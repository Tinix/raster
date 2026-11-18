raster.syntax.define("gleam", {
  extensions = { "gleam" },
  line_comment = { "//" },
  spans = {
    { '"', '"', "String", escape = "\\", multiline = true },
  },
  words = {
    Keyword = {
      "as", "assert", "case", "const", "echo", "fn", "if", "import", "let",
      "opaque", "panic", "pub", "todo", "type", "use",
    },
    Constant = { "True", "False", "Nil" },
  },
  sigils = { ["@"] = "PreProc" },
  capitalized = "Type",
  call = "Function",
  definers = { "fn" },
  operators = "+-*/%=<>!&|",
})
