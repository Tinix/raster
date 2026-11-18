//! Motions: where a key moves the cursor, and how an operator treats it.

use crate::buffer::Buffer;

/// A cursor motion. Counts are passed to [`Motion::apply`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    /// `h` / Left.
    Left,
    /// `s` / Right (Dvorak `l`).
    Right,
    /// `n` / Up (Dvorak `k`).
    Up,
    /// `t` / Down (Dvorak `j`).
    Down,
    /// `0`.
    LineStart,
    /// `^`.
    FirstNonBlank,
    /// `$`.
    LineEnd,
    /// `w` / `W`.
    WordForward { big: bool },
    /// `b` / `B`.
    WordBackward { big: bool },
    /// `e` / `E`.
    WordEnd { big: bool },
    /// `gg`, or `{count}gg`.
    FileStart,
    /// `G`, or `{count}G`.
    FileEnd,
    /// `f{char}`.
    FindForward(char),
    /// `F{char}`.
    FindBackward(char),
}

/// How an operator uses the span between the cursor and the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionKind {
    /// The target char is not included (`w`, `b`, `h`, `F`).
    Exclusive,
    /// The target char is included (`e`, `f`).
    Inclusive,
    /// Whole lines (`t`, `n`, `gg`, `G`).
    Linewise,
}

impl Motion {
    pub fn kind(self) -> MotionKind {
        use Motion::*;
        match self {
            Up | Down | FileStart | FileEnd => MotionKind::Linewise,
            WordEnd { .. } | FindForward(_) => MotionKind::Inclusive,
            // `$` lands just past the last char, so exclusive covers the line
            // without touching its newline, even on an empty line.
            Left
            | Right
            | LineStart
            | FirstNonBlank
            | LineEnd
            | WordForward { .. }
            | WordBackward { .. }
            | FindBackward(_) => MotionKind::Exclusive,
        }
    }

    /// Target `(row, col)` from `(row, col)`, or `None` if the motion fails.
    ///
    /// The target column may be one past the last char of the line (for
    /// example after `$` or `s` at the end of a line). Normal mode clamps
    /// it, while operators use it as an exclusive end.
    pub fn apply(
        self,
        buf: &Buffer,
        (row, col): (usize, usize),
        count: Option<usize>,
    ) -> Option<(usize, usize)> {
        let n = count.unwrap_or(1).max(1);
        let last = buf.line_count().saturating_sub(1);
        match self {
            Motion::Left => (col > 0).then(|| (row, col.saturating_sub(n))),
            Motion::Right => {
                let len = buf.line_len(row);
                (col < len).then(|| (row, (col + n).min(len)))
            }
            Motion::Up => (row > 0).then(|| (row.saturating_sub(n), col)),
            Motion::Down => (row < last).then(|| ((row + n).min(last), col)),
            Motion::LineStart => Some((row, 0)),
            Motion::FirstNonBlank => Some((row, buf.first_non_blank(row))),
            Motion::LineEnd => {
                let r = (row + n - 1).min(last);
                Some((r, buf.line_len(r)))
            }
            Motion::FileStart | Motion::FileEnd => {
                let default = if self == Motion::FileStart { 0 } else { last };
                let r = count.map_or(default, |c| c.saturating_sub(1)).min(last);
                Some((r, buf.first_non_blank(r)))
            }
            Motion::WordForward { big } => repeat(buf, row, col, n, |i| word_forward(buf, i, big)),
            Motion::WordBackward { big } => {
                repeat(buf, row, col, n, |i| word_backward(buf, i, big))
            }
            Motion::WordEnd { big } => repeat(buf, row, col, n, |i| word_end(buf, i, big)),
            Motion::FindForward(ch) => {
                let start = buf.pos_to_idx(row, col);
                (start + 1..buf.line_end(row))
                    .filter(|&i| buf.char_at(i) == Some(ch))
                    .nth(n - 1)
                    .map(|i| buf.idx_to_pos(i))
            }
            Motion::FindBackward(ch) => {
                let line = buf.pos_to_idx(row, 0);
                (line..buf.pos_to_idx(row, col))
                    .rev()
                    .filter(|&i| buf.char_at(i) == Some(ch))
                    .nth(n - 1)
                    .map(|i| buf.idx_to_pos(i))
            }
        }
    }
}

/// Apply a char-index step `n` times. Fails if the cursor never moves.
fn repeat(
    buf: &Buffer,
    row: usize,
    col: usize,
    n: usize,
    step: impl Fn(usize) -> usize,
) -> Option<(usize, usize)> {
    let start = buf.pos_to_idx(row, col);
    let mut idx = start;
    for _ in 0..n {
        let next = step(idx);
        if next == idx {
            break;
        }
        idx = next;
    }
    (idx != start).then(|| buf.idx_to_pos(idx))
}

