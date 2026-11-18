//! Parser: the Normal-mode state machine that turns keys into commands.
//!
//! Grammar (the same as Vim's):
//!
//! ```text
//! command  = [count] ( motion | simple | operator [count] target )
//! target   = motion | "i" object | "a" object | <operator key again>
//! motion   = h t n s 0 ^ $ w b e W B E G | "gg" | ("f" | "F") char
//! ```
//!
//! Feed keys one at a time with [`Parser::feed`]. It answers
//! [`Step::Pending`] while a command is incomplete, then [`Step::Done`] with
//! the parsed [`Cmd`], or [`Step::Cancel`] for an invalid key. Both reset the
//! parser. New motions, operators and text objects only need a new arm in
//! the tables below.

use super::{motion::Motion, ops::Operator, textobj::TextObject};
use crate::command::Command;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// What an operator acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Motion(Motion),
    Object(TextObject),
    /// The operator key typed twice: `dd`, `cc`, `yy`.
    Lines,
}

/// Where an Insert session starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertAt {
    /// `i`.
    Cursor,
    /// `a`.
    After,
    /// `I`: first non-blank.
    LineStart,
    /// `A`.
    LineEnd,
    /// `o`.
    LineBelow,
    /// `O`.
    LineAbove,
}

/// A complete Normal-mode command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    Move {
        motion: Motion,
        count: Option<usize>,
    },
    /// `d`/`c`/`y` + target, and the shortcuts `x` (`dl`), `D` (`d$`), `C` (`c$`).
    Operate {
        op: Operator,
        count: Option<usize>,
        target: Target,
    },
    Insert(InsertAt),
    /// `p` (after) / `P` (before).
    Put {
        before: bool,
        count: Option<usize>,
    },
    Undo {
        count: Option<usize>,
    },
    Redo {
        count: Option<usize>,
    },
    /// `.`
    Repeat {
        count: Option<usize>,
    },
    /// `:`
    CommandLine,
    /// An ex-command bound to a key (`<C-s>`, `<leader>q`).
    Ex(Command),
}

impl Cmd {
    /// Whether the command changes the buffer, so `.` can repeat it.
    pub fn is_change(&self) -> bool {
        match self {
            Cmd::Operate { op, .. } => op.is_change(),
            Cmd::Insert(_) | Cmd::Put { .. } => true,
            _ => false,
        }
    }

    /// The same command with a new count (`3.` repeats with count 3).
    pub fn with_count(self, new: Option<usize>) -> Self {
        match self {
            Cmd::Operate { op, target, .. } => Cmd::Operate {
                op,
                target,
                count: new,
            },
            Cmd::Put { before, .. } => Cmd::Put { before, count: new },
            other => other,
        }
    }
}

/// Result of feeding one key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    Pending,
    Done(Cmd),
    Cancel,
}

/// A key that needs another key to finish.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Pending {
    #[default]
    None,
    /// After `g`.
    G,
    /// After `f` (forward) or `F`.
    Find { forward: bool },
    /// After `i` or `a` in operator-pending mode.
    Object { inner: bool },
    /// After the leader key.
    Leader,
}

/// Leader key, from the nvim config (`vim.g.mapleader = " "`).
pub const LEADER: char = ' ';

#[derive(Debug, Default)]
pub struct Parser {
    /// Count before the operator (the `2` in `2d3w`).
    count: Option<usize>,
    op: Option<Operator>,
    /// Count after the operator (the `3` in `2d3w`).
    op_count: Option<usize>,
    pending: Pending,
}

impl Parser {
    /// Whether a command is half typed (for the status bar later on).
    pub fn is_pending(&self) -> bool {
        self.count.is_some() || self.op.is_some() || self.pending != Pending::None
    }

    pub fn feed(&mut self, key: KeyEvent) -> Step {
        let step = self.step(key);
        if step != Step::Pending {
            *self = Self::default();
        }
        step
    }

