//! Syntax: a small, configurable lexer for syntax highlighting.
//!
//! No tree-sitter: each language is a [`Syntax`] table (comments, strings,
//! word lists, a few rules), defined in Lua with `raster.syntax.define`. The
//! lexer works one line at a time and carries a [`State`] across lines, so
//! block comments and multi-line strings work. [`Highlighter`] caches the
//! state at the start of each line for the current buffer version.

use crate::buffer::Buffer;
use std::{collections::HashMap, ops::Range, path::Path};

/// A delimited region: a string, a block comment, a Markdown code span...
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub open: String,
    pub close: String,
    pub group: String,
    /// Char that escapes the next one (`\` in most strings).
    pub escape: Option<char>,
    /// May continue on the next lines.
    pub multiline: bool,
    /// Only a span if it closes within this many chars, open and close
    /// included. Tells Rust's `'a'` apart from the lifetime `'a`.
    pub max_len: Option<usize>,
}

/// How to highlight one language.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Syntax {
    pub name: String,
    /// File extensions without the dot: `rs`, `lua`.
    pub extensions: Vec<String>,
    /// Exact file names: `Gemfile`, `Rakefile`.
    pub filenames: Vec<String>,
    /// Comments to the end of the line: `//`, `#`, `--`.
    pub line_comments: Vec<String>,
    pub spans: Vec<Span>,
    /// Word → group (`fn` → `Keyword`, `true` → `Constant`).
    pub words: HashMap<String, String>,
    /// Prefix + identifier → group (`@` → `PreProc` for `@decorator`).
    pub sigils: Vec<(String, String)>,
    /// A line starting (after indentation) with the prefix is all one group
    /// (`#` → `Title` for Markdown headings).
    pub line_rules: Vec<(String, String)>,
    /// Extra chars allowed at the end of identifiers (`?!` for Ruby, Elixir).
    pub ident_end: String,
    /// Group for identifiers starting with an uppercase letter.
    pub capitalized: Option<String>,
    /// Group for an identifier followed by `(`.
    pub call: Option<String>,
    /// Keywords whose next identifier is a function name (`fn`, `def`).
    pub definers: Vec<String>,
    /// Group for `name!` macros (Rust).
    pub macro_bang: Option<String>,
    /// Highlight numbers as `Number`.
    pub numbers: bool,
    /// Chars highlighted as `Operator`.
    pub operators: String,
}

/// Lexer state at a line boundary: the multi-line span still open, if any
/// (an index into [`Syntax::spans`]).
pub type State = Option<usize>;

/// Highlighted regions of a line: char ranges and their group.
pub type Highlights = Vec<(Range<usize>, String)>;

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn starts_with(line: &[char], at: usize, s: &str) -> bool {
    !s.is_empty() && (at..).zip(s.chars()).all(|(i, c)| line.get(i) == Some(&c))
}

impl Syntax {
    /// Whether this syntax handles `path` (by file name or extension).
    pub fn matches(&self, path: &Path) -> bool {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        self.filenames.iter().any(|f| f == name)
            || (!ext.is_empty() && self.extensions.iter().any(|e| e == ext))
    }

    /// Highlight one line, starting in `state`. Returns the highlights and
    /// the state at the end of the line.
    pub fn highlight_line(&self, line: &[char], state: State) -> (Highlights, State) {
        let mut out: Highlights = Vec::new();
        let len = line.len();
        let mut i = 0;

        // Continue a span left open by an earlier line.
        if let Some(idx) = state
            && let Some(span) = self.spans.get(idx)
        {
            let (end, closed) = self.find_close(line, 0, span);
            push(&mut out, 0..end, &span.group);
            if !closed {
                return (out, state);
            }
            i = end;
        }

        if i == 0 {
            let indent = line.iter().take_while(|c| c.is_whitespace()).count();
            if let Some((_, group)) = self
                .line_rules
                .iter()
                .find(|(p, _)| starts_with(line, indent, p))
            {
                push(&mut out, indent..len, group);
                return (out, None);
            }
        }

        let mut after_definer = false;
        while i < len {
            let c = line[i];
            if c.is_whitespace() {
                i += 1;
                continue;
            }
            let prev_ident = i > 0 && is_ident(line[i - 1]);

            if self.line_comments.iter().any(|p| starts_with(line, i, p)) {
                push(&mut out, i..len, "Comment");
                break;
            }

            if let Some((idx, span)) = self.span_at(line, i, prev_ident) {
                let (end, closed) = self.find_close(line, i + span.open.chars().count(), span);
                let too_long = span.max_len.is_some_and(|max| !closed || end - i > max);
                if too_long {
                    // Not this span after all (a Rust lifetime): one plain char.
                    i += 1;
                    continue;
                }
                push(&mut out, i..end, &span.group);
                if !closed && span.multiline {
                    return (out, Some(idx));
                }
                i = end;
                continue;
            }

            if let Some(end) = self.sigil_at(line, i, prev_ident) {
                push(&mut out, i..end.0, end.1);
                i = end.0;
                continue;
            }

            if self.numbers && c.is_ascii_digit() && !prev_ident {
                let end = number_end(line, i);
                push(&mut out, i..end, "Number");
                i = end;
                continue;
            }

            if is_ident(c) {
                let mut end = i;
                while end < len && is_ident(line[end]) {
                    end += 1;
                }
                if end < len && self.ident_end.contains(line[end]) {
                    end += 1;
                }
                let word: String = line[i..end].iter().collect();
                let next = line.get(end).copied();
                let group = if after_definer {
                    Some("Function")
                } else if let Some(g) = self.words.get(&word) {
                    Some(g.as_str())
                } else if self.macro_bang.is_some()
                    && next == Some('!')
                    && line.get(end + 1) != Some(&'=')
                {
                    end += 1;
                    self.macro_bang.as_deref()
                } else if self.call.is_some() && next == Some('(') {
                    self.call.as_deref()
                } else if self.capitalized.is_some() && c.is_uppercase() {
                    self.capitalized.as_deref()
                } else {
                    None
                };
                if let Some(g) = group {
                    push(&mut out, i..end, g);
                }
                after_definer = self.definers.contains(&word);
                i = end;
                continue;
            }

            if self.operators.contains(c) {
                push(&mut out, i..i + 1, "Operator");
            }
            i += 1;
        }
        (out, None)
    }

