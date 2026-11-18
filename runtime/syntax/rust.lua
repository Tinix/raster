raster.syntax.define("rust", {
  extensions = { "rs" },
  line_comment = { "//" },
  spans = {
    { "/*", "*/", "Comment", multiline = true },
    { '"', '"', "String", escape = "\\", multiline = true },
    { 'b"', '"', "String", escape = "\\", multiline = true },
    { 'r"', '"', "String", multiline = true },
    { 'r#"', '"#', "String", multiline = true },
    -- 'a' and '\n', but not the lifetime 'a.
    { "'", "'", "String", escape = "\\", max_len = 4 },
    { "b'", "'", "String", escape = "\\", max_len = 5 },
    { "#[", "]", "PreProc" },
    { "#![", "]", "PreProc" },
  },
  words = {
    Keyword = {
      "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else",
      "enum", "extern", "fn", "for", "if", "impl", "in", "let", "loop", "match",
      "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static",
      "struct", "super", "trait", "type", "unsafe", "use", "where", "while",
    },
    Type = {
      "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8",
      "i16", "i32", "i64", "i128", "isize", "f32", "f64",
    },
    Constant = { "true", "false", "None", "Some", "Ok", "Err" },
  },
  capitalized = "Type",
  call = "Function",
  definers = { "fn" },
  macro_bang = "PreProc",
  operators = "+-*/%=<>!&|^?",
})