    fn step(&mut self, key: KeyEvent) -> Step {
        if key.code == KeyCode::Esc {
            return Step::Cancel;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let ch = match key.code {
            KeyCode::Char(c) => Some(c),
            _ => None,
        };

        match std::mem::take(&mut self.pending) {
            Pending::None => {}
            Pending::G => {
                return match ch {
                    Some('g') => self.motion(Motion::FileStart),
                    _ => Step::Cancel,
                };
            }
            Pending::Find { forward } => {
                return match ch {
                    Some(c) if forward => self.motion(Motion::FindForward(c)),
                    Some(c) => self.motion(Motion::FindBackward(c)),
                    None => Step::Cancel,
                };
            }
            Pending::Object { inner } => {
                return match ch.and_then(|c| object_key(c, inner)) {
                    Some(obj) => self.operate(Target::Object(obj)),
                    None => Step::Cancel,
                };
            }
            Pending::Leader => {
                return match ch {
                    Some('q') => Step::Done(Cmd::Ex(Command::Quit)),
                    Some('Q') => Step::Done(Cmd::Ex(Command::ForceQuit)),
                    _ => Step::Cancel,
                };
            }
        }

        if ctrl {
            return match ch {
                Some('s') => Step::Done(Cmd::Ex(Command::Write)),
                Some('r') if self.op.is_none() => Step::Done(Cmd::Redo { count: self.count }),
                _ => Step::Cancel,
            };
        }

        // Counts. `0` is a motion unless a count is being typed.
        if let Some(d) = ch.and_then(|c| c.to_digit(10)) {
            let slot = if self.op.is_some() {
                &mut self.op_count
            } else {
                &mut self.count
            };
            if d != 0 || slot.is_some() {
                *slot = Some(
                    slot.unwrap_or(0)
                        .saturating_mul(10)
                        .saturating_add(d as usize),
                );
                return Step::Pending;
            }
        }

        if let Some(m) = motion_key(key.code) {
            return self.motion(m);
        }
        let Some(c) = ch else {
            return Step::Cancel;
        };
        match c {
            'g' => return self.wait(Pending::G),
            'f' => return self.wait(Pending::Find { forward: true }),
            'F' => return self.wait(Pending::Find { forward: false }),
            _ => {}
        }

        if let Some(op) = self.op {
            return match c {
                'i' => self.wait(Pending::Object { inner: true }),
                'a' => self.wait(Pending::Object { inner: false }),
                c if c == op.key() => self.operate(Target::Lines),
                _ => Step::Cancel,
            };
        }

        let count = self.count;
        let done = |cmd| Step::Done(cmd);
        match c {
            'd' | 'c' | 'y' => {
                self.op = operator_key(c);
                Step::Pending
            }
            'x' => done(op_cmd(Operator::Delete, count, Motion::Right)),
            'D' => done(op_cmd(Operator::Delete, count, Motion::LineEnd)),
            'C' => done(op_cmd(Operator::Change, count, Motion::LineEnd)),
            'p' => done(Cmd::Put {
                before: false,
                count,
            }),
            'P' => done(Cmd::Put {
                before: true,
                count,
            }),
            'u' => done(Cmd::Undo { count }),
            '.' => done(Cmd::Repeat { count }),
            ':' => done(Cmd::CommandLine),
            LEADER => self.wait(Pending::Leader),
            c => match insert_key(c) {
                Some(at) => done(Cmd::Insert(at)),
                None => Step::Cancel,
            },
        }
    }

    fn wait(&mut self, pending: Pending) -> Step {
        self.pending = pending;
        Step::Pending
    }

    /// Both counts multiply, like Vim: `2d3w` deletes six words.
    fn total_count(&self) -> Option<usize> {
        match (self.count, self.op_count) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(1).saturating_mul(b.unwrap_or(1))),
        }
    }

    fn motion(&self, motion: Motion) -> Step {
        if self.op.is_some() {
            self.operate(Target::Motion(motion))
        } else {
            Step::Done(Cmd::Move {
                motion,
                count: self.total_count(),
            })
        }
    }

    fn operate(&self, target: Target) -> Step {
        match self.op {
            Some(op) => Step::Done(Cmd::Operate {
                op,
                count: self.total_count(),
                target,
            }),
            None => Step::Cancel,
        }
    }
}

fn op_cmd(op: Operator, count: Option<usize>, motion: Motion) -> Cmd {
    Cmd::Operate {
        op,
        count,
        target: Target::Motion(motion),
    }
}

