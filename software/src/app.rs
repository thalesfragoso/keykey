use super::GREETING;
use crossterm::{
    cursor, queue,
    style::{self, Colorize},
    terminal::{self, ClearType},
    Result as TermResult,
};
use keylib::key_code::KeyCode;
use std::{convert::AsRef, fmt, io::Write};
use strum::IntoEnumIterator;

pub struct App {
    current_line: usize,
    user_input: String,
    hits: Vec<KeyCode>,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            current_line: 0,
            user_input: String::with_capacity(16),
            hits: Vec::with_capacity(16),
        };
        app.search_all();
        app
    }

    pub fn push_char_hit(&mut self, mut new: char) {
        if !new.is_ascii_alphanumeric() {
            return;
        }
        new.make_ascii_lowercase();
        self.user_input.push(new);

        let input = self.user_input.as_str();
        let new_hits = self
            .hits
            .iter()
            .filter(|&k| k.as_ref().starts_with(input))
            .map(|k| *k)
            .collect();
        self.hits = new_hits;
        if self.current_line + 1 > self.hits.len() {
            self.current_line = self.hits.len().saturating_sub(1);
        }
    }

    pub fn backspace(&mut self) {
        if let Some(_) = self.user_input.pop() {
            self.search_all();
        }
    }

    pub fn up(&mut self) {
        self.current_line = self.current_line.saturating_sub(1);
    }

    pub fn down(&mut self) {
        if self.current_line + 1 < self.hits.len() {
            self.current_line += 1;
        }
    }

    pub fn render(&self, w: &mut impl Write) -> TermResult<()> {
        queue!(
            w,
            style::ResetColor,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 1),
        )?;
        for (index, &key) in self.hits.iter().enumerate() {
            let mut text = String::new();
            fmt::write(&mut text, format_args!("{:?}", key))?;
            if index == self.current_line {
                queue!(w, style::Print(text.black().on_yellow()))?;
            } else {
                queue!(w, style::Print(text))?;
            }
            queue!(w, cursor::MoveToNextLine(1))?;
        }
        queue!(
            w,
            cursor::MoveTo(0, 0),
            style::Print(GREETING),
            style::Print(&self.user_input),
        )?;
        w.flush()?;
        Ok(())
    }

    fn search_all(&mut self) {
        self.hits.clear();
        let input = self.user_input.as_str();
        for code in KeyCode::iter().filter(|k| k.as_ref().starts_with(input)) {
            self.hits.push(code);
        }
        if self.current_line + 1 > self.hits.len() {
            self.current_line = self.hits.len().saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init() {
        let mut app = App::new();
        app.push_char_hit('a');
        assert_eq!(
            app.hits,
            &[
                KeyCode::A,
                KeyCode::Application,
                KeyCode::Again,
                KeyCode::AltErase
            ]
        );
        app.push_char_hit('P');
        assert_eq!(app.hits, &[KeyCode::Application]);

        app.backspace();
        assert_eq!(
            app.hits,
            &[
                KeyCode::A,
                KeyCode::Application,
                KeyCode::Again,
                KeyCode::AltErase
            ]
        );
    }
}
