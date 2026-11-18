raster.syntax.define("lua", {
  extensions = { "lua" },
  line_comment = { "--" },
  spans = {
    { "--[[", "]]", "Comment", multiline = true },
    { "[[", "]]", "String", multiline = true },
    { '"', '"', "String", escape = "\\" },
    { "'", "'", "String", escape = "\\" },
  },
  words = {
    Keyword = {
      "and", "break", "do", "else", "elseif", "end", "for", "function", "goto",
      "if", "in", "local", "not", "or", "repeat", "return", "then", "until", "while",
    },
    Constant = { "true", "false", "nil" },
    Special = { "self", "raster" },
  },
  call = "Function",
  definers = { "function" },
  operators = "+-*/%^#=<>~",
})