    /// The span opening at `i`, longest opener first. Openers that start
    /// with a letter (`r"`, `b"`) must not be inside an identifier.
    fn span_at(&self, line: &[char], i: usize, prev_ident: bool) -> Option<(usize, &Span)> {
        self.spans
            .iter()
            .enumerate()
            .filter(|(_, s)| starts_with(line, i, &s.open))
            .filter(|(_, s)| !(prev_ident && s.open.starts_with(is_ident)))
            .max_by_key(|(_, s)| s.open.chars().count())
    }

    /// End of a sigil + identifier at `i` (`@name`, `:atom`), and its group.
    fn sigil_at(&self, line: &[char], i: usize, prev_ident: bool) -> Option<(usize, &str)> {
        if prev_ident {
            return None;
        }
        self.sigils.iter().find_map(|(prefix, group)| {
            let n = prefix.chars().count();
            // `::` is a path separator, not a `:atom`.
            let doubled = prefix.chars().count() == 1
                && (line.get(i + 1) == Some(&line[i]) || (i > 0 && line[i - 1] == line[i]));
            if !starts_with(line, i, prefix)
                || doubled
                || !line.get(i + n).is_some_and(|c| is_ident(*c))
            {
                return None;
            }
            let mut end = i + n;
            while end < line.len() && is_ident(line[end]) {
                end += 1;
            }
            if end < line.len() && self.ident_end.contains(line[end]) {
                end += 1;
            }
            Some((end, group.as_str()))
        })
    }

    /// Where `span` closes on `line`, searching from `from`: (end, closed).
    fn find_close(&self, line: &[char], from: usize, span: &Span) -> (usize, bool) {
        let mut k = from;
        while k < line.len() {
            if Some(line[k]) == span.escape {
                k += 2;
                continue;
            }
            if starts_with(line, k, &span.close) {
                return (k + span.close.chars().count(), true);
            }
            k += 1;
        }
        (line.len(), false)
    }
}

fn push(out: &mut Highlights, range: Range<usize>, group: &str) {
    if !range.is_empty() {
        out.push((range, group.to_owned()));
    }
}

/// End of a number at `i`: digits, letters (`0x1f`, `10u32`, `1e5`), `_`,
/// and one `.` followed by a digit (so `1..10` stays a range).
fn number_end(line: &[char], i: usize) -> usize {
    let mut end = i;
    let mut dot = false;
    while end < line.len() {
        let c = line[end];
        if is_ident(c) {
            end += 1;
        } else if c == '.' && !dot && line.get(end + 1).is_some_and(|d| d.is_ascii_digit()) {
            dot = true;
            end += 1;
        } else {
            break;
        }
    }
    end
}

/// All known syntaxes, by name.
#[derive(Debug, Clone, Default)]
pub struct SyntaxSet {
    syntaxes: Vec<Syntax>,
}

impl SyntaxSet {
    /// Add a syntax, replacing one with the same name.
    pub fn define(&mut self, syntax: Syntax) {
        self.syntaxes.retain(|s| s.name != syntax.name);
        self.syntaxes.push(syntax);
    }

    pub fn get(&self, name: &str) -> Option<&Syntax> {
        self.syntaxes.iter().find(|s| s.name == name)
    }

