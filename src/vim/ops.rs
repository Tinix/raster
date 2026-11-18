//! Operators: what `d`, `c` and `y` do to a span of text, plus put (`p`/`P`).

use super::motion::{self, Motion, MotionKind};
use super::textobj::TextObject;
use crate::buffer::Buffer;
use std::ops::Range;

/// An operator waiting for a motion or text object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Change,
    Yank,
}

impl Operator {
    /// Key that starts the operator (and repeats it for the linewise form, `dd`).
    pub fn key(self) -> char {
        match self {
            Operator::Delete => 'd',
            Operator::Change => 'c',
            Operator::Yank => 'y',
        }
    }

    /// Whether the operator modifies the buffer (and so is undoable and
    /// repeatable with `.`).
    pub fn is_change(self) -> bool {
        self != Operator::Yank
    }
}

/// The unnamed register: the text of the last delete, change or yank.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Register {
    pub text: String,
    /// Whole lines (`dd`, `yy`): `text` ends with a newline and puts go on
    /// their own lines.
    pub linewise: bool,
}

/// Text an operator acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Span {
    /// A char range.
    Chars(Range<usize>),
    /// Whole lines, `first..=last`.
    Lines(usize, usize),
}

/// Span covered by `op` with `motion` from the cursor, or `None` if the
/// motion fails.
pub fn motion_span(
    buf: &Buffer,
    op: Operator,
    cur: (usize, usize),
    motion: Motion,
    count: Option<usize>,
) -> Option<Span> {
    let start = buf.pos_to_idx(cur.0, cur.1);

    // `cw` on a word changes to the end of the word, like `ce`, but starts
    // counting at the current word.
    if let (Operator::Change, Motion::WordForward { big }) = (op, motion)
        && buf.char_at(start).is_some_and(|c| !c.is_whitespace())
    {
        let c = motion::class(buf.char_at(start)?, big);
        let mut end = start;
        while buf
            .char_at(end + 1)
            .is_some_and(|ch| motion::class(ch, big) == c)
        {
            end += 1;
        }
        if let Some(n) = count.filter(|&n| n > 1) {
            let (r, c) = Motion::WordEnd { big }
                .apply(buf, buf.idx_to_pos(end), Some(n - 1))
                .unwrap_or(buf.idx_to_pos(end));
            end = buf.pos_to_idx(r, c);
        }
        return Some(Span::Chars(start..end + 1));
    }

    let target = motion.apply(buf, cur, count)?;
    let end = buf.pos_to_idx(target.0, target.1);
    let (lo, hi) = (start.min(end), start.max(end));
    Some(match motion.kind() {
        MotionKind::Linewise => Span::Lines(cur.0.min(target.0), cur.0.max(target.0)),
        MotionKind::Inclusive => Span::Chars(lo..(hi + 1).min(buf.len_chars())),
        MotionKind::Exclusive => {
            let mut hi = hi;
            // An exclusive motion that ends at the start of a later line
            // stops at the end of the line before it (`dw` on the last word
            // of a line keeps the line break). For `w`, Vim also ignores the
            // indent of the next line.
            let to_later_line = target.0 > cur.0;
            let is_word = matches!(motion, Motion::WordForward { .. });
            if to_later_line && (is_word || target.1 == 0) {
                hi = buf.line_end(target.0 - 1).max(lo);
            }
            Span::Chars(lo..hi)
        }
    })
}

/// Span covered by a text object at the cursor.
pub fn object_span(
    buf: &Buffer,
    cur: (usize, usize),
    obj: TextObject,
    count: Option<usize>,
) -> Option<Span> {
    obj.range(buf, cur, count).map(Span::Chars)
}

/// Span of `count` whole lines from the cursor (`dd`, `cc`, `yy`).
pub fn lines_span(buf: &Buffer, cur: (usize, usize), count: Option<usize>) -> Span {
    let n = count.unwrap_or(1).max(1);
    let last = buf.line_count().saturating_sub(1);
    Span::Lines(cur.0, (cur.0 + n - 1).min(last))
}

