use super::{BtnsType, NUM_BTS};
use core::convert::TryFrom;
use debouncer::typenum::consts::*;
use debouncer::{BtnState, PortDebouncer};
use heapless::spsc::Producer;
use keylib::{
    key_code::{
        valid_ranges::{ZONE1_FIRST, ZONE1_LAST, ZONE2_FIRST, ZONE2_LAST},
        KbHidReport, KeyCode,
    },
    packets::{AppCommand, DescriptorType, ReportType, Request, VendorCommand},
};
use usb_device::{
    bus::{InterfaceNumber, StringIndex, UsbBus, UsbBusAllocator},
    class::{ControlIn, ControlOut, UsbClass},
    control::{self, Recipient, RequestType},
    descriptor::DescriptorWriter,
    endpoint::{EndpointAddress, EndpointIn},
    UsbError,
};

#[rustfmt::skip]
const REPORT_DESCRIPTOR: &[u8] = &[
    0x05, 0x01,         // Usage Page (Generic Desktop Ctrls)
    0x09, 0x06,         // Usage (Keyboard)
    0xA1, 0x01,         // Collection (Application)
    0x05, 0x07,         //   Usage Page (Kbrd/Keypad)
    0x19, 0xE0,         //   Usage Minimum (0xE0)
    0x29, 0xE7,         //   Usage Maximum (0xE7)
    0x15, 0x00,         //   Logical Minimum (0)
    0x25, 0x01,         //   Logical Maximum (1)
    0x75, 0x01,         //   Report Size (1)
    0x95, 0x08,         //   Report Count (8)
    0x81, 0x02,         //   Input (Data,Var,Abs,No Wrap,Linear,Preferred State,No Null Position)
    0x95, 0x01,         //   Report Count (1)
    0x75, 0x08,         //   Report Size (8)
    0x81, 0x03,         //   Input (Const,Var,Abs,No Wrap,Linear,Preferred State,No Null Position)
    0x95, 0x06,         //   Report Count (6)
    0x75, 0x08,         //   Report Size (8)
    0x15, 0x00,         //   Logical Minimum (0)
    0x26, 0xFB, 0x00,   //   Logical Maximum (0xFB)
    0x05, 0x07,         //   Usage Page (Kbrd/Keypad)
    0x19, 0x00,         //   Usage Minimum (0x00)
    0x29, 0xFB,         //   Usage Maximum (0xFB)
    0x81, 0x00,         //   Input (Data,Array,Abs,No Wrap,Linear,Preferred State,No Null Position)
    0xC0,               // End Collection
];

const SPECIFICATION_RELEASE: u16 = 0x111;
const INTERFACE_CLASS_HID: u8 = 0x03;
const SUBCLASS_NONE: u8 = 0x00;
const KEYBOARD_PROTOCOL: u8 = 0x01;

pub struct Keykey<'a, 'b, B: UsbBus> {
    interface: InterfaceNumber,
    endpoint_interrupt_in: EndpointIn<'a, B>,
    expect_interrupt_in_complete: bool,
    report: KbHidReport,
    cmd_prod: Producer<'b, AppCommand, U8>,
}

impl<'a, 'b, B: UsbBus> Keykey<'a, 'b, B> {
    pub fn new(alloc: &'a UsbBusAllocator<B>, prod: Producer<'b, AppCommand, U8>) -> Self {
        Self {
            interface: alloc.interface(),
            endpoint_interrupt_in: alloc.interrupt(8, 10),
            expect_interrupt_in_complete: false,
            report: KbHidReport::default(),
            cmd_prod: prod,
        }
    }

    pub fn write(&mut self, data: &[u8]) -> Result<usize, ()> {
        if self.expect_interrupt_in_complete {
            return Ok(0);
        }

        if data.len() >= 8 {
            self.expect_interrupt_in_complete = true;
        }

        match self.endpoint_interrupt_in.write(data) {
            Ok(count) => Ok(count),
            Err(UsbError::WouldBlock) => Ok(0),
            Err(_) => Err(()),
        }
    }

    pub fn set_keyboard_report(&mut self, report: KbHidReport) -> bool {
        if report == self.report {
            false
        } else {
            self.report = report;
            true
        }
    }

    fn get_report(&mut self, xfer: ControlIn<B>) {
        let req = xfer.request();
        let [report_type, _report_id] = req.value.to_be_bytes();
        let report_type = ReportType::from(report_type);
        let response = self.report.as_bytes();
        match report_type {
            ReportType::Input if req.length >= response.len() as u16 => {
                xfer.accept_with(response).ok()
            }
            _ => xfer.reject().ok(),
        };
    }
}

impl<B: UsbBus> UsbClass<B> for Keykey<'_, '_, B> {
    fn poll(&mut self) {}

    fn reset(&mut self) {
        self.expect_interrupt_in_complete = false;
    }

