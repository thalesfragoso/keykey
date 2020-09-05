use crate::key_code::KeyCode;
use num_enum::TryFromPrimitive;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DescriptorType {
    Hid = 0x21,
    Report = 0x22,
    _Physical = 0x23,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum Request {
    GetReport = 0x01,
    GetIdle = 0x02,
    GetProtocol = 0x03,
    SetReport = 0x09,
    SetIdle = 0x0a,
    SetProtocol = 0x0b,
}
impl Request {
    pub fn new(u: u8) -> Option<Request> {
        use Request::*;
        match u {
            0x01 => Some(GetReport),
            0x02 => Some(GetIdle),
            0x03 => Some(GetProtocol),
            0x09 => Some(SetReport),
            0x0a => Some(SetIdle),
            0x0b => Some(SetProtocol),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReportType {
    Input,
    Output,
    Feature,
    Reserved(u8),
}

impl From<u8> for ReportType {
    fn from(val: u8) -> Self {
        match val {
            1 => ReportType::Input,
            2 => ReportType::Output,
            3 => ReportType::Feature,
            _ => ReportType::Reserved(val),
        }
    }
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum VendorCommand {
    Set1 = 1,
    Set2,
    Set3,
    Save,
}

#[derive(Debug, Copy, Clone)]
pub enum AppCommand {
    Set1(KeyCode),
    Set2(KeyCode),
    Set3(KeyCode),
    Save,
}

impl AppCommand {
    pub fn from_req_value(req: VendorCommand, value: KeyCode) -> Self {
        match req {
            VendorCommand::Set1 => AppCommand::Set1(value),
            VendorCommand::Set2 => AppCommand::Set2(value),
            VendorCommand::Set3 => AppCommand::Set3(value),
            VendorCommand::Save => AppCommand::Save,
        }
    }
}