/// Motion keys. `h t n s` are the Dvorak `h j k l` from the nvim config.
fn motion_key(code: KeyCode) -> Option<Motion> {
    use Motion::*;
    Some(match code {
        KeyCode::Char('h') | KeyCode::Left => Left,
        KeyCode::Char('t') | KeyCode::Down => Down,
        KeyCode::Char('n') | KeyCode::Up => Up,
        KeyCode::Char('s') | KeyCode::Right => Right,
        KeyCode::Char('0') | KeyCode::Home => LineStart,
        KeyCode::Char('^') => FirstNonBlank,
        KeyCode::Char('$') | KeyCode::End => LineEnd,
        KeyCode::Char('w') => WordForward { big: false },
        KeyCode::Char('W') => WordForward { big: true },
        KeyCode::Char('b') => WordBackward { big: false },
        KeyCode::Char('B') => WordBackward { big: true },
        KeyCode::Char('e') => WordEnd { big: false },
        KeyCode::Char('E') => WordEnd { big: true },
        KeyCode::Char('G') => FileEnd,
        _ => return None,
    })
}

fn operator_key(c: char) -> Option<Operator> {
    Some(match c {
        'd' => Operator::Delete,
        'c' => Operator::Change,
        'y' => Operator::Yank,
        _ => return None,
    })
}

fn object_key(c: char, inner: bool) -> Option<TextObject> {
    match c {
        'w' => Some(TextObject::Word { inner, big: false }),
        'W' => Some(TextObject::Word { inner, big: true }),
        _ => None,
    }
}

