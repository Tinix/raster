//! Text objects: spans selected around the cursor (`iw`, `aw`, ...).

use crate::buffer::Buffer;
use std::ops::Range;

/// A text object, used after an operator (`diw`, `caw`, `yiW`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextObject {
    /// `iw` / `aw` (`big` = `iW` / `aW`). `inner` leaves out the white space.
    Word { inner: bool, big: bool },
}

impl TextObject {
    /// Char range of the object at `(row, col)`, repeated `count` times, or
    /// `None` if there is nothing to select (an empty line).
    pub fn range(
        self,
        buf: &Buffer,
        (row, col): (usize, usize),
        count: Option<usize>,
    ) -> Option<Range<usize>> {
        let TextObject::Word { inner, big } = self;
        let line_start = buf.pos_to_idx(row, 0);
        let line_end = buf.line_end(row);
        if line_start == line_end {
            return None;
        }
        let line: Vec<char> = buf.slice(line_start..line_end).chars().collect();
        let n = count.unwrap_or(1).max(1);

        let class = |i: usize| word_class(line[i], big);
        // End of the run of same-class chars starting at `i`.
        let run_end = |i: usize| {
            let c = class(i);
            (i..line.len())
                .find(|&j| class(j) != c)
                .unwrap_or(line.len())
        };

        let col = col.min(line.len() - 1);
        let mut start = col;
        let c = class(col);
        while start > 0 && class(start - 1) == c {
            start -= 1;
        }

        let mut end = col;
        for _ in 0..n {
            if end >= line.len() {
                break;
            }
            let on_blank = class(end) == 0;
            end = run_end(end);
            // `aw` adds the white space after a word (or the word after a
            // run of white space).
            if !inner && end < line.len() && (class(end) == 0) != on_blank {
                end = run_end(end);
            }
        }

        // `aw` with no trailing white space takes the leading white space.
        if !inner && c != 0 && class(end - 1) != 0 {
            while start > 0 && class(start - 1) == 0 {
                start -= 1;
            }
        }
        Some(line_start + start..line_start + end)
    }
}

/// 0 blank, 1 word, 2 punctuation; WORD treats every non-blank alike.
fn word_class(c: char, big: bool) -> u8 {
    if c.is_whitespace() {
        0
    } else if big || c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IW: TextObject = TextObject::Word {
        inner: true,
        big: false,
    };
    const AW: TextObject = TextObject::Word {
        inner: false,
        big: false,
    };
    const BIG_IW: TextObject = TextObject::Word {
        inner: true,
        big: true,
    };
    const BIG_AW: TextObject = TextObject::Word {
        inner: false,
        big: true,
    };

    fn sel(s: &str, obj: TextObject, pos: (usize, usize), count: Option<usize>) -> Option<String> {
        let mut b = Buffer::default();
        b.insert(0, s);
        obj.range(&b, pos, count).map(|r| b.slice(r))
    }

    #[test]
    fn inner_word() {
        assert_eq!(sel("foo bar baz", IW, (0, 5), None).as_deref(), Some("bar"));
        assert_eq!(sel("foo.bar", IW, (0, 1), None).as_deref(), Some("foo"));
        assert_eq!(sel("foo.bar", IW, (0, 3), None).as_deref(), Some("."));
        // On white space, `iw` selects the white space.
        assert_eq!(sel("foo   bar", IW, (0, 4), None).as_deref(), Some("   "));
    }

    #[test]
    fn inner_word_with_count() {
        assert_eq!(
            sel("foo bar baz", IW, (0, 0), Some(3)).as_deref(),
            Some("foo bar")
        );
    }

    #[test]
    fn a_word_takes_trailing_space() {
        assert_eq!(
            sel("foo bar baz", AW, (0, 5), None).as_deref(),
            Some("bar ")
        );
        assert_eq!(
            sel("foo bar baz", AW, (0, 0), Some(2)).as_deref(),
            Some("foo bar ")
        );
    }

    #[test]
    fn a_word_at_line_end_takes_leading_space() {
        assert_eq!(sel("foo bar", AW, (0, 5), None).as_deref(), Some(" bar"));
    }

    #[test]
    fn a_word_on_space_takes_next_word() {
        assert_eq!(
            sel("foo   bar baz", AW, (0, 4), None).as_deref(),
            Some("   bar")
        );
    }

    #[test]
    fn big_words() {
        assert_eq!(
            sel("a foo.bar b", BIG_IW, (0, 4), None).as_deref(),
            Some("foo.bar")
        );
        assert_eq!(
            sel("a foo.bar b", BIG_AW, (0, 4), None).as_deref(),
            Some("foo.bar ")
        );
    }

    #[test]
    fn stays_on_the_line() {
        assert_eq!(sel("ab\ncd", IW, (1, 0), None).as_deref(), Some("cd"));
        assert_eq!(sel("ab\n\ncd", IW, (1, 0), None), None);
    }
}