    fn get_configuration_descriptors(
        &self,
        writer: &mut DescriptorWriter,
    ) -> usb_device::Result<()> {
        writer.interface(
            self.interface,
            INTERFACE_CLASS_HID,
            SUBCLASS_NONE,
            KEYBOARD_PROTOCOL,
        )?;

        let descriptor_len = REPORT_DESCRIPTOR.len();
        if descriptor_len > u16::max_value() as usize {
            return Err(UsbError::InvalidState);
        }
        let descriptor_len = (descriptor_len as u16).to_le_bytes();
        let specification_release = SPECIFICATION_RELEASE.to_le_bytes();
        writer.write(
            DescriptorType::Hid as u8,
            &[
                specification_release[0],     // bcdHID.lower
                specification_release[1],     // bcdHID.upper
                0,                            // bCountryCode: 0 = not supported
                1,                            // bNumDescriptors
                DescriptorType::Report as u8, // bDescriptorType
                descriptor_len[0],            // bDescriptorLength.lower
                descriptor_len[1],            // bDescriptorLength.upper
            ],
        )?;

        writer.endpoint(&self.endpoint_interrupt_in)?;

        Ok(())
    }

    fn get_string(&self, _index: StringIndex, _lang_id: u16) -> Option<&str> {
        None
    }

    fn endpoint_in_complete(&mut self, addr: EndpointAddress) {
        if addr == self.endpoint_interrupt_in.address() {
            self.expect_interrupt_in_complete = false;
        }
    }

    fn endpoint_out(&mut self, _addr: EndpointAddress) {}

    fn control_in(&mut self, xfer: ControlIn<B>) {
        let req = xfer.request();
        match (req.request_type, req.recipient) {
            (RequestType::Standard, Recipient::Interface) => {
                if req.request == control::Request::GET_DESCRIPTOR {
                    let (dtype, index) = req.descriptor_type_index();
                    if dtype == DescriptorType::Report as u8 && index == 0 {
                        let report_len = REPORT_DESCRIPTOR.len();
                        if report_len as u16 <= req.length {
                            xfer.accept_with(REPORT_DESCRIPTOR).ok();
                        }
                    }
                }
            }
            (RequestType::Class, Recipient::Interface) => {
                if let Some(request) = Request::new(req.request) {
                    if request == Request::GetReport {
                        self.get_report(xfer);
                    }
                }
            }
            _ => {}
        }
    }

    fn control_out(&mut self, xfer: ControlOut<B>) {
        let req = xfer.request();
        #[allow(clippy::single_match)]
        match (req.request_type, req.recipient) {
            (RequestType::Vendor, Recipient::Device) => {
                if let (Ok(cmd), Ok(key)) = (
                    VendorCommand::try_from(req.request),
                    KeyCode::try_from(req.value as u8),
                ) {
                    if self
                        .cmd_prod
                        .enqueue(AppCommand::from_req_value(cmd, key))
                        .is_ok()
                    {
                        xfer.accept().ok();
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Matrix {
    layout: [KeyCode; NUM_BTS],
}

impl Matrix {
    pub const fn new() -> Self {
        Self {
            layout: [KeyCode::A, KeyCode::B, KeyCode::C],
        }
    }

    pub fn update_layout(&mut self, command: AppCommand) {
        match command {
            AppCommand::Set1(value) => self.layout[0] = value,
            AppCommand::Set2(value) => self.layout[1] = value,
            AppCommand::Set3(value) => self.layout[2] = value,
        }
    }

    pub fn update(&self, debouncer: &mut PortDebouncer<U8, BtnsType>) -> KbHidReport {
        let mut report = KbHidReport::default();

        for (index, &btn) in self.layout.iter().enumerate() {
            let state = debouncer.get_state(index);
            if let Ok(value) = state {
                if value == BtnState::ChangedToPressed || value == BtnState::Repeat {
                    report.pressed(btn);
                }
            }
        }
        report
    }

    pub fn to_bytes(self) -> [u8; NUM_BTS] {
        // NOTE(unsafe) `self.layout` is `[KeyCode; NUM_BTS]` and `KeyCode` is `repr(u8)`
        unsafe { core::mem::transmute(self.layout) }
    }

    pub fn from_bytes(bytes: [u8; NUM_BTS]) -> Option<Self> {
        // Look for invalid codes
        #[allow(clippy::absurd_extreme_comparisons)]
        let invalid_code = bytes.iter().any(|&code| {
            // The first test will probably get optimized out when `ZONE1_FIRST` == 0, but we do it
            // anyway because that can change
            (code < ZONE1_FIRST) || (code > ZONE1_LAST && code < ZONE2_FIRST) || (code > ZONE2_LAST)
        });
        if invalid_code {
            None
        } else {
            // NOTE(unsafe) safe based on the check above
            unsafe {
                Some(Self {
                    layout: core::mem::transmute(bytes),
                })
            }
        }
    }
}