/// Character class for word motions: 0 blank, 1 word, 2 punctuation.
/// With `big` (WORD motions), every non-blank is the same class.
pub(crate) fn class(c: char, big: bool) -> u8 {
    if c.is_whitespace() {
        0
    } else if big || c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

fn ch(buf: &Buffer, i: usize) -> char {
    buf.char_at(i).unwrap_or('\n')
}

/// Whether char index `i` is the start of an empty line. Word motions stop
/// on empty lines, like Vim.
fn is_empty_line(buf: &Buffer, i: usize) -> bool {
    (i >= buf.len_chars() || ch(buf, i) == '\n') && (i == 0 || ch(buf, i - 1) == '\n')
}

fn word_forward(buf: &Buffer, i: usize, big: bool) -> usize {
    let len = buf.len_chars();
    let mut j = i;
    let c = class(ch(buf, j), big);
    if c != 0 {
        while j < len && class(ch(buf, j), big) == c {
            j += 1;
        }
    }
    while j < len && ch(buf, j).is_whitespace() {
        if ch(buf, j) == '\n' && is_empty_line(buf, j + 1) {
            return j + 1;
        }
        j += 1;
    }
    j
}

fn word_backward(buf: &Buffer, i: usize, big: bool) -> usize {
    if i == 0 {
        return 0;
    }
    let mut j = i - 1;
    while ch(buf, j).is_whitespace() {
        if is_empty_line(buf, j) || j == 0 {
            return j;
        }
        j -= 1;
    }
    let c = class(ch(buf, j), big);
    while j > 0 && class(ch(buf, j - 1), big) == c {
        j -= 1;
    }
    j
}

fn word_end(buf: &Buffer, i: usize, big: bool) -> usize {
    let len = buf.len_chars();
    let mut j = i + 1;
    while j < len && ch(buf, j).is_whitespace() {
        j += 1;
    }
    if j >= len {
        return i;
    }
    let c = class(ch(buf, j), big);
    while j + 1 < len && class(ch(buf, j + 1), big) == c {
        j += 1;
    }
    j
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(s: &str) -> Buffer {
        let mut b = Buffer::default();
        b.insert(0, s);
        b
    }

    fn go(s: &str, m: Motion, pos: (usize, usize), count: Option<usize>) -> Option<(usize, usize)> {
        m.apply(&buf(s), pos, count)
    }

    const W: Motion = Motion::WordForward { big: false };
    const BW: Motion = Motion::WordForward { big: true };
    const B: Motion = Motion::WordBackward { big: false };
    const BB: Motion = Motion::WordBackward { big: true };
    const E: Motion = Motion::WordEnd { big: false };
    const BE: Motion = Motion::WordEnd { big: true };

    #[test]
    fn left_and_right() {
        assert_eq!(go("abc", Motion::Left, (0, 2), None), Some((0, 1)));
        assert_eq!(go("abc", Motion::Left, (0, 2), Some(5)), Some((0, 0)));
        assert_eq!(go("abc", Motion::Left, (0, 0), None), None);
        assert_eq!(go("abc", Motion::Right, (0, 0), Some(2)), Some((0, 2)));
        // Past the last char is allowed (operators need it), Normal clamps.
        assert_eq!(go("abc", Motion::Right, (0, 2), None), Some((0, 3)));
        assert_eq!(go("", Motion::Right, (0, 0), None), None);
    }

    #[test]
    fn up_and_down_with_counts() {
        let s = "a\nb\nc\nd";
        assert_eq!(go(s, Motion::Down, (0, 0), None), Some((1, 0)));
        assert_eq!(go(s, Motion::Down, (0, 0), Some(5)), Some((3, 0)));
        assert_eq!(go(s, Motion::Down, (3, 0), None), None);
        assert_eq!(go(s, Motion::Up, (3, 0), Some(2)), Some((1, 0)));
        assert_eq!(go(s, Motion::Up, (0, 0), None), None);
    }

    #[test]
    fn line_start_first_non_blank_and_end() {
        let s = "  foo bar\nxy";
        assert_eq!(go(s, Motion::LineStart, (0, 5), None), Some((0, 0)));
        assert_eq!(go(s, Motion::FirstNonBlank, (0, 7), None), Some((0, 2)));
        assert_eq!(go(s, Motion::LineEnd, (0, 0), None), Some((0, 9)));
        assert_eq!(go(s, Motion::LineEnd, (0, 0), Some(2)), Some((1, 2)));
    }

    #[test]
    fn file_start_and_end() {
        let s = "one\n  two\nthree";
        assert_eq!(go(s, Motion::FileEnd, (0, 0), None), Some((2, 0)));
        assert_eq!(go(s, Motion::FileEnd, (0, 0), Some(2)), Some((1, 2)));
        assert_eq!(go(s, Motion::FileEnd, (0, 0), Some(99)), Some((2, 0)));
        assert_eq!(go(s, Motion::FileStart, (2, 3), None), Some((0, 0)));
        assert_eq!(go(s, Motion::FileStart, (0, 0), Some(2)), Some((1, 2)));
    }

    #[test]
    fn word_forward() {
        let s = "foo.bar baz";
        assert_eq!(go(s, W, (0, 0), None), Some((0, 3)));
        assert_eq!(go(s, W, (0, 3), None), Some((0, 4)));
        assert_eq!(go(s, W, (0, 4), None), Some((0, 8)));
        assert_eq!(go(s, W, (0, 0), Some(3)), Some((0, 8)));
        // Past the last word goes to the end of the buffer.
        assert_eq!(go(s, W, (0, 8), None), Some((0, 11)));
    }

    #[test]
    fn word_forward_crosses_lines_and_stops_on_empty_lines() {
        let s = "ab\n  cd\n\nef";
        assert_eq!(go(s, W, (0, 0), None), Some((1, 2)));
        assert_eq!(go(s, W, (1, 2), None), Some((2, 0)));
        assert_eq!(go(s, W, (2, 0), None), Some((3, 0)));
    }

    #[test]
    fn big_word_forward() {
        let s = "foo.bar baz";
        assert_eq!(go(s, BW, (0, 0), None), Some((0, 8)));
        assert_eq!(go(s, BW, (0, 0), Some(2)), Some((0, 11)));
    }

    #[test]
    fn word_backward() {
        let s = "foo.bar baz";
        assert_eq!(go(s, B, (0, 10), None), Some((0, 8)));
        assert_eq!(go(s, B, (0, 8), None), Some((0, 4)));
        assert_eq!(go(s, B, (0, 4), None), Some((0, 3)));
        assert_eq!(go(s, B, (0, 8), Some(3)), Some((0, 0)));
        assert_eq!(go(s, B, (0, 0), None), None);
    }

    #[test]
    fn word_backward_crosses_lines_and_stops_on_empty_lines() {
        let s = "ab\n\n  cd";
        assert_eq!(go(s, B, (2, 2), None), Some((1, 0)));
        assert_eq!(go(s, B, (1, 0), None), Some((0, 0)));
    }

    #[test]
    fn big_word_backward() {
        let s = "foo.bar baz";
        assert_eq!(go(s, BB, (0, 8), None), Some((0, 0)));
    }

    #[test]
    fn word_end() {
        let s = "foo.bar baz";
        assert_eq!(go(s, E, (0, 0), None), Some((0, 2)));
        assert_eq!(go(s, E, (0, 2), None), Some((0, 3)));
        assert_eq!(go(s, E, (0, 3), None), Some((0, 6)));
        assert_eq!(go(s, E, (0, 0), Some(4)), Some((0, 10)));
        assert_eq!(go(s, E, (0, 10), None), None);
        // `e` skips empty lines.
        assert_eq!(go("ab\n\ncd", E, (0, 1), None), Some((2, 1)));
    }

    #[test]
    fn big_word_end() {
        let s = "foo.bar baz";
        assert_eq!(go(s, BE, (0, 0), None), Some((0, 6)));
        assert_eq!(go(s, BE, (0, 0), Some(2)), Some((0, 10)));
    }

    #[test]
    fn find_forward_and_backward() {
        let s = "a,b,c,d\nx,y";
        assert_eq!(go(s, Motion::FindForward(','), (0, 0), None), Some((0, 1)));
        assert_eq!(
            go(s, Motion::FindForward(','), (0, 0), Some(3)),
            Some((0, 5))
        );
        // Stays on the current line.
        assert_eq!(go(s, Motion::FindForward(','), (0, 5), None), None);
        assert_eq!(go(s, Motion::FindForward('z'), (0, 0), None), None);
        assert_eq!(go(s, Motion::FindBackward(','), (0, 6), None), Some((0, 5)));
        assert_eq!(
            go(s, Motion::FindBackward(','), (0, 6), Some(2)),
            Some((0, 3))
        );
        assert_eq!(go(s, Motion::FindBackward('a'), (0, 0), None), None);
    }

    #[test]
    fn kinds() {
        assert_eq!(W.kind(), MotionKind::Exclusive);
        assert_eq!(E.kind(), MotionKind::Inclusive);
        assert_eq!(Motion::FindForward('x').kind(), MotionKind::Inclusive);
        assert_eq!(Motion::FindBackward('x').kind(), MotionKind::Exclusive);
        assert_eq!(Motion::Down.kind(), MotionKind::Linewise);
        assert_eq!(Motion::FileEnd.kind(), MotionKind::Linewise);
    }
}
