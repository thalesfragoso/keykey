use crossterm::{
    cursor,
    event::{read, Event, KeyCode as TermKey, KeyEvent, KeyModifiers},
    execute, style,
    terminal::{self, disable_raw_mode, enable_raw_mode},
    Result as TermResult,
};
use std::io::{stdout, Write};

mod app;
use app::App;

const GREETING: &'static str = "Key: ";

fn main() -> TermResult<()> {
    let mut w = stdout();
    let mut app = App::new();

    execute!(w, terminal::EnterAlternateScreen)?;
    enable_raw_mode()?;

    app.render(&mut w)?;
    loop {
        match read()? {
            Event::Key(KeyEvent {
                code: TermKey::Char('q'),
                modifiers: KeyModifiers::CONTROL,
            }) => break,
            Event::Key(KeyEvent {
                code: TermKey::Char(c),
                ..
            }) => app.push_char_hit(c),
            Event::Key(KeyEvent {
                code: TermKey::Backspace,
                ..
            }) => app.backspace(),
            Event::Key(KeyEvent {
                code: TermKey::Up, ..
            }) => app.up(),
            Event::Key(KeyEvent {
                code: TermKey::Down,
                ..
            }) => app.down(),
            _ => {}
        }
        app.render(&mut w)?;
    }

    execute!(
        w,
        style::ResetColor,
        cursor::Show,
        terminal::LeaveAlternateScreen
    )?;
    disable_raw_mode()?;
    Ok(())
}
