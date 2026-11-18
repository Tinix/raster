//! App: editor state, the event loop, key handling and drawing.

use crate::{
    buffer::Buffer,
    command::Command,
    config::{Action, Config, Shared},
    history::History,
    keymap::{Keymap, MapMode, Match, Rhs},
    keys::{self, Key},
    mode::Mode,
    options::Options,
    ui::{self, Status},
    vim::{
        motion::Motion,
        ops::{self, Operator, Register},
        parser::{Cmd, InsertAt, Parser, Step, Target},
    },
};
use anyhow::Result;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint, Layout, Position},
    style::{Color, Style},
    text::Line,
    widgets::Paragraph,
};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

/// The last change, kept for `.` repeat: the command plus the keys typed in
/// the Insert session it started (if any).
#[derive(Debug, Clone)]
struct Change {
    cmd: Cmd,
    insert_keys: Vec<KeyEvent>,
}

/// The whole editor: one buffer, a cursor and the current mode.
#[derive(Debug, Default)]
pub struct App {
    pub buffer: Buffer,
    pub mode: Mode,
    /// Cursor row (line index).
    pub row: usize,
    /// Cursor column (char index within the line).
    pub col: usize,
    /// Column to aim for when moving up and down (`usize::MAX` = end of line).
    want_col: usize,
    /// First visible line.
    pub scroll: usize,
    /// Text typed after `:` in Command mode.
    pub cmdline: String,
    /// Message shown on the bottom line (errors, "written", ...).
    pub message: Option<String>,
    pub should_quit: bool,
    /// Normal-mode key parser (operator-pending state machine).
    parser: Parser,
    /// The unnamed register.
    pub register: Register,
    history: History,
    last_change: Option<Change>,
    /// The change that started the current Insert session, finished on Esc.
    insert_change: Option<Cmd>,
    insert_keys: Vec<KeyEvent>,
    /// Replaying a change with `.`: don't record it again.
    replaying: bool,
    /// Key mappings, shared with the Lua config.
    pub keymap: Rc<RefCell<Keymap>>,
    /// Editor options, shared with the Lua config.
    pub options: Rc<RefCell<Options>>,
    /// Typed keys that may still become a mapping.
    pending_keys: Vec<Key>,
    /// When the first pending key was typed (for `timeoutlen`).
    pending_since: Option<Instant>,
    /// Lua runtime for `init.lua` and Lua mappings.
    config: Option<Config>,
}

impl App {
    pub fn new(path: Option<String>) -> Result<Self> {
        Ok(Self {
            buffer: Buffer::open(path)?,
            ..Self::default()
        })
    }

