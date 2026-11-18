raster.syntax.define("elixir", {
  extensions = { "ex", "exs" },
  line_comment = { "#" },
  spans = {
    { '"""', '"""', "String", escape = "\\", multiline = true },
    { '"', '"', "String", escape = "\\", multiline = true },
    { "'", "'", "String", escape = "\\" },
  },
  words = {
    Keyword = {
      "after", "alias", "and", "case", "catch", "cond", "def", "defdelegate",
      "defexception", "defguard", "defimpl", "defmacro", "defmacrop", "defmodule",
      "defp", "defprotocol", "defstruct", "do", "else", "end", "fn", "for", "if",
      "import", "in", "not", "or", "quote", "raise", "receive", "require",
      "rescue", "try", "unless", "unquote", "use", "when", "with",
    },
    Constant = { "true", "false", "nil" },
  },
  sigils = { [":"] = "Constant", ["@"] = "PreProc" },
  ident_end = "?!",
  capitalized = "Type",
  call = "Function",
  definers = { "def", "defp", "defmacro", "defmacrop", "defguard" },
  operators = "+-*/=<>!&|^~",
})
