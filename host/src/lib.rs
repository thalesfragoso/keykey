#![cfg_attr(not(feature = "host"), no_std)]

pub mod key_code;
pub mod packets;

pub const VID: u16 = 0x1209;
pub const PID: u16 = 0x0010;