fn insert_key(c: char) -> Option<InsertAt> {
    Some(match c {
        'i' => InsertAt::Cursor,
        'a' => InsertAt::After,
        'I' => InsertAt::LineStart,
        'A' => InsertAt::LineEnd,
        'o' => InsertAt::LineBelow,
        'O' => InsertAt::LineAbove,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed `keys` and return the final step.
    fn parse(keys: &str) -> Step {
        let mut p = Parser::default();
        let mut last = Step::Pending;
        for c in keys.chars() {
            last = p.feed(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        last
    }

    fn done(keys: &str) -> Cmd {
        match parse(keys) {
            Step::Done(cmd) => cmd,
            other => panic!("{keys:?} gave {other:?}"),
        }
    }

    fn mv(motion: Motion, count: Option<usize>) -> Cmd {
        Cmd::Move { motion, count }
    }

    fn op(op: Operator, count: Option<usize>, target: Target) -> Cmd {
        Cmd::Operate { op, count, target }
    }

    const W: Motion = Motion::WordForward { big: false };

    #[test]
    fn motions_and_counts() {
        assert_eq!(done("w"), mv(W, None));
        assert_eq!(done("3w"), mv(W, Some(3)));
        assert_eq!(done("5t"), mv(Motion::Down, Some(5)));
        assert_eq!(done("12n"), mv(Motion::Up, Some(12)));
        assert_eq!(done("h"), mv(Motion::Left, None));
        assert_eq!(done("s"), mv(Motion::Right, None));
        assert_eq!(done("E"), mv(Motion::WordEnd { big: true }, None));
        assert_eq!(done("^"), mv(Motion::FirstNonBlank, None));
    }

    #[test]
    fn zero_is_a_motion_or_a_digit() {
        assert_eq!(done("0"), mv(Motion::LineStart, None));
        assert_eq!(done("10G"), mv(Motion::FileEnd, Some(10)));
    }

    #[test]
    fn g_prefix_and_find() {
        assert_eq!(parse("g"), Step::Pending);
        assert_eq!(done("gg"), mv(Motion::FileStart, None));
        assert_eq!(done("3gg"), mv(Motion::FileStart, Some(3)));
        assert_eq!(done("G"), mv(Motion::FileEnd, None));
        assert_eq!(done("fx"), mv(Motion::FindForward('x'), None));
        assert_eq!(done("2F,"), mv(Motion::FindBackward(','), Some(2)));
        // `f` takes any char, even one that is a command elsewhere.
        assert_eq!(done("ft"), mv(Motion::FindForward('t'), None));
        assert_eq!(parse("gx"), Step::Cancel);
    }

    #[test]
    fn operator_with_motion() {
        assert_eq!(parse("d"), Step::Pending);
        assert_eq!(done("dw"), op(Operator::Delete, None, Target::Motion(W)));
        assert_eq!(
            done("ce"),
            op(
                Operator::Change,
                None,
                Target::Motion(Motion::WordEnd { big: false })
            )
        );
        assert_eq!(
            done("y$"),
            op(Operator::Yank, None, Target::Motion(Motion::LineEnd))
        );
        assert_eq!(
            done("dfx"),
            op(
                Operator::Delete,
                None,
                Target::Motion(Motion::FindForward('x'))
            )
        );
        assert_eq!(
            done("dgg"),
            op(Operator::Delete, None, Target::Motion(Motion::FileStart))
        );
        assert_eq!(
            done("dt"),
            op(Operator::Delete, None, Target::Motion(Motion::Down))
        );
    }

    #[test]
    fn counts_multiply() {
        assert_eq!(
            done("3dw"),
            op(Operator::Delete, Some(3), Target::Motion(W))
        );
        assert_eq!(
            done("d3w"),
            op(Operator::Delete, Some(3), Target::Motion(W))
        );
        assert_eq!(
            done("2d3w"),
            op(Operator::Delete, Some(6), Target::Motion(W))
        );
    }

    #[test]
    fn operator_with_text_object() {
        let iw = TextObject::Word {
            inner: true,
            big: false,
        };
        let a_big = TextObject::Word {
            inner: false,
            big: true,
        };
        assert_eq!(done("diw"), op(Operator::Delete, None, Target::Object(iw)));
        assert_eq!(
            done("caW"),
            op(Operator::Change, None, Target::Object(a_big))
        );
        assert_eq!(
            done("2yiw"),
            op(Operator::Yank, Some(2), Target::Object(iw))
        );
        assert_eq!(parse("diq"), Step::Cancel);
    }

    #[test]
    fn doubled_operator_is_linewise() {
        assert_eq!(done("dd"), op(Operator::Delete, None, Target::Lines));
        assert_eq!(done("3cc"), op(Operator::Change, Some(3), Target::Lines));
        assert_eq!(done("yy"), op(Operator::Yank, None, Target::Lines));
        assert_eq!(parse("dy"), Step::Cancel);
    }

    #[test]
    fn shortcuts() {
        assert_eq!(
            done("x"),
            op(Operator::Delete, None, Target::Motion(Motion::Right))
        );
        assert_eq!(
            done("3x"),
            op(Operator::Delete, Some(3), Target::Motion(Motion::Right))
        );
        assert_eq!(
            done("D"),
            op(Operator::Delete, None, Target::Motion(Motion::LineEnd))
        );
        assert_eq!(
            done("C"),
            op(Operator::Change, None, Target::Motion(Motion::LineEnd))
        );
    }

    #[test]
    fn simple_commands() {
        assert_eq!(
            done("p"),
            Cmd::Put {
                before: false,
                count: None
            }
        );
        assert_eq!(
            done("2P"),
            Cmd::Put {
                before: true,
                count: Some(2)
            }
        );
        assert_eq!(done("u"), Cmd::Undo { count: None });
        assert_eq!(done("3."), Cmd::Repeat { count: Some(3) });
        assert_eq!(done(":"), Cmd::CommandLine);
        assert_eq!(done("o"), Cmd::Insert(InsertAt::LineBelow));
        assert_eq!(done("O"), Cmd::Insert(InsertAt::LineAbove));
        assert_eq!(done("I"), Cmd::Insert(InsertAt::LineStart));
        assert_eq!(done("A"), Cmd::Insert(InsertAt::LineEnd));
        assert_eq!(done("i"), Cmd::Insert(InsertAt::Cursor));
        assert_eq!(done("a"), Cmd::Insert(InsertAt::After));
    }

    #[test]
    fn ctrl_keys_and_leader() {
        let mut p = Parser::default();
        let ctrl = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        assert_eq!(p.feed(ctrl('r')), Step::Done(Cmd::Redo { count: None }));
        assert_eq!(p.feed(ctrl('s')), Step::Done(Cmd::Ex(Command::Write)));
        assert_eq!(done(" q"), Cmd::Ex(Command::Quit));
        assert_eq!(done(" Q"), Cmd::Ex(Command::ForceQuit));
        assert_eq!(parse(" z"), Step::Cancel);
    }

    #[test]
    fn esc_and_invalid_keys_reset() {
        let mut p = Parser::default();
        p.feed(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(p.is_pending());
        assert_eq!(
            p.feed(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Step::Cancel
        );
        assert!(!p.is_pending());
        assert_eq!(parse("Z"), Step::Cancel);
        // After a cancel the next command parses from scratch.
        assert_eq!(
            p.feed(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE)),
            Step::Done(mv(W, None))
        );
    }

    #[test]
    fn changes_and_count_override() {
        assert!(done("dw").is_change());
        assert!(done("p").is_change());
        assert!(done("o").is_change());
        assert!(!done("yy").is_change());
        assert!(!done("w").is_change());
        assert_eq!(
            done("dw").with_count(Some(2)),
            op(Operator::Delete, Some(2), Target::Motion(W))
        );
    }
}
