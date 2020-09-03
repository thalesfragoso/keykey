use anyhow::Result;
use crossterm::event::{read, Event, KeyCode as TermKey, KeyEvent, KeyModifiers};

mod app;
use app::{App, State, Term};

fn main() -> Result<()> {
    let mut term = Term::new()?;
    let mut app = App::new()?;

    'outer: loop {
        if term.state == State::SelectScreen {
            term.render_menu_screen()?;
            match read()? {
                Event::Key(KeyEvent {
                    code: TermKey::Char('q'),
                    modifiers: KeyModifiers::CONTROL,
                }) => break 'outer,
                Event::Key(KeyEvent {
                    code: TermKey::Char(c),
                    ..
                }) => match c {
                    '1' => term.state = State::Set1,
                    '2' => term.state = State::Set2,
                    '3' => term.state = State::Set3,
                    _ => {}
                },
                _ => {}
            }
        } else {
            'inner: loop {
                app.render(&mut term)?;
                match read()? {
                    Event::Key(KeyEvent {
                        code: TermKey::Char('q'),
                        modifiers: KeyModifiers::CONTROL,
                    }) => break 'outer,
                    Event::Key(KeyEvent {
                        code: TermKey::Esc, ..
                    }) => {
                        term.state = State::SelectScreen;
                        app.clear();
                        break 'inner;
                    }
                    Event::Key(KeyEvent {
                        code: TermKey::Enter,
                        ..
                    }) => {
                        app.send_selected(term.state.to_vendor_command()?)?;
                        term.state = State::SelectScreen;
                        app.clear();
                        break 'inner;
                    }
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
            }
        }
    }
    Ok(())
}