/// Apply `op` to `span`, filling `reg`. Returns the new cursor. After
/// [`Operator::Change`] the caller enters Insert mode at that cursor.
pub fn apply(
    buf: &mut Buffer,
    reg: &mut Register,
    op: Operator,
    span: Span,
    cur: (usize, usize),
) -> (usize, usize) {
    match span {
        Span::Chars(range) => {
            let start = range.start;
            *reg = Register {
                text: buf.slice(range.clone()),
                linewise: false,
            };
            if op.is_change() {
                buf.remove(range);
            }
            buf.idx_to_pos(start)
        }
        Span::Lines(first, last) => {
            let text_start = buf.pos_to_idx(first, 0);
            *reg = Register {
                text: buf.slice(text_start..buf.line_end(last)) + "\n",
                linewise: true,
            };
            match op {
                Operator::Yank => (first, cur.1),
                Operator::Delete => {
                    let range = if last + 1 < buf.line_count() {
                        text_start..buf.pos_to_idx(last + 1, 0)
                    } else if first > 0 {
                        // Last lines of the buffer: take the newline before them.
                        buf.line_end(first - 1)..buf.len_chars()
                    } else {
                        0..buf.len_chars()
                    };
                    buf.remove(range);
                    let row = first.min(buf.line_count().saturating_sub(1));
                    (row, buf.first_non_blank(row))
                }
                Operator::Change => {
                    // Keep the indent of the first line, like nvim's autoindent.
                    let indent = buf.first_non_blank(first);
                    buf.remove(text_start + indent..buf.line_end(last));
                    (first, indent)
                }
            }
        }
    }
}