    /// The syntax for a file, by name or extension. Later definitions win.
    pub fn detect(&self, path: &Path) -> Option<&Syntax> {
        self.syntaxes.iter().rev().find(|s| s.matches(path))
    }

    pub fn names(&self) -> Vec<&str> {
        self.syntaxes.iter().map(|s| s.name.as_str()).collect()
    }
}

/// Caches the lexer state at the start of each line.
#[derive(Debug, Default)]
pub struct Highlighter {
    key: Option<(u64, String)>,
    /// `states[r]` is the state at the start of line `r`.
    states: Vec<State>,
}

impl Highlighter {
    /// Highlights of line `row`. Lines above it are lexed once per buffer
    /// version; after an edit the cache starts over.
    pub fn line(&mut self, syntax: &Syntax, buf: &Buffer, row: usize) -> Highlights {
        let key = (buf.version(), syntax.name.clone());
        if self.key.as_ref() != Some(&key) {
            self.key = Some(key);
            self.states = vec![None];
        }
        while self.states.len() <= row {
            let r = self.states.len() - 1;
            let (_, end) = syntax.highlight_line(&line_chars(buf, r), self.states[r]);
            self.states.push(end);
        }
        syntax
            .highlight_line(&line_chars(buf, row), self.states[row])
            .0
    }
}