    /// Draw and handle events until the user quits.
    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|f| self.draw(f))?;
            // While a mapping is half typed, wait at most `timeoutlen`.
            if let Some(wait) = self.map_timeout()
                && !event::poll(wait)?
            {
                self.resolve_pending(true);
                continue;
            }
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.handle_key(key);
            }
        }
        Ok(())
    }

    /// Time left before a half-typed mapping resolves, if one is pending.
    pub fn map_timeout(&self) -> Option<Duration> {
        let since = self.pending_since?;
        let total = Duration::from_millis(self.options.borrow().timeoutlen);
        Some(total.saturating_sub(since.elapsed()))
    }

    /// Mappings that apply to the next key, if any.
    fn map_mode(&self) -> Option<MapMode> {
        match self.mode {
            Mode::Normal => self.parser.map_mode(),
            Mode::Insert => Some(MapMode::Insert),
            Mode::Command => None,
        }
    }

    /// Handle a typed key: match it against the keymap, then run it.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.map_mode().is_none() && self.pending_keys.is_empty() {
            self.dispatch(key);
            return;
        }
        self.pending_keys.push(Key::from(key));
        self.pending_since.get_or_insert_with(Instant::now);
        self.resolve_pending(false);
    }

    /// Turn pending keys into mappings or plain keys until they are used up
    /// or form the start of a longer mapping. `timed_out` stops waiting.
    pub fn resolve_pending(&mut self, mut timed_out: bool) {
        while !self.pending_keys.is_empty() {
            let Some(mode) = self.map_mode() else {
                let key = self.pending_keys.remove(0);
                self.dispatch(key.event());
                continue;
            };
            let found = self
                .keymap
                .borrow()
                .matches(mode, &self.pending_keys, timed_out);
            match found {
                Match::Wait => return,
                Match::Mapped { rhs, len } => {
                    self.pending_keys.drain(..len);
                    self.run_rhs(rhs);
                }
                Match::Unmapped => {
                    let key = self.pending_keys.remove(0);
                    self.dispatch(key.event());
                }
            }
            timed_out = false;
        }
        self.pending_since = None;
    }

    /// Run a mapping's right-hand side. Its keys are not remapped.
    fn run_rhs(&mut self, rhs: Rhs) {
        match rhs {
            Rhs::Keys(tokens) => {
                let keys = self.keymap.borrow().resolve(&tokens);
                self.feed_keys(&keys);
            }
            Rhs::Lua(id) => self.call_lua(id),
        }
    }

    /// Feed keys as if typed, without mappings.
    pub fn feed_keys(&mut self, keys: &[Key]) {
        for key in keys {
            self.dispatch(key.event());
        }
    }

    /// Start the Lua runtime and run the config file at `path`, if any.
    /// Errors end up on the message line; the editor starts anyway.
    pub fn load_config(&mut self, path: Option<&std::path::Path>) {
        let shared = Shared {
            options: self.options.clone(),
            keymap: self.keymap.clone(),
            ..Shared::default()
        };
        let config = match Config::new(&shared) {
            Ok(c) => c,
            Err(e) => {
                self.message = Some(format!("Lua: {e}"));
                return;
            }
        };
        let result = path.map_or(Ok(()), |p| config.load_file(p));
        self.config = Some(config);
        self.apply_lua_actions();
        if let Err(e) = result {
            self.message = Some(e);
        }
    }

    /// Call a Lua mapping, then apply what it asked for.
    fn call_lua(&mut self, id: usize) {
        let result = match &self.config {
            Some(config) => config.call(id),
            None => Err("no Lua runtime".into()),
        };
        self.apply_lua_actions();
        if let Err(e) = result {
            self.message = Some(e);
        }
    }

    fn apply_lua_actions(&mut self) {
        let actions = self
            .config
            .as_ref()
            .map(Config::take_actions)
            .unwrap_or_default();
        for action in actions {
            match action {
                Action::Cmd(cmd) => self.execute(Command::parse(&cmd)),
                Action::Feed(notation) => {
                    let leader = self.keymap.borrow().leader;
                    match keys::parse_keys(&notation, leader) {
                        Ok(keys) => self.feed_keys(&keys),
                        Err(e) => self.message = Some(e),
                    }
                }
                Action::Notify(msg) => self.message = Some(msg),
            }
        }
    }

    /// Run a key without mappings.
    fn dispatch(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Normal => {
                if let Step::Done(cmd) = self.parser.feed(key) {
                    self.run_cmd(cmd);
                }
            }
            Mode::Insert => self.insert_key(key),
            Mode::Command => self.command_key(key),
        }
    }

    fn cursor(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    fn set_cursor(&mut self, (row, col): (usize, usize)) {
        self.row = row.min(self.buffer.line_count().saturating_sub(1));
        self.col = col;
        self.clamp_col();
        self.want_col = self.col;
    }

    // --- Normal ------------------------------------------------------------

    /// Run a parsed Normal-mode command.
    pub fn run_cmd(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Move { motion, count } => self.move_cursor(motion, count),
            Cmd::Undo { count } => self.undo(count, false),
            Cmd::Redo { count } => self.undo(count, true),
            Cmd::Repeat { count } => self.repeat(count),
            Cmd::CommandLine => {
                self.mode = Mode::Command;
                self.cmdline.clear();
                self.message = None;
            }
            Cmd::Operate {
                op: Operator::Yank,
                count,
                target,
            } => self.operate(Operator::Yank, count, target),
            change => self.change(change),
        }
    }

    /// Run a command that edits the buffer as one undo step, and remember
    /// it for `.`. If it starts an Insert session, the step and the record
    /// are finished when Insert ends.
    fn change(&mut self, cmd: Cmd) {
        self.history.checkpoint(&self.buffer, self.cursor());
        match cmd {
            Cmd::Operate { op, count, target } => self.operate(op, count, target),
            Cmd::Insert(at) => self.start_insert(at),
            Cmd::Put { before, count } => {
                let cur = self.cursor();
                let cur = ops::put(&mut self.buffer, &self.register, cur, before, count);
                self.set_cursor(cur);
            }
            _ => unreachable!("not a change: {cmd:?}"),
        }
        if self.mode == Mode::Insert {
            if !self.replaying {
                self.insert_change = Some(cmd);
                self.insert_keys.clear();
            }
        } else {
            self.history.discard_if_unchanged(&self.buffer);
            if !self.replaying {
                self.last_change = Some(Change {
                    cmd,
                    insert_keys: Vec::new(),
                });
            }
        }
    }

    fn operate(&mut self, op: Operator, count: Option<usize>, target: Target) {
        let cur = self.cursor();
        let span = match target {
            Target::Motion(m) => ops::motion_span(&self.buffer, op, cur, m, count),
            Target::Object(obj) => ops::object_span(&self.buffer, cur, obj, count),
            Target::Lines => Some(ops::lines_span(&self.buffer, cur, count)),
        };
        let Some(span) = span else {
            return;
        };
        let cur = ops::apply(&mut self.buffer, &mut self.register, op, span, cur);
        let mut cur = cur;
        if op == Operator::Change {
            self.mode = Mode::Insert;
            // `cc` keeps the indent only with 'autoindent'.
            if target == Target::Lines && !self.options.borrow().autoindent {
                let start = self.buffer.pos_to_idx(cur.0, 0);
                self.buffer.remove(start..start + cur.1);
                cur.1 = 0;
            }
        }
        self.set_cursor(cur);
    }

    fn move_cursor(&mut self, motion: Motion, count: Option<usize>) {
        let Some((row, col)) = motion.apply(&self.buffer, self.cursor(), count) else {
            return;
        };
        self.row = row;
        match motion {
            Motion::Up | Motion::Down => self.col = self.want_col.min(self.max_col()),
            Motion::LineEnd => {
                self.col = self.max_col();
                self.want_col = usize::MAX;
            }
            _ => self.set_cursor((row, col)),
        }
    }

    /// `u` (or `<C-r>` with `redo`), `count` times.
    fn undo(&mut self, count: Option<usize>, redo: bool) {
        for _ in 0..count.unwrap_or(1).max(1) {
            let cur = if redo {
                self.history.redo(&mut self.buffer)
            } else {
                self.history.undo(&mut self.buffer)
            };
            match cur {
                Some(cur) => self.set_cursor(cur),
                None => {
                    let edge = if redo { "newest" } else { "oldest" };
                    self.message = Some(format!("Already at {edge} change"));
                    break;
                }
            }
        }
    }

    /// `.`: replay the last change, with a new count if one is given.
    fn repeat(&mut self, count: Option<usize>) {
        let Some(change) = self.last_change.clone() else {
            return;
        };
        let cmd = match count {
            Some(_) => change.cmd.clone().with_count(count),
            None => change.cmd.clone(),
        };
        self.replaying = true;
        self.change(cmd.clone());
        if self.mode == Mode::Insert {
            for key in &change.insert_keys {
                self.insert_key(*key);
            }
            self.insert_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        }
        self.replaying = false;
        // Like Vim, a count given to `.` sticks for the next `.`.
        self.last_change = Some(Change { cmd, ..change });
    }

    // --- Insert ------------------------------------------------------------

    fn start_insert(&mut self, at: InsertAt) {
        self.mode = Mode::Insert;
        let row = self.row;
        match at {
            InsertAt::Cursor => {}
            InsertAt::After => self.col = (self.col + 1).min(self.buffer.line_len(row)),
            InsertAt::LineStart => self.col = self.buffer.first_non_blank(row),
            InsertAt::LineEnd => self.col = self.buffer.line_len(row),
            InsertAt::LineBelow | InsertAt::LineAbove => {
                let indent = if self.options.borrow().autoindent {
                    self.indent(row)
                } else {
                    String::new()
                };
                if at == InsertAt::LineBelow {
                    let idx = self.buffer.line_end(row);
                    self.buffer.insert(idx, &format!("\n{indent}"));
                    self.row += 1;
                } else {
                    let idx = self.buffer.pos_to_idx(row, 0);
                    self.buffer.insert(idx, &format!("{indent}\n"));
                }
                self.col = indent.chars().count();
            }
        }
        self.want_col = self.col;
    }

    /// Leading white space of `row`.
    fn indent(&self, row: usize) -> String {
        let start = self.buffer.pos_to_idx(row, 0);
        self.buffer
            .slice(start..start + self.buffer.first_non_blank(row))
    }

    fn insert_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.finish_insert();
            return;
        }
        if self.insert_change.is_some() {
            self.insert_keys.push(key);
        }
        match key.code {
            KeyCode::Char(c) => {
                self.buffer.insert_char(self.row, self.col, c);
                self.col += 1;
                self.want_col = self.col;
            }
            KeyCode::Enter => {
                let mut indent = String::new();
                if self.options.borrow().autoindent {
                    indent = self.indent(self.row);
                    indent.truncate(
                        indent
                            .char_indices()
                            .nth(self.col)
                            .map_or(indent.len(), |(i, _)| i),
                    );
                }
                let idx = self.buffer.pos_to_idx(self.row, self.col);
                self.buffer.insert(idx, &format!("\n{indent}"));
                self.row += 1;
                self.col = indent.chars().count();
                self.want_col = self.col;
            }
            KeyCode::Tab => {
                let (expand, tabstop) = {
                    let o = self.options.borrow();
                    (o.expandtab, o.tabstop)
                };
                let text = if expand {
                    let line = self.line_text(self.row);
                    let width = ui::display_col(&line, self.col, tabstop);
                    " ".repeat(tabstop - width % tabstop)
                } else {
                    "\t".to_owned()
                };
                let idx = self.buffer.pos_to_idx(self.row, self.col);
                self.buffer.insert(idx, &text);
                self.col += text.chars().count();
                self.want_col = self.col;
            }
            KeyCode::Backspace => {
                (self.row, self.col) = self.buffer.backspace(self.row, self.col);
                self.want_col = self.col;
            }
            code => {
                let motion = match code {
                    KeyCode::Left => Motion::Left,
                    KeyCode::Down => Motion::Down,
                    KeyCode::Up => Motion::Up,
                    KeyCode::Right => Motion::Right,
                    KeyCode::Home => Motion::LineStart,
                    KeyCode::End => Motion::LineEnd,
                    _ => return,
                };
                self.move_cursor(motion, None);
            }
        }
    }

    /// Leave Insert: close the undo step and record the change for `.`.
    fn finish_insert(&mut self) {
        self.mode = Mode::Normal;
        // Like Vim, leaving Insert steps back onto the last typed char.
        self.col = self.col.saturating_sub(1);
        self.clamp_col();
        self.want_col = self.col;
        self.history.discard_if_unchanged(&self.buffer);
        if let Some(cmd) = self.insert_change.take() {
            self.last_change = Some(Change {
                cmd,
                insert_keys: std::mem::take(&mut self.insert_keys),
            });
        }
    }

    // --- Command -----------------------------------------------------------

    fn command_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                let cmd = Command::parse(&std::mem::take(&mut self.cmdline));
                self.execute(cmd);
            }
            KeyCode::Backspace => {
                if self.cmdline.pop().is_none() {
                    self.mode = Mode::Normal;
                }
            }
            KeyCode::Char(c) => self.cmdline.push(c),
            _ => {}
        }
    }

    /// Run an ex-command.
    pub fn execute(&mut self, cmd: Command) {
        match cmd {
            Command::Write => {
                self.write();
            }
            Command::Quit => {
                if self.buffer.dirty {
                    self.message =
                        Some("E37: No write since last change (add ! to override)".into());
                } else {
                    self.should_quit = true;
                }
            }
            Command::ForceQuit => self.should_quit = true,
            Command::WriteQuit => {
                if self.write() {
                    self.should_quit = true;
                }
            }
            Command::Set(args) => {
                let result = self.options.borrow_mut().set_from_ex(&args);
                match result {
                    Ok(shown) => self.message = shown,
                    Err(e) => self.message = Some(e),
                }
            }
            Command::Unknown(s) if s.is_empty() => {}
            Command::Unknown(s) => {
                self.message = Some(format!("E492: Not an editor command: {s}"));
            }
        }
    }

    /// Save the buffer, reporting the outcome on the message line.
    fn write(&mut self) -> bool {
        if self.buffer.path.is_none() {
            self.message = Some("E32: No file name".into());
            return false;
        }
        match self.buffer.save() {
            Ok(()) => {
                self.message = Some(format!(
                    "\"{}\" {}L written",
                    self.file_name().unwrap_or_default(),
                    self.buffer.line_count()
                ));
                true
            }
            Err(e) => {
                self.message = Some(format!("E212: Can't write file: {e}"));
                false
            }
        }
    }

    // --- Motions -----------------------------------------------------------

    /// Last column the cursor may sit on in the current mode.
    fn max_col(&self) -> usize {
        let len = self.buffer.line_len(self.row);
        match self.mode {
            Mode::Insert => len,
            _ => len.saturating_sub(1),
        }
    }

    fn clamp_col(&mut self) {
        self.col = self.col.min(self.max_col());
    }

    // --- Drawing -----------------------------------------------------------

    fn file_name(&self) -> Option<String> {
        let path = self.buffer.path.as_ref()?;
        Some(
            path.file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned(),
        )
    }

    /// Keep the cursor row inside the visible window of `height` lines.
    /// Keep the cursor row inside the visible window of `height` lines,
    /// with 'scrolloff' lines of context when possible.
    fn scroll_to_cursor(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        let so = self.options.borrow().scrolloff.min((height - 1) / 2);
        let last_top = self.buffer.line_count().saturating_sub(height);
        if self.row < self.scroll + so {
            self.scroll = self.row.saturating_sub(so);
        } else if self.row + so >= self.scroll + height {
            self.scroll = (self.row + so + 1 - height).min(last_top.max(self.scroll));
        }
    }

    /// Text of `row` without its line break.
    fn line_text(&self, row: usize) -> String {
        let start = self.buffer.pos_to_idx(row, 0);
        self.buffer.slice(start..self.buffer.line_end(row))
    }

    fn draw(&mut self, frame: &mut Frame) {
        let [text_area, status_area, cmd_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        self.scroll_to_cursor(text_area.height as usize);

        let opts = self.options.borrow().clone();
        let gutter = ui::gutter_width(opts.number, opts.relativenumber, self.buffer.line_count());
        let [gutter_area, text_area] =
            Layout::horizontal([Constraint::Length(gutter as u16), Constraint::Min(1)])
                .areas(text_area);

        let rows = self.scroll
            ..self
                .buffer
                .line_count()
                .min(self.scroll + text_area.height as usize);
        let lines: Vec<Line> = rows
            .clone()
            .map(|r| Line::raw(ui::expand_tabs(&self.line_text(r), opts.tabstop)))
            .collect();
        frame.render_widget(Paragraph::new(lines), text_area);
        if gutter > 0 {
            let numbers: Vec<Line> = rows
                .map(|r| {
                    let n = ui::line_number(r, self.row, opts.number, opts.relativenumber, gutter);
                    Line::styled(n, Style::default().fg(Color::DarkGray))
                })
                .collect();
            frame.render_widget(Paragraph::new(numbers), gutter_area);
        }

        let name = self.file_name();
        let status = Status {
            mode: self.mode,
            file_name: name.as_deref(),
            dirty: self.buffer.dirty,
            row: self.row,
            col: self.col,
        };
        frame.render_widget(ui::status_line(status, status_area.width), status_area);

        if self.mode == Mode::Command {
            let text = format!(":{}", self.cmdline);
            let x = cmd_area.x + text.chars().count() as u16;
            frame.render_widget(Line::raw(text), cmd_area);
            frame.set_cursor_position(Position::new(x, cmd_area.y));
        } else {
            if let Some(msg) = &self.message {
                frame.render_widget(Line::raw(msg.clone()), cmd_area);
            }
            let col = ui::display_col(&self.line_text(self.row), self.col, opts.tabstop);
            let x = text_area.x + col as u16;
            let y = text_area.y + (self.row - self.scroll) as u16;
            frame.set_cursor_position(Position::new(x, y));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(text: &str) -> App {
        let mut app = App::default();
        app.buffer.text.insert(0, text);
        app
    }

    fn press(app: &mut App, code: KeyCode) {
        app.handle_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn keys(app: &mut App, s: &str) {
        for c in s.chars() {
            press(app, KeyCode::Char(c));
        }
    }

    #[test]
    fn dvorak_motions() {
        let mut a = app("abc\ndef\nghi");
        keys(&mut a, "ss");
        assert_eq!((a.row, a.col), (0, 2));
        keys(&mut a, "t");
        assert_eq!((a.row, a.col), (1, 2));
        keys(&mut a, "h");
        assert_eq!((a.row, a.col), (1, 1));
        keys(&mut a, "n");
        assert_eq!((a.row, a.col), (0, 1));
    }

    #[test]
    fn arrows_move_too() {
        let mut a = app("abc\ndef");
        press(&mut a, KeyCode::Right);
        press(&mut a, KeyCode::Down);
        assert_eq!((a.row, a.col), (1, 1));
        press(&mut a, KeyCode::Left);
        press(&mut a, KeyCode::Up);
        assert_eq!((a.row, a.col), (0, 0));
    }

    #[test]
    fn motions_stop_at_edges() {
        let mut a = app("ab\nc");
        keys(&mut a, "hnsss");
        assert_eq!((a.row, a.col), (0, 1));
        keys(&mut a, "ttt");
        assert_eq!((a.row, a.col), (1, 0));
    }

    #[test]
    fn zero_and_dollar() {
        let mut a = app("hello\nhi\nworld");
        keys(&mut a, "$");
        assert_eq!(a.col, 4);
        // `$` sticks to the end of line when moving vertically.
        keys(&mut a, "t");
        assert_eq!(a.col, 1);
        keys(&mut a, "t");
        assert_eq!(a.col, 4);
        keys(&mut a, "0");
        assert_eq!(a.col, 0);
    }

    #[test]
    fn vertical_moves_remember_column() {
        let mut a = app("hello\nhi\nworld");
        keys(&mut a, "ssss");
        keys(&mut a, "t");
        assert_eq!(a.col, 1);
        keys(&mut a, "t");
        assert_eq!(a.col, 4);
    }

    #[test]
    fn insert_typing_enter_and_backspace() {
        let mut a = app("");
        keys(&mut a, "i");
        assert_eq!(a.mode, Mode::Insert);
        keys(&mut a, "ab");
        press(&mut a, KeyCode::Enter);
        keys(&mut a, "cd");
        assert_eq!(a.buffer.text.to_string(), "ab\ncd");
        assert_eq!((a.row, a.col), (1, 2));

        press(&mut a, KeyCode::Backspace);
        press(&mut a, KeyCode::Backspace);
        press(&mut a, KeyCode::Backspace);
        assert_eq!(a.buffer.text.to_string(), "ab");
        assert_eq!((a.row, a.col), (0, 2));
        assert!(a.buffer.dirty);

        press(&mut a, KeyCode::Esc);
        assert_eq!(a.mode, Mode::Normal);
        assert_eq!(a.col, 1);
    }

    #[test]
    fn append_inserts_after_cursor() {
        let mut a = app("ac");
        keys(&mut a, "ab");
        assert_eq!(a.buffer.text.to_string(), "abc");
    }

    #[test]
    fn insert_mode_does_not_run_motions() {
        let mut a = app("");
        keys(&mut a, "ithns");
        assert_eq!(a.buffer.text.to_string(), "thns");
    }

    #[test]
    fn command_mode_typing_and_cancel() {
        let mut a = app("");
        keys(&mut a, ":wq");
        assert_eq!(a.mode, Mode::Command);
        assert_eq!(a.cmdline, "wq");
        press(&mut a, KeyCode::Esc);
        assert_eq!(a.mode, Mode::Normal);
        assert!(!a.should_quit);
    }

    #[test]
    fn backspace_on_empty_cmdline_leaves_command_mode() {
        let mut a = app("");
        keys(&mut a, ":");
        press(&mut a, KeyCode::Backspace);
        assert_eq!(a.mode, Mode::Normal);
    }

    #[test]
    fn quit_refuses_when_dirty() {
        let mut a = app("");
        a.buffer.dirty = true;
        keys(&mut a, ":q");
        press(&mut a, KeyCode::Enter);
        assert!(!a.should_quit);
        assert!(a.message.as_deref().unwrap().starts_with("E37"));

        keys(&mut a, ":q!");
        press(&mut a, KeyCode::Enter);
        assert!(a.should_quit);
    }

    #[test]
    fn quit_when_clean() {
        let mut a = app("x");
        keys(&mut a, ":q");
        press(&mut a, KeyCode::Enter);
        assert!(a.should_quit);
    }

    #[test]
    fn leader_quit_and_force_quit() {
        let mut a = app("");
        a.buffer.dirty = true;
        keys(&mut a, " q");
        assert!(!a.should_quit);
        keys(&mut a, " Q");
        assert!(a.should_quit);
    }

    #[test]
    fn write_without_name_fails() {
        let mut a = app("x");
        a.buffer.dirty = true;
        keys(&mut a, ":wq");
        press(&mut a, KeyCode::Enter);
        assert!(!a.should_quit);
        assert_eq!(a.message.as_deref(), Some("E32: No file name"));
    }

    #[test]
    fn write_and_quit_saves_file() {
        for cmd in [":wq", ":x"] {
            let path = std::env::temp_dir().join(format!("raster_app_{}.txt", &cmd[1..]));
            let mut a = App::new(Some(path.display().to_string())).unwrap();
            keys(&mut a, "ihi");
            press(&mut a, KeyCode::Esc);
            keys(&mut a, cmd);
            press(&mut a, KeyCode::Enter);
            assert!(a.should_quit);
            assert!(!a.buffer.dirty);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "hi\n");
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn ctrl_s_saves() {
        let path = std::env::temp_dir().join("raster_app_ctrl_s.txt");
        let mut a = App::new(Some(path.display().to_string())).unwrap();
        keys(&mut a, "iok");
        press(&mut a, KeyCode::Esc);
        a.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert!(!a.buffer.dirty);
        assert_eq!((a.row, a.col), (0, 1));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "ok\n");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn unknown_command_reports_error() {
        let mut a = app("");
        keys(&mut a, ":foo");
        press(&mut a, KeyCode::Enter);
        assert_eq!(
            a.message.as_deref(),
            Some("E492: Not an editor command: foo")
        );
    }

    fn text(a: &App) -> String {
        a.buffer.text.to_string()
    }

    fn esc(a: &mut App) {
        press(a, KeyCode::Esc);
    }

    fn ctrl(a: &mut App, c: char) {
        a.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
    }

    #[test]
    fn counted_motions() {
        let mut a = app("one two three four\nb\nc\nd\ne\nf\ng");
        keys(&mut a, "3w");
        assert_eq!((a.row, a.col), (0, 14));
        keys(&mut a, "2b");
        assert_eq!((a.row, a.col), (0, 4));
        keys(&mut a, "e");
        assert_eq!((a.row, a.col), (0, 6));
        keys(&mut a, "5t");
        assert_eq!(a.row, 5);
        keys(&mut a, "3n");
        assert_eq!(a.row, 2);
    }

    #[test]
    fn file_motions() {
        let mut a = app("one\n  two\nthree\nfour");
        keys(&mut a, "G");
        assert_eq!((a.row, a.col), (3, 0));
        keys(&mut a, "gg");
        assert_eq!((a.row, a.col), (0, 0));
        keys(&mut a, "2G");
        assert_eq!((a.row, a.col), (1, 2));
        keys(&mut a, "3gg");
        assert_eq!(a.row, 2);
    }

    #[test]
    fn caret_and_find() {
        let mut a = app("   foo(a, b)");
        keys(&mut a, "$^");
        assert_eq!(a.col, 3);
        keys(&mut a, "f,");
        assert_eq!(a.col, 8);
        keys(&mut a, "F(");
        assert_eq!(a.col, 6);
    }

    #[test]
    fn w_past_the_last_word_stays_on_the_last_char() {
        let mut a = app("ab cd");
        keys(&mut a, "5w");
        assert_eq!(a.col, 4);
    }

    #[test]
    fn insert_entries() {
        let mut a = app("  foo");
        keys(&mut a, "$Ix");
        esc(&mut a);
        assert_eq!(text(&a), "  xfoo");
        keys(&mut a, "Ay");
        esc(&mut a);
        assert_eq!(text(&a), "  xfooy");
        keys(&mut a, "obar");
        esc(&mut a);
        assert_eq!(text(&a), "  xfooy\n  bar");
        keys(&mut a, "Obaz");
        esc(&mut a);
        assert_eq!(text(&a), "  xfooy\n  baz\n  bar");
        assert_eq!(a.row, 1);
    }

    #[test]
    fn delete_change_yank_and_put() {
        let mut a = app("foo bar baz");
        keys(&mut a, "wdw");
        assert_eq!(text(&a), "foo baz");
        keys(&mut a, "P");
        assert_eq!(text(&a), "foo bar baz");

        keys(&mut a, "0ciwqux");
        esc(&mut a);
        assert_eq!(text(&a), "qux bar baz");
        assert_eq!(a.mode, Mode::Normal);

        keys(&mut a, "$D");
        assert_eq!(text(&a), "qux bar ba");
        keys(&mut a, "0Cnew");
        esc(&mut a);
        assert_eq!(text(&a), "new");
        keys(&mut a, "0x");
        assert_eq!(text(&a), "ew");
        assert_eq!(a.register.text, "n");
    }

    #[test]
    fn dd_yy_and_linewise_put() {
        let mut a = app("a\nb\nc");
        keys(&mut a, "dd");
        assert_eq!(text(&a), "b\nc");
        keys(&mut a, "p");
        assert_eq!(text(&a), "b\na\nc");
        assert_eq!(a.row, 1);
        keys(&mut a, "yyGp");
        assert_eq!(text(&a), "b\na\nc\na");
        keys(&mut a, "gg2dd");
        assert_eq!(text(&a), "c\na");
        keys(&mut a, "cct");
        esc(&mut a);
        assert_eq!(text(&a), "t\na");
    }

    #[test]
    fn cw_then_typing() {
        let mut a = app("foo bar");
        keys(&mut a, "cwqux");
        esc(&mut a);
        assert_eq!(text(&a), "qux bar");
    }

    #[test]
    fn undo_and_redo() {
        let mut a = app("a b c");
        keys(&mut a, "dwdw");
        assert_eq!(text(&a), "c");
        keys(&mut a, "u");
        assert_eq!(text(&a), "b c");
        keys(&mut a, "u");
        assert_eq!(text(&a), "a b c");
        keys(&mut a, "u");
        assert_eq!(a.message.as_deref(), Some("Already at oldest change"));
        ctrl(&mut a, 'r');
        assert_eq!(text(&a), "b c");
        keys(&mut a, "2u");
        assert_eq!(text(&a), "a b c");
        ctrl(&mut a, 'r');
        ctrl(&mut a, 'r');
        assert_eq!(text(&a), "c");
    }

    #[test]
    fn insert_session_is_one_undo_step() {
        let mut a = app("x");
        keys(&mut a, "ahello");
        press(&mut a, KeyCode::Enter);
        keys(&mut a, "world");
        esc(&mut a);
        assert_eq!(text(&a), "xhello\nworld");
        keys(&mut a, "u");
        assert_eq!(text(&a), "x");
        assert_eq!((a.row, a.col), (0, 0));
        // `cw` plus the typing is one step too.
        keys(&mut a, "cwyz");
        esc(&mut a);
        keys(&mut a, "u");
        assert_eq!(text(&a), "x");
    }

    #[test]
    fn empty_insert_and_failed_motion_leave_no_undo_step() {
        let mut a = app("a b");
        keys(&mut a, "dw");
        keys(&mut a, "i");
        esc(&mut a);
        keys(&mut a, "dfz");
        keys(&mut a, "u");
        assert_eq!(text(&a), "a b");
    }

    #[test]
    fn undo_back_to_saved_state_is_clean() {
        let mut a = app("abc");
        keys(&mut a, "x");
        assert!(a.buffer.dirty);
        keys(&mut a, "u");
        assert!(!a.buffer.dirty);
    }

    #[test]
    fn yank_is_not_undoable_or_repeatable() {
        let mut a = app("a b");
        keys(&mut a, "x");
        keys(&mut a, "yw");
        keys(&mut a, "u");
        assert_eq!(text(&a), "a b");
        keys(&mut a, "yw.");
        assert_eq!(text(&a), " b");
    }

    #[test]
    fn dot_repeats_operator() {
        let mut a = app("a b c d e");
        keys(&mut a, "dw..");
        assert_eq!(text(&a), "d e");
        // A count on `.` replaces the original count and sticks.
        let mut a = app("a b c d e f");
        keys(&mut a, "dw2.");
        assert_eq!(text(&a), "d e f");
        keys(&mut a, ".");
        assert_eq!(text(&a), "f");
    }

    #[test]
    fn dot_repeats_change_with_typed_text() {
        let mut a = app("foo bar");
        keys(&mut a, "ciwX");
        esc(&mut a);
        keys(&mut a, "w.");
        assert_eq!(text(&a), "X X");
    }

    #[test]
    fn dot_repeats_insert_entries() {
        let mut a = app("a\nb");
        keys(&mut a, "A;");
        esc(&mut a);
        keys(&mut a, "t.");
        assert_eq!(text(&a), "a;\nb;");

        let mut a = app("x");
        keys(&mut a, "o-");
        esc(&mut a);
        keys(&mut a, ".");
        assert_eq!(text(&a), "x\n-\n-");
    }

    #[test]
    fn dot_repeats_x_dd_and_put() {
        let mut a = app("abcdef");
        keys(&mut a, "2x.");
        assert_eq!(text(&a), "ef");

        let mut a = app("1\n2\n3\n4");
        keys(&mut a, "dd.");
        assert_eq!(text(&a), "3\n4");

        let mut a = app("a");
        keys(&mut a, "yyp.");
        assert_eq!(text(&a), "a\na\na");
    }

    #[test]
    fn each_dot_is_one_undo_step() {
        let mut a = app("foo bar");
        keys(&mut a, "ciwX");
        esc(&mut a);
        keys(&mut a, "w.u");
        assert_eq!(text(&a), "X bar");
    }

    #[test]
    fn operator_pending_esc_cancels() {
        let mut a = app("abc");
        keys(&mut a, "d");
        esc(&mut a);
        keys(&mut a, "s");
        assert_eq!((text(&a).as_str(), a.col), ("abc", 1));
    }

    #[test]
    fn dvorak_maps_apply_in_normal_but_not_after_operators() {
        // `dt,` is Vim's "delete till", as in the nvim config.
        let mut a = app("foo, bar\nnext");
        keys(&mut a, "dt,");
        assert_eq!(text(&a), ", bar\nnext");
        // `t` alone still moves down, with counts too.
        let mut a = app("a\nb\nc\nd");
        keys(&mut a, "2t");
        assert_eq!(a.row, 2);
        keys(&mut a, "n");
        assert_eq!(a.row, 1);
    }

    #[test]
    fn vim_builtins_under_the_maps() {
        let mut a = app("a(b, c)");
        keys(&mut a, "$T(");
        assert_eq!(a.col, 2);
        keys(&mut a, "jkl");
        assert_eq!(a.col, 3);
        let mut a = app("abc\n  def");
        keys(&mut a, "tSx");
        esc(&mut a);
        assert_eq!(text(&a), "abc\n  x");
    }

    #[test]
    fn char_arguments_are_not_remapped() {
        let mut a = app("xtx");
        keys(&mut a, "ft");
        assert_eq!(a.col, 1);
        let mut a = app("a s b");
        keys(&mut a, "dfs");
        assert_eq!(text(&a), " b");
    }

    #[test]
    fn command_line_is_not_remapped() {
        let mut a = app("");
        keys(&mut a, ":tns");
        assert_eq!(a.cmdline, "tns");
    }

    #[test]
    fn leader_waits_and_times_out() {
        let mut a = app("abc");
        keys(&mut a, " ");
        assert!(a.map_timeout().is_some());
        assert_eq!(a.col, 0);
        a.resolve_pending(true);
        // `<Space>` is `<Nop>`: the cursor did not move.
        assert_eq!(a.col, 0);
        assert!(a.map_timeout().is_none());
        // A key that breaks the sequence runs `<Space>`, then itself.
        keys(&mut a, " s");
        assert_eq!(a.col, 1);
    }

    #[test]
    fn user_mappings_override_defaults() {
        let mut a = app("a\nb\nc");
        a.keymap.borrow_mut().set_keys("n", "t", "G", None).unwrap();
        keys(&mut a, "t");
        assert_eq!(a.row, 2);
        a.keymap
            .borrow_mut()
            .del(&[MapMode::Normal], &crate::keys::parse("t").unwrap())
            .unwrap();
        // Without the mapping, `t` is Vim's till again.
        keys(&mut a, "gg");
        a.buffer.insert(0, "x,");
        keys(&mut a, "dt,");
        assert_eq!(text(&a), ",a\nb\nc");
    }

    #[test]
    fn insert_mode_mappings() {
        let mut a = app("");
        a.keymap
            .borrow_mut()
            .set_keys("i", "jk", "<Esc>", None)
            .unwrap();
        keys(&mut a, "ihijk");
        assert_eq!(a.mode, Mode::Normal);
        assert_eq!(text(&a), "hi");
        // A lone `j` is typed after the timeout.
        keys(&mut a, "aj");
        assert_eq!(text(&a), "hi");
        a.resolve_pending(true);
        assert_eq!(text(&a), "hij");
    }

    #[test]
    fn mapping_rhs_is_not_remapped() {
        let mut a = app("a\nb\nc");
        // `x` -> `t`: `t` must be Vim's till here, not the Dvorak `j`.
        a.keymap.borrow_mut().set_keys("n", "x", "t", None).unwrap();
        keys(&mut a, "x");
        assert_eq!(a.row, 0);
        keys(&mut a, "c");
        assert_eq!(a.row, 0);
    }

    fn set_opt(a: &App, name: &str, v: crate::options::Value) {
        a.options.borrow_mut().set(name, v).unwrap();
    }

    /// Draw on a `w`x`h` test terminal: (screen rows, cursor x, cursor y).
    fn render(a: &mut App, w: u16, h: u16) -> (Vec<String>, u16, u16) {
        use ratatui::{Terminal, backend::TestBackend};
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| a.draw(f)).unwrap();
        let buf = t.backend().buffer().clone();
        let rows = (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect();
        let cur = t.get_cursor_position().unwrap();
        (rows, cur.x, cur.y)
    }

    use crate::options::Value::{Bool, Int};

    #[test]
    fn number_gutter() {
        let mut a = app("a\nb");
        set_opt(&a, "number", Bool(true));
        keys(&mut a, "t");
        let (rows, x, y) = render(&mut a, 12, 4);
        assert_eq!(rows[0], "  1 a       ");
        assert_eq!(rows[1], "  2 b       ");
        assert_eq!((x, y), (4, 1));

        set_opt(&a, "relativenumber", Bool(true));
        let (rows, ..) = render(&mut a, 12, 4);
        assert_eq!(rows[0], "  1 a       ");
        assert_eq!(rows[1], "  2 b       ");
        set_opt(&a, "number", Bool(false));
        let (rows, ..) = render(&mut a, 12, 4);
        assert_eq!(rows[1], "  0 b       ");
    }

    #[test]
    fn tabs_render_with_tabstop_and_cursor_follows() {
        let mut a = app("\tx");
        set_opt(&a, "tabstop", Int(4));
        keys(&mut a, "s");
        let (rows, x, _) = render(&mut a, 10, 3);
        assert_eq!(rows[0], "    x     ");
        assert_eq!(x, 4);
    }

    #[test]
    fn scrolloff_keeps_context() {
        let text: Vec<String> = (1..=20).map(|n| n.to_string()).collect();
        let mut a = app(&text.join("\n"));
        set_opt(&a, "scrolloff", Int(2));
        // 5 text rows (7 minus status and command lines).
        keys(&mut a, "4t");
        render(&mut a, 10, 7);
        assert_eq!(a.scroll, 2);
        keys(&mut a, "G");
        let (rows, _, y) = render(&mut a, 10, 7);
        // Never scrolls past the last line.
        assert_eq!(a.scroll, 15);
        assert_eq!((rows[4].trim(), y), ("20", 4));
        keys(&mut a, "n");
        render(&mut a, 10, 7);
        assert_eq!(a.scroll, 15);
        keys(&mut a, "gg");
        render(&mut a, 10, 7);
        assert_eq!(a.scroll, 0);
        // On a short window 'scrolloff' is capped to half the height.
        keys(&mut a, "3t");
        render(&mut a, 10, 6);
        assert_eq!(a.scroll, 1);
    }

    #[test]
    fn tab_key_with_and_without_expandtab() {
        let mut a = app("");
        keys(&mut a, "i");
        press(&mut a, KeyCode::Tab);
        assert_eq!(text(&a), "\t");
        esc(&mut a);

        let mut a = app("");
        set_opt(&a, "expandtab", Bool(true));
        set_opt(&a, "tabstop", Int(4));
        keys(&mut a, "iab");
        press(&mut a, KeyCode::Tab);
        keys(&mut a, "c");
        assert_eq!(text(&a), "ab  c");
    }

    #[test]
    fn autoindent_on_enter_and_off() {
        let mut a = app("    foo");
        keys(&mut a, "A");
        press(&mut a, KeyCode::Enter);
        keys(&mut a, "bar");
        esc(&mut a);
        assert_eq!(text(&a), "    foo\n    bar");

        let mut a = app("    foo");
        set_opt(&a, "autoindent", Bool(false));
        keys(&mut a, "A");
        press(&mut a, KeyCode::Enter);
        keys(&mut a, "x");
        esc(&mut a);
        keys(&mut a, "oy");
        esc(&mut a);
        keys(&mut a, "ggccz");
        esc(&mut a);
        assert_eq!(text(&a), "z\nx\ny");
    }

    #[test]
    fn enter_inside_indent_keeps_only_what_is_before_the_cursor() {
        let mut a = app("    foo");
        keys(&mut a, "ss");
        keys(&mut a, "i");
        press(&mut a, KeyCode::Enter);
        esc(&mut a);
        assert_eq!(text(&a), "  \n    foo");
    }

    fn with_config(text: &str, lua: &str) -> App {
        let dir = std::env::temp_dir().join(format!(
            "raster_app_cfg_{}_{}",
            std::process::id(),
            lua.len()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let init = dir.join("init.lua");
        std::fs::write(&init, lua).unwrap();
        let mut a = app(text);
        a.load_config(Some(&init));
        std::fs::remove_dir_all(dir).unwrap();
        a
    }

    #[test]
    fn init_lua_sets_options_and_maps() {
        let mut a = with_config(
            "a b c",
            r#"
            raster.g.mapleader = ","
            raster.opt.number = true
            raster.keymap.set("n", "<leader>d", "dw")
            raster.keymap.set("i", "jk", "<Esc>")
            "#,
        );
        assert!(a.options.borrow().number);
        assert_eq!(a.message, None);
        keys(&mut a, ",d");
        assert_eq!(text(&a), "b c");
        keys(&mut a, "ixjk");
        assert_eq!((text(&a).as_str(), a.mode), ("xb c", Mode::Normal));
        // Defaults now use the new leader too.
        a.buffer.dirty = false;
        keys(&mut a, ",q");
        assert!(a.should_quit);
    }

    #[test]
    fn lua_function_mappings_drive_the_editor() {
        let mut a = with_config(
            "one\ntwo",
            r#"
            raster.keymap.set("n", "<leader>x", function()
              raster.feedkeys("dd")
              raster.notify("deleted")
            end)
            raster.keymap.set("n", "Q", function() raster.cmd("q!") end)
            raster.keymap.set("n", "E", function() error("nope") end)
            "#,
        );
        keys(&mut a, " x");
        assert_eq!(text(&a), "two");
        assert_eq!(a.message.as_deref(), Some("deleted"));
        keys(&mut a, "E");
        assert_eq!(a.message.as_deref(), Some("init.lua:7: nope"));
        keys(&mut a, "Q");
        assert!(a.should_quit);
    }

    #[test]
    fn init_lua_errors_show_but_keep_earlier_settings() {
        let a = with_config(
            "",
            "raster.opt.number = true\nraster.opt.mouse = 'a'\nraster.opt.scrolloff = 5",
        );
        assert_eq!(
            a.message.as_deref(),
            Some("init.lua:2: unknown option 'mouse'")
        );
        assert!(a.options.borrow().number);
        assert_eq!(a.options.borrow().scrolloff, 0);
    }

    #[test]
    fn init_lua_can_restore_vim_keys() {
        let mut a = with_config(
            "x,y\nz",
            r#"
            for _, k in ipairs({ "t", "n", "s" }) do raster.keymap.del("n", k) end
            "#,
        );
        keys(&mut a, "t,");
        assert_eq!(a.col, 0);
        keys(&mut a, "j");
        assert_eq!(a.row, 1);
        keys(&mut a, "ks");
        assert_eq!(a.mode, Mode::Insert);
    }

    fn ex(a: &mut App, cmd: &str) {
        keys(a, &format!(":{cmd}"));
        press(a, KeyCode::Enter);
    }

    #[test]
    fn set_number_from_the_command_line_shows_line_numbers() {
        let mut a = app("one\ntwo");
        let (rows, ..) = render(&mut a, 12, 4);
        assert_eq!(rows[0], "one         ");

        ex(&mut a, "set nu");
        let (rows, x, _) = render(&mut a, 12, 4);
        assert_eq!(rows[0], "  1 one     ");
        assert_eq!(rows[1], "  2 two     ");
        assert_eq!(x, 4);

        ex(&mut a, "set nonumber");
        let (rows, ..) = render(&mut a, 12, 4);
        assert_eq!(rows[0], "one         ");

        ex(&mut a, "set number");
        assert!(a.options.borrow().number);
        ex(&mut a, "se nu?");
        assert_eq!(a.message.as_deref(), Some("  number"));
        ex(&mut a, "set nu!");
        assert!(!a.options.borrow().number);
    }

    #[test]
    fn set_errors_show_on_the_message_line() {
        let mut a = app("");
        ex(&mut a, "set mouse=a");
        assert_eq!(a.message.as_deref(), Some("E518: Unknown option: mouse=a"));
        ex(&mut a, "set ts=4 rnu");
        assert_eq!(a.options.borrow().tabstop, 4);
        assert!(a.options.borrow().relativenumber);
    }
}
