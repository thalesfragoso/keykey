use crate::{
    descriptors::{
        OSFeatureDescriptorType, IF0_MS_PROPERTIES_OS_DESCRIPTOR, MS_COMPATIBLE_ID_DESCRIPTOR,
        REPORT_DESCRIPTOR, STRING_MOS,
    },
    flash::{ConfigWriter, FlashError},
    BtnsType, NUM_BTS,
};
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
        let keyboard = Self {
            interface: alloc.interface(),
            endpoint_interrupt_in: alloc.interrupt(8, 10),
            expect_interrupt_in_complete: false,
            report: KbHidReport::default(),
            cmd_prod: prod,
        };
        // We use the interface 0 as WinUSB compatible, we could change the descriptor at runtime,
        // but it seems wasteful, since this is gonna be true because that is our only interface
        assert!(u8::from(keyboard.interface) == 0);
        keyboard
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

    fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&str> {
        if u8::from(index) == 0xEE {
            Some(STRING_MOS)
        } else {
            None
        }
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
            (RequestType::Vendor, Recipient::Device)
            | (RequestType::Vendor, Recipient::Interface) => {
                log!(
                    "Vendor request: {:?}, Recipient: {:?}, Index: {:?}",
                    req.request,
                    req.recipient,
                    req.index
                );
                if let Ok(VendorCommand::GetOSFeature) = VendorCommand::try_from(req.request) {
                    match OSFeatureDescriptorType::try_from(req.index) {
                        Ok(OSFeatureDescriptorType::CompatibleID) => {
                            log!("Sending Compatible ID Descriptor");
                            let desc = &MS_COMPATIBLE_ID_DESCRIPTOR;
                            let max_len = req.length.min(desc.len() as u16) as usize;
                            let data = desc.to_bytes();
                            xfer.accept_with_static(&data[..max_len]).ok();
                        }
                        Ok(OSFeatureDescriptorType::Properties) => {
                            if req.value == u8::from(self.interface) as u16 {
                                log!("Sending Properties OS Descriptor");
                                let desc = &IF0_MS_PROPERTIES_OS_DESCRIPTOR;
                                let max_len = req.length.min(desc.len() as u16) as usize;
                                let mut data = [0u8; 192];
                                desc.write_to_buf(&mut data[..]);
                                xfer.accept_with(&data[..max_len]).ok();
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn control_out(&mut self, xfer: ControlOut<B>) {
        let req = xfer.request();
        if req.request_type == RequestType::Vendor && req.recipient == Recipient::Device {
            if let (Ok(cmd), Ok(key)) = (
                VendorCommand::try_from(req.request),
                KeyCode::try_from(req.value as u8),
            ) {
                if let Some(app_command) = AppCommand::from_req_value(cmd, key) {
                    if self.cmd_prod.enqueue(app_command).is_ok() {
                        xfer.accept().ok();
                    }
                }
            }
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

    pub fn update_layout(
        &mut self,
        command: AppCommand,
        writer: &mut ConfigWriter,
    ) -> Result<(), FlashError> {
        match command {
            AppCommand::Set1(value) => self.layout[0] = value,
            AppCommand::Set2(value) => self.layout[1] = value,
            AppCommand::Set3(value) => self.layout[2] = value,
            AppCommand::Save => writer.write_config(*self)?,
        };
        Ok(())
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