/// Put the register `count` times after the cursor (`p`) or before it
/// (`P`). Returns the new cursor.
pub fn put(
    buf: &mut Buffer,
    reg: &Register,
    cur: (usize, usize),
    before: bool,
    count: Option<usize>,
) -> (usize, usize) {
    if reg.text.is_empty() {
        return cur;
    }
    let text = reg.text.repeat(count.unwrap_or(1).max(1));
    if reg.linewise {
        let row = if before { cur.0 } else { cur.0 + 1 };
        if row < buf.line_count() {
            buf.insert(buf.pos_to_idx(row, 0), &text);
        } else {
            let body = text.strip_suffix('\n').unwrap_or(&text);
            buf.insert(buf.len_chars(), &format!("\n{body}"));
        }
        (row, buf.first_non_blank(row))
    } else {
        let after = !before && buf.line_len(cur.0) > 0;
        let idx = buf.pos_to_idx(cur.0, cur.1) + usize::from(after);
        buf.insert(idx, &text);
        buf.idx_to_pos(idx + text.chars().count() - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: Motion = Motion::WordForward { big: false };
    const IW: TextObject = TextObject::Word {
        inner: true,
        big: false,
    };
    const AW: TextObject = TextObject::Word {
        inner: false,
        big: false,
    };

    fn buf(s: &str) -> Buffer {
        let mut b = Buffer::default();
        b.insert(0, s);
        b
    }

    /// Run `op` + `motion` on `s` at `cur`: (text, register, cursor).
    fn op_motion(
        s: &str,
        op: Operator,
        cur: (usize, usize),
        motion: Motion,
        count: Option<usize>,
    ) -> (String, String, (usize, usize)) {
        let mut b = buf(s);
        let mut reg = Register::default();
        let span = motion_span(&b, op, cur, motion, count).expect("motion fails");
        let cur = apply(&mut b, &mut reg, op, span, cur);
        (b.text.to_string(), reg.text, cur)
    }

    fn op_lines(
        s: &str,
        op: Operator,
        cur: (usize, usize),
        count: Option<usize>,
    ) -> (String, Register, (usize, usize)) {
        let mut b = buf(s);
        let mut reg = Register::default();
        let span = lines_span(&b, cur, count);
        let cur = apply(&mut b, &mut reg, op, span, cur);
        (b.text.to_string(), reg, cur)
    }

    fn op_object(s: &str, op: Operator, cur: (usize, usize), obj: TextObject) -> (String, String) {
        let mut b = buf(s);
        let mut reg = Register::default();
        let span = object_span(&b, cur, obj, None).unwrap();
        apply(&mut b, &mut reg, op, span, cur);
        (b.text.to_string(), reg.text)
    }

    #[test]
    fn dw_deletes_word_and_space() {
        let (t, r, c) = op_motion("foo bar baz", Operator::Delete, (0, 4), W, None);
        assert_eq!((t.as_str(), r.as_str(), c), ("foo baz", "bar ", (0, 4)));
    }

    #[test]
    fn dw_with_count_and_on_last_word() {
        let (t, ..) = op_motion("foo bar baz", Operator::Delete, (0, 0), W, Some(2));
        assert_eq!(t, "baz");
        let (t, ..) = op_motion("foo bar", Operator::Delete, (0, 4), W, None);
        assert_eq!(t, "foo ");
    }

    #[test]
    fn dw_keeps_the_line_break() {
        let (t, r, _) = op_motion("foo\n  bar", Operator::Delete, (0, 0), W, None);
        assert_eq!((t.as_str(), r.as_str()), ("\n  bar", "foo"));
    }

    #[test]
    fn de_is_inclusive() {
        let e = Motion::WordEnd { big: false };
        let (t, r, _) = op_motion("foo bar", Operator::Delete, (0, 0), e, None);
        assert_eq!((t.as_str(), r.as_str()), (" bar", "foo"));
    }

    #[test]
    fn db_deletes_backwards() {
        let b = Motion::WordBackward { big: false };
        let (t, r, c) = op_motion("foo bar", Operator::Delete, (0, 4), b, None);
        assert_eq!((t.as_str(), r.as_str(), c), ("bar", "foo ", (0, 0)));
    }

    #[test]
    fn d_dollar_and_count() {
        let (t, r, _) = op_motion(
            "foo bar\nbaz",
            Operator::Delete,
            (0, 4),
            Motion::LineEnd,
            None,
        );
        assert_eq!((t.as_str(), r.as_str()), ("foo \nbaz", "bar"));
        let (t, ..) = op_motion(
            "ab\ncd\nef",
            Operator::Delete,
            (0, 1),
            Motion::LineEnd,
            Some(2),
        );
        assert_eq!(t, "a\nef");
        // On an empty line there is nothing to delete.
        let (t, ..) = op_motion("\nx", Operator::Delete, (0, 0), Motion::LineEnd, None);
        assert_eq!(t, "\nx");
    }

    #[test]
    fn x_is_delete_right() {
        let (t, r, c) = op_motion("abc", Operator::Delete, (0, 1), Motion::Right, None);
        assert_eq!((t.as_str(), r.as_str(), c), ("ac", "b", (0, 1)));
        let (t, ..) = op_motion("abc", Operator::Delete, (0, 1), Motion::Right, Some(9));
        assert_eq!(t, "a");
        assert!(motion_span(&buf(""), Operator::Delete, (0, 0), Motion::Right, None).is_none());
    }

    #[test]
    fn find_with_operator() {
        let (t, ..) = op_motion(
            "a,b,c",
            Operator::Delete,
            (0, 0),
            Motion::FindForward(','),
            Some(2),
        );
        assert_eq!(t, "c");
        let (t, ..) = op_motion(
            "a,b,c",
            Operator::Delete,
            (0, 4),
            Motion::FindBackward(','),
            None,
        );
        assert_eq!(t, "a,bc");
    }

    #[test]
    fn linewise_motions_with_operator() {
        let s = "a\nb\nc\nd";
        let (t, ..) = op_motion(s, Operator::Delete, (1, 0), Motion::Down, None);
        assert_eq!(t, "a\nd");
        let (t, ..) = op_motion(s, Operator::Delete, (1, 0), Motion::FileEnd, None);
        assert_eq!(t, "a");
        let (t, ..) = op_motion(s, Operator::Delete, (2, 0), Motion::FileStart, None);
        assert_eq!(t, "d");
    }

    #[test]
    fn cw_acts_like_ce() {
        let (t, r, c) = op_motion("foo bar", Operator::Change, (0, 0), W, None);
        assert_eq!((t.as_str(), r.as_str(), c), (" bar", "foo", (0, 0)));
        // On the last char of a word, only that char.
        let (t, ..) = op_motion("foo bar", Operator::Change, (0, 2), W, None);
        assert_eq!(t, "fo bar");
        let (t, ..) = op_motion("a b c d", Operator::Change, (0, 0), W, Some(3));
        assert_eq!(t, " d");
        // On white space it behaves like `dw`.
        let (t, ..) = op_motion("a   b", Operator::Change, (0, 1), W, None);
        assert_eq!(t, "ab");
    }

    #[test]
    fn yank_keeps_text() {
        let (t, r, c) = op_motion(
            "foo bar",
            Operator::Yank,
            (0, 4),
            Motion::WordBackward { big: false },
            None,
        );
        assert_eq!((t.as_str(), r.as_str(), c), ("foo bar", "foo ", (0, 0)));
    }

    #[test]
    fn text_objects_with_operators() {
        assert_eq!(
            op_object("foo bar baz", Operator::Delete, (0, 5), IW),
            ("foo  baz".into(), "bar".into())
        );
        assert_eq!(
            op_object("foo bar baz", Operator::Delete, (0, 5), AW),
            ("foo baz".into(), "bar ".into())
        );
        assert_eq!(
            op_object("foo bar", Operator::Change, (0, 1), IW),
            (" bar".into(), "foo".into())
        );
        assert_eq!(
            op_object("foo bar", Operator::Yank, (0, 1), AW),
            ("foo bar".into(), "foo ".into())
        );
    }

    #[test]
    fn dd_deletes_lines() {
        let (t, r, c) = op_lines("a\n  b\nc", Operator::Delete, (0, 0), None);
        assert_eq!((t.as_str(), c), ("  b\nc", (0, 2)));
        assert_eq!(
            r,
            Register {
                text: "a\n".into(),
                linewise: true
            }
        );

        let (t, r, _) = op_lines("a\nb\nc\nd", Operator::Delete, (1, 0), Some(2));
        assert_eq!((t.as_str(), r.text.as_str()), ("a\nd", "b\nc\n"));
    }

    #[test]
    fn dd_on_last_lines_and_only_line() {
        let (t, _, c) = op_lines("a\nb\nc", Operator::Delete, (2, 0), None);
        assert_eq!((t.as_str(), c), ("a\nb", (1, 0)));
        let (t, ..) = op_lines("a\nb\nc", Operator::Delete, (1, 0), Some(9));
        assert_eq!(t, "a");
        let (t, r, c) = op_lines("only", Operator::Delete, (0, 2), None);
        assert_eq!((t.as_str(), r.text.as_str(), c), ("", "only\n", (0, 0)));
    }

    #[test]
    fn cc_keeps_indent() {
        let (t, r, c) = op_lines("a\n    foo\nb", Operator::Change, (1, 6), None);
        assert_eq!((t.as_str(), c), ("a\n    \nb", (1, 4)));
        assert_eq!(r.text, "    foo\n");
        let (t, ..) = op_lines("x\ny\nz", Operator::Change, (0, 0), Some(2));
        assert_eq!(t, "\nz");
    }

    #[test]
    fn yy_keeps_text() {
        let (t, r, c) = op_lines("a\nb", Operator::Yank, (0, 0), Some(2));
        assert_eq!((t.as_str(), r.text.as_str(), c), ("a\nb", "a\nb\n", (0, 0)));
    }

    fn put_on(
        s: &str,
        reg: Register,
        cur: (usize, usize),
        before: bool,
        count: Option<usize>,
    ) -> (String, (usize, usize)) {
        let mut b = buf(s);
        let cur = put(&mut b, &reg, cur, before, count);
        (b.text.to_string(), cur)
    }

    fn chars(s: &str) -> Register {
        Register {
            text: s.into(),
            linewise: false,
        }
    }

    fn lines(s: &str) -> Register {
        Register {
            text: s.into(),
            linewise: true,
        }
    }

    #[test]
    fn put_chars() {
        assert_eq!(
            put_on("ac", chars("b"), (0, 0), false, None),
            ("abc".into(), (0, 1))
        );
        assert_eq!(
            put_on("ac", chars("b"), (0, 1), true, None),
            ("abc".into(), (0, 1))
        );
        assert_eq!(
            put_on("a", chars("xy"), (0, 0), false, Some(2)),
            ("axyxy".into(), (0, 4))
        );
        assert_eq!(
            put_on("", chars("x"), (0, 0), false, None),
            ("x".into(), (0, 0))
        );
    }

    #[test]
    fn put_lines() {
        assert_eq!(
            put_on("a\nc", lines("b\n"), (0, 0), false, None),
            ("a\nb\nc".into(), (1, 0))
        );
        assert_eq!(
            put_on("a\nc", lines("b\n"), (1, 0), true, None),
            ("a\nb\nc".into(), (1, 0))
        );
        // After the last line.
        assert_eq!(
            put_on("a", lines("  b\n"), (0, 0), false, Some(2)),
            ("a\n  b\n  b".into(), (1, 2))
        );
    }

    #[test]
    fn put_empty_register_does_nothing() {
        assert_eq!(
            put_on("a", Register::default(), (0, 0), false, None),
            ("a".into(), (0, 0))
        );
    }
}
