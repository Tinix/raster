raster.syntax.define("ruby", {
  extensions = { "rb", "rake", "gemspec", "ru" },
  filenames = { "Gemfile", "Rakefile", "Guardfile" },
  line_comment = { "#" },
  spans = {
    { '"', '"', "String", escape = "\\", multiline = true },
    { "'", "'", "String", escape = "\\", multiline = true },
  },
  words = {
    Keyword = {
      "alias", "and", "begin", "break", "case", "class", "def", "defined?", "do",
      "else", "elsif", "end", "ensure", "for", "if", "in", "module", "next", "not",
      "or", "redo", "rescue", "retry", "return", "super", "then", "undef",
      "unless", "until", "when", "while", "yield", "require", "require_relative",
      "include", "extend", "attr_accessor", "attr_reader", "attr_writer",
      "private", "protected", "public", "raise",
    },
    Constant = { "true", "false", "nil" },
    Special = { "self" },
  },
  sigils = { [":"] = "Constant", ["@@"] = "Special", ["@"] = "Special", ["$"] = "Special" },
  ident_end = "?!",
  capitalized = "Type",
  call = "Function",
  definers = { "def" },
  operators = "+-*/%=<>!&|^~",
})
