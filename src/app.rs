//! App: editor state, the event loop and drawing.

use crate::{buffer::Buffer, mode::Mode};
use anyhow::Result;
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind},
    layout::Position,
    text::Line,
    widgets::Paragraph,
};

/// The whole editor: one buffer, a cursor and the current mode.
#[derive(Debug, Default)]
pub struct App {
    pub buffer: Buffer,
    pub mode: Mode,
    /// Cursor row (line index).
    pub row: usize,
    /// Cursor column (char index within the line).
    pub col: usize,
    /// First visible line.
    pub scroll: usize,
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
        if key.code == KeyCode::Esc {
            self.should_quit = true;
        }
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
        let area = frame.area();
        self.scroll_to_cursor(area.height as usize);

        let lines: Vec<Line> = (self.scroll..self.buffer.line_count())
            .take(area.height as usize)
            .map(|r| {
                Line::raw(
                    self.buffer
                        .text
                        .line(r)
                        .to_string()
                        .trim_end_matches(['\n', '\r'])
                        .to_owned(),
                )
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), area);

        let x = area.x + self.col as u16;
        let y = area.y + (self.row - self.scroll) as u16;
        frame.set_cursor_position(Position::new(x, y));
    }
}
