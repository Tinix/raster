raster.syntax.define("python", {
  extensions = { "py", "pyi" },
  line_comment = { "#" },
  spans = {
    { '"""', '"""', "String", escape = "\\", multiline = true },
    { "'''", "'''", "String", escape = "\\", multiline = true },
    { '"', '"', "String", escape = "\\" },
    { "'", "'", "String", escape = "\\" },
  },
  words = {
    Keyword = {
      "and", "as", "assert", "async", "await", "break", "case", "class", "continue",
      "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
      "if", "import", "in", "is", "lambda", "match", "nonlocal", "not", "or",
      "pass", "raise", "return", "try", "while", "with", "yield",
    },
    Constant = { "True", "False", "None" },
    Special = { "self", "cls" },
  },
  sigils = { ["@"] = "PreProc" },
  capitalized = "Type",
  call = "Function",
  definers = { "def" },
  operators = "+-*/%=<>!&|^~",
})