fn line_chars(buf: &Buffer, row: usize) -> Vec<char> {
    let start = buf.pos_to_idx(row, 0);
    buf.text.slice(start..buf.line_end(row)).chars().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(open: &str, close: &str, group: &str) -> Span {
        Span {
            open: open.into(),
            close: close.into(),
            group: group.into(),
            escape: None,
            multiline: false,
            max_len: None,
        }
    }

    /// A Rust-like syntax for the lexer tests.
    fn rusty() -> Syntax {
        let mut words = HashMap::new();
        for w in ["fn", "let", "if"] {
            words.insert(w.into(), "Keyword".into());
        }
        words.insert("true".into(), "Constant".into());
        Syntax {
            name: "rusty".into(),
            extensions: vec!["rs".into()],
            filenames: vec!["Rustfile".into()],
            line_comments: vec!["//".into()],
            spans: vec![
                Span {
                    multiline: true,
                    ..span("/*", "*/", "Comment")
                },
                Span {
                    escape: Some('\\'),
                    multiline: true,
                    ..span("\"", "\"", "String")
                },
                Span {
                    escape: Some('\\'),
                    max_len: Some(4),
                    ..span("'", "'", "String")
                },
                span("r#\"", "\"#", "String"),
                span("#[", "]", "PreProc"),
            ],
            words,
            sigils: vec![("@".into(), "PreProc".into())],
            capitalized: Some("Type".into()),
            call: Some("Function".into()),
            definers: vec!["fn".into()],
            macro_bang: Some("PreProc".into()),
            numbers: true,
            operators: "=+".into(),
            ..Syntax::default()
        }
    }

    /// Highlight `line` and render it as `text` with `[Group:text]` marks.
    fn marks(syntax: &Syntax, line: &str, state: State) -> (String, State) {
        let chars: Vec<char> = line.chars().collect();
        let (hl, end) = syntax.highlight_line(&chars, state);
        let mut out = String::new();
        let mut i = 0;
        for (r, g) in hl {
            out.extend(&chars[i..r.start]);
            out.push_str(&format!(
                "[{g}:{}]",
                chars[r.clone()].iter().collect::<String>()
            ));
            i = r.end;
        }
        out.extend(&chars[i..]);
        (out, end)
    }

    fn hl(syntax: &Syntax, line: &str) -> String {
        marks(syntax, line, None).0
    }

    #[test]
    fn keywords_functions_types_and_numbers() {
        let s = rusty();
        assert_eq!(
            hl(&s, "fn main() { let x = Foo::new(42); }"),
            "[Keyword:fn] [Function:main]() { [Keyword:let] x [Operator:=] [Type:Foo]::[Function:new]([Number:42]); }"
        );
        assert_eq!(hl(&s, "if true"), "[Keyword:if] [Constant:true]");
    }

    #[test]
    fn numbers() {
        let s = rusty();
        assert_eq!(
            hl(&s, "0x1f 1_000 1.5e3 10u32"),
            "[Number:0x1f] [Number:1_000] [Number:1.5e3] [Number:10u32]"
        );
        assert_eq!(hl(&s, "1..10"), "[Number:1]..[Number:10]");
        assert_eq!(hl(&s, "x2"), "x2");
    }

    #[test]
    fn comments() {
        let s = rusty();
        assert_eq!(
            hl(&s, "let x // fn \"no\""),
            "[Keyword:let] x [Comment:// fn \"no\"]"
        );
        assert_eq!(hl(&s, "a /* b */ c"), "a [Comment:/* b */] c");
    }

    #[test]
    fn strings_and_escapes() {
        let s = rusty();
        assert_eq!(
            hl(&s, r#"x = "a \" fn" + 1"#),
            r#"x [Operator:=] [String:"a \" fn"] [Operator:+] [Number:1]"#
        );
        assert_eq!(hl(&s, r##"r#"raw "x""#"##), r##"[String:r#"raw "x""#]"##);
        // `r#"` inside an identifier is not a raw string.
        assert_eq!(hl(&s, "for"), "for");
    }

    #[test]
    fn char_literals_versus_lifetimes() {
        let s = rusty();
        assert_eq!(hl(&s, "'a' '\\n'"), "[String:'a'] [String:'\\n']");
        assert_eq!(
            hl(&s, "fn f<'a>(x: &'a str)"),
            "[Keyword:fn] [Function:f]<'a>(x: &'a str)"
        );
    }

    #[test]
    fn multiline_spans_carry_state() {
        let s = rusty();
        let (first, state) = marks(&s, "x /* start", None);
        assert_eq!(first, "x [Comment:/* start]");
        assert_eq!(state, Some(0));
        let (mid, state) = marks(&s, "fn middle", state);
        assert_eq!((mid.as_str(), state), ("[Comment:fn middle]", Some(0)));
        let (last, state) = marks(&s, "end */ fn", state);
        assert_eq!(
            (last.as_str(), state),
            ("[Comment:end */] [Keyword:fn]", None)
        );

        let (_, state) = marks(&s, "let s = \"open", None);
        assert_eq!(state, Some(1));
    }

    #[test]
    fn single_line_spans_end_with_the_line() {
        let s = rusty();
        let (text, state) = marks(&s, "#[derive(Debug", None);
        assert_eq!((text.as_str(), state), ("[PreProc:#[derive(Debug]", None));
    }

    #[test]
    fn attributes_macros_and_sigils() {
        let s = rusty();
        assert_eq!(hl(&s, "#[test]"), "[PreProc:#[test]]");
        assert_eq!(
            hl(&s, "println!(x) a != b"),
            "[PreProc:println!](x) a ![Operator:=] b"
        );
        assert_eq!(hl(&s, "@decorator x@y"), "[PreProc:@decorator] x@y");
    }

    #[test]
    fn sigils_ignore_double_colons() {
        let s = Syntax {
            sigils: vec![(":".into(), "Constant".into())],
            ident_end: "?".into(),
            ..Syntax::default()
        };
        assert_eq!(
            hl(&s, ":ok Foo::Bar :empty?"),
            "[Constant::ok] Foo::Bar [Constant::empty?]"
        );
    }

    #[test]
    fn ident_end_chars() {
        let mut s = Syntax {
            ident_end: "?!".into(),
            ..Syntax::default()
        };
        s.words.insert("empty?".into(), "Function".into());
        assert_eq!(hl(&s, "x.empty? y"), "x.[Function:empty?] y");
    }

    #[test]
    fn line_rules() {
        let s = Syntax {
            line_rules: vec![("#".into(), "Title".into())],
            spans: vec![span("`", "`", "String")],
            ..Syntax::default()
        };
        assert_eq!(hl(&s, "  # Title `x`"), "  [Title:# Title `x`]");
        assert_eq!(hl(&s, "a `code` b"), "a [String:`code`] b");
    }

    #[test]
    fn detection() {
        let mut set = SyntaxSet::default();
        set.define(rusty());
        assert_eq!(
            set.detect(Path::new("src/main.rs"))
                .map(|s| s.name.as_str()),
            Some("rusty")
        );
        assert_eq!(
            set.detect(Path::new("Rustfile")).map(|s| s.name.as_str()),
            Some("rusty")
        );
        assert!(set.detect(Path::new("notes.txt")).is_none());
        assert!(set.detect(Path::new("rs")).is_none());
        // Redefining replaces.
        set.define(Syntax {
            name: "rusty".into(),
            ..Syntax::default()
        });
        assert_eq!(set.names(), vec!["rusty"]);
        assert!(set.detect(Path::new("a.rs")).is_none());
    }

    #[test]
    fn highlighter_tracks_state_across_lines_and_edits() {
        let s = rusty();
        let mut b = Buffer::default();
        b.insert(0, "/* a\nb */ fn\nfn");
        let mut h = Highlighter::default();
        assert_eq!(
            h.line(&s, &b, 1),
            vec![(0..4, "Comment".to_string()), (5..7, "Keyword".to_string())]
        );
        assert_eq!(h.line(&s, &b, 2), vec![(0..2, "Keyword".to_string())]);
        // Closing the comment on line 0 changes the lines below.
        b.insert(4, " */");
        assert_eq!(h.line(&s, &b, 1), vec![(5..7, "Keyword".to_string())]);
    }
}
