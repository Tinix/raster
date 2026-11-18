raster.syntax.define("go", {
  extensions = { "go" },
  line_comment = { "//" },
  spans = {
    { "/*", "*/", "Comment", multiline = true },
    { '"', '"', "String", escape = "\\" },
    { "`", "`", "String", multiline = true },
    { "'", "'", "String", escape = "\\", max_len = 10 },
  },
  words = {
    Keyword = {
      "break", "case", "chan", "const", "continue", "default", "defer", "else",
      "fallthrough", "for", "func", "go", "goto", "if", "import", "interface",
      "map", "package", "range", "return", "select", "struct", "switch", "type", "var",
    },
    Type = {
      "any", "bool", "byte", "complex64", "complex128", "error", "float32",
      "float64", "int", "int8", "int16", "int32", "int64", "rune", "string",
      "uint", "uint8", "uint16", "uint32", "uint64", "uintptr",
    },
    Constant = { "true", "false", "nil", "iota" },
  },
  call = "Function",
  definers = { "func" },
  operators = "+-*/%=<>!&|^:",
})
