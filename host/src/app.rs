use anyhow::{anyhow, Context, Result};
use crossterm::{
    cursor, execute, queue,
    style::{self, Colorize},
    terminal::{self, disable_raw_mode, enable_raw_mode, ClearType},
};
use keylib::packets::VendorCommand;
use keylib::{key_code::KeyCode, PID, VID};
use rusb::{
    request_type, Context as RusbContext, DeviceHandle, Direction, Recipient, RequestType,
    UsbContext,
};
use std::{
    convert::AsRef,
    fmt,
    io::{self, stdout, Stdout, Write},
    time::Duration,
};
use strum::IntoEnumIterator;

const KEY_INPUT_LABEL: &'static str = "Key: ";
const SELECT_MENU: &str = r#"Keykey configuration tool

Controls:
 - 'ctrl + q' - quit
 - 'esc' - return to this menu
 - 'enter' - select key

Options:
1. Config button 1
2. Config button 2
3. Config button 3
s. Save current configuration to device flash
"#;

pub struct App {
    current_line: usize,
    user_input: String,
    hits: Vec<KeyCode>,
    usb_handle: DeviceHandle<RusbContext>,
    request_type: u8,
}

impl App {
    pub fn new() -> Result<Self> {
        let context = RusbContext::new().with_context(|| "Failed to create libusb context")?;
        let usb_handle = context
            .open_device_with_vid_pid(VID, PID)
            .ok_or_else(|| anyhow!("Unable to open device, VID: {:#X}, PID: {:#X}", VID, PID))?;

        let request_type = request_type(Direction::Out, RequestType::Vendor, Recipient::Device);
        let mut app = Self {
            current_line: 0,
            user_input: String::with_capacity(16),
            hits: Vec::with_capacity(16),
            usb_handle,
            request_type,
        };
        app.search_all();
        Ok(app)
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

    pub fn clear(&mut self) {
        self.user_input.clear();
        self.search_all();
    }

    pub fn render(&self, w: &mut impl Write) -> Result<()> {
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
            style::Print(KEY_INPUT_LABEL),
            style::Print(&self.user_input),
        )?;
        w.flush()?;
        Ok(())
    }

    pub fn send_selected(&mut self, command: VendorCommand) -> Result<()> {
        let key = self
            .hits
            .get(self.current_line)
            .ok_or_else(|| anyhow!("Internal Error: Could not find selected key"))?;

        let timeout = Duration::from_secs(1);
        self.usb_handle
            .write_control(
                self.request_type,
                command as u8,
                *key as u8 as u16,
                0,
                &[],
                timeout,
            )
            .map(|_| ())
            .with_context(|| "Failed to send control transfer.")
    }

    pub fn save_config(&mut self) -> Result<()> {
        let command = VendorCommand::Save;
        let timeout = Duration::from_secs(1);
        self.usb_handle
            .write_control(self.request_type, command as u8, 0, 0, &[], timeout)
            .map(|_| ())
            .with_context(|| "Failed to send control transfer.")
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

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum State {
    SelectScreen,
    Set1,
    Set2,
    Set3,
}

impl State {
    pub fn to_vendor_command(self) -> Result<VendorCommand> {
        match self {
            State::Set1 => Ok(VendorCommand::Set1),
            State::Set2 => Ok(VendorCommand::Set2),
            State::Set3 => Ok(VendorCommand::Set3),
            _ => Err(anyhow!("Internal Error: Invalid Vendor command.")),
        }
    }
}

pub struct Term {
    w: Stdout,
    pub state: State,
}

impl Term {
    pub fn new() -> Result<Self> {
        let mut term = Self {
            w: stdout(),
            state: State::SelectScreen,
        };
        execute!(&mut term, terminal::EnterAlternateScreen)?;
        enable_raw_mode()?;
        Ok(term)
    }
    pub fn render_menu_screen(&mut self, config_saved: bool) -> Result<()> {
        queue!(
            self,
            style::ResetColor,
            terminal::Clear(ClearType::All),
            cursor::Hide,
            cursor::MoveTo(0, 0)
        )?;

        for line in SELECT_MENU.split('\n') {
            queue!(self, style::Print(line), cursor::MoveToNextLine(1))?;
        }
        if config_saved {
            queue!(
                self,
                cursor::MoveToNextLine(1),
                style::Print("Configuration saved"),
            )?;
        }
        self.flush()?;
        Ok(())
    }
}

impl Write for Term {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.w.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.w.flush()
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        execute!(
            self,
            style::ResetColor,
            cursor::Show,
            terminal::LeaveAlternateScreen
        )
        .ok();
        disable_raw_mode().ok();
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
