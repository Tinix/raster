//! App: editor state, the event loop, key handling and drawing.

use crate::{
    buffer::Buffer,
    command::Command,
    mode::Mode,
    ui::{self, Status},
};
use anyhow::Result;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint, Layout, Position},
    text::Line,
    widgets::Paragraph,
};

/// Leader key, as in the user's nvim config (`vim.g.mapleader = " "`).
pub const LEADER: char = ' ';

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
    /// The leader key was pressed and we wait for the next key.
    leader_pending: bool,
    pub should_quit: bool,
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
            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                self.handle_key(key);
            }
        }
        Ok(())
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Normal => self.normal_key(key),
            Mode::Insert => self.insert_key(key),
            Mode::Command => self.command_key(key),
        }
    }

    // --- Normal ------------------------------------------------------------

    fn normal_key(&mut self, key: KeyEvent) {
        if std::mem::take(&mut self.leader_pending) {
            match key.code {
                KeyCode::Char('q') => self.execute(Command::Quit),
                KeyCode::Char('Q') => self.execute(Command::ForceQuit),
                _ => {}
            }
            return;
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('s') if ctrl => self.execute(Command::Write),
            KeyCode::Char(LEADER) => self.leader_pending = true,
            // Dvorak motions: h left, t down, n up, s right.
            KeyCode::Char('h') | KeyCode::Left => self.move_left(),
            KeyCode::Char('t') | KeyCode::Down => self.move_down(),
            KeyCode::Char('n') | KeyCode::Up => self.move_up(),
            KeyCode::Char('s') | KeyCode::Right => self.move_right(),
            KeyCode::Char('0') | KeyCode::Home => self.line_start(),
            KeyCode::Char('$') | KeyCode::End => self.line_end(),
            KeyCode::Char('i') => self.mode = Mode::Insert,
            KeyCode::Char('a') => {
                self.mode = Mode::Insert;
                self.col = (self.col + 1).min(self.buffer.line_len(self.row));
                self.want_col = self.col;
            }
            KeyCode::Char(':') => {
                self.mode = Mode::Command;
                self.cmdline.clear();
                self.message = None;
            }
            _ => {}
        }
    }

    // --- Insert ------------------------------------------------------------

    fn insert_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                // Like Vim, leaving Insert steps back onto the last typed char.
                self.col = self.col.saturating_sub(1);
                self.clamp_col();
                self.want_col = self.col;
            }
            KeyCode::Char(c) => {
                self.buffer.insert_char(self.row, self.col, c);
                self.col += 1;
                self.want_col = self.col;
            }
            KeyCode::Enter => {
                self.buffer.insert_char(self.row, self.col, '\n');
                self.row += 1;
                self.col = 0;
                self.want_col = 0;
            }
            KeyCode::Backspace => {
                (self.row, self.col) = self.buffer.backspace(self.row, self.col);
                self.want_col = self.col;
            }
            KeyCode::Left => self.move_left(),
            KeyCode::Down => self.move_down(),
            KeyCode::Up => self.move_up(),
            KeyCode::Right => self.move_right(),
            KeyCode::Home => self.line_start(),
            KeyCode::End => self.line_end(),
            _ => {}
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

    fn move_left(&mut self) {
        self.col = self.col.saturating_sub(1);
        self.want_col = self.col;
    }

    fn move_right(&mut self) {
        self.col = (self.col + 1).min(self.max_col());
        self.want_col = self.col;
    }

    fn move_down(&mut self) {
        if self.row + 1 < self.buffer.line_count() {
            self.row += 1;
            self.col = self.want_col.min(self.max_col());
        }
    }

    fn move_up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.col = self.want_col.min(self.max_col());
        }
    }

    fn line_start(&mut self) {
        self.col = 0;
        self.want_col = 0;
    }

    fn line_end(&mut self) {
        self.col = self.max_col();
        self.want_col = usize::MAX;
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
    fn scroll_to_cursor(&mut self, height: usize) {
        if self.row < self.scroll {
            self.scroll = self.row;
        } else if height > 0 && self.row >= self.scroll + height {
            self.scroll = self.row + 1 - height;
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let [text_area, status_area, cmd_area] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(frame.area());
        self.scroll_to_cursor(text_area.height as usize);

        let lines: Vec<Line> = (self.scroll..self.buffer.line_count())
            .take(text_area.height as usize)
            .map(|r| {
                let line = self.buffer.text.line(r).to_string();
                Line::raw(line.trim_end_matches(['\n', '\r']).to_owned())
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), text_area);

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
            let x = text_area.x + self.col as u16;
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
}
