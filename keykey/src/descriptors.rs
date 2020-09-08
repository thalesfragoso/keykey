use core::mem::size_of;
use num_enum::TryFromPrimitive;

/// WinUSB compatible string descriptor, last byte will be the vendor request used to get the OS
/// feature descriptors.
pub const STRING_MOS: &str = "MSFT100F";
const MS_COMPATIBLE_ID_WINUSB: [u8; 8] = [b'W', b'I', b'N', b'U', b'S', b'B', 0, 0];

#[rustfmt::skip]
pub const REPORT_DESCRIPTOR: &[u8] = &[
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

pub const MS_COMPATIBLE_ID_DESCRIPTOR: MSCompatibleIDDescriptor = MSCompatibleIDDescriptor {
    dwLength: size_of::<MSCompatibleIDDescriptor>() as u32,
    bcdVersion: 0x0100,
    wIndex: OSFeatureDescriptorType::CompatibleID as u16,
    bNumSections: 1,
    _rsvd0: [0; 7],
    features: [MSCompatibleIDDescriptorFunction {
        bInterfaceNumber: 0,
        _rsvd0: 0,
        sCompatibleID: MS_COMPATIBLE_ID_WINUSB,
        sSubCompatibleID: [0u8; 8],
        _rsvd1: [0u8; 6],
    }],
};

pub const IF0_MS_PROPERTIES_OS_DESCRIPTOR: MSPropertiesOSDescriptor = MSPropertiesOSDescriptor {
    bcdVersion: 0x0100,
    wIndex: OSFeatureDescriptorType::Properties as u16,
    wCount: 1,
    features: [MSPropertiesOSDescriptorFeature {
        dwPropertyDataType: MSPropertyDataType::REG_SZ as u32,
        bPropertyName: "DeviceInterfaceGUID\x00",
        bPropertyData: "{183BE48C-1C39-4612-92EB-650C4450C1D3}\x00",
    }],
};

// Adapted from Adam's ffp project

#[allow(non_snake_case)]
#[repr(C)]
#[repr(packed)]
pub struct MSCompatibleIDDescriptor {
    pub dwLength: u32,
    pub bcdVersion: u16,
    pub wIndex: u16,
    pub bNumSections: u8,
    pub _rsvd0: [u8; 7],
    pub features: [MSCompatibleIDDescriptorFunction; 1],
}

#[allow(non_snake_case)]
#[repr(C)]
#[repr(packed)]
pub struct MSCompatibleIDDescriptorFunction {
    pub bInterfaceNumber: u8,
    pub _rsvd0: u8,
    pub sCompatibleID: [u8; 8],
    pub sSubCompatibleID: [u8; 8],
    pub _rsvd1: [u8; 6],
}

#[allow(non_snake_case)]
pub struct MSPropertiesOSDescriptor {
    pub bcdVersion: u16,
    pub wIndex: u16,
    pub wCount: u16,
    pub features: [MSPropertiesOSDescriptorFeature; 1],
}

#[allow(non_snake_case)]
pub struct MSPropertiesOSDescriptorFeature {
    pub dwPropertyDataType: u32,
    pub bPropertyName: &'static str,
    pub bPropertyData: &'static str,
}

#[allow(non_snake_case)]
#[repr(u16)]
#[derive(TryFromPrimitive)]
pub enum OSFeatureDescriptorType {
    CompatibleID = 4,
    Properties = 5,
}

#[allow(non_camel_case_types)]
#[allow(unused)]
#[repr(u32)]
pub enum MSPropertyDataType {
    REG_SZ = 1,
    REG_EXPAND_SZ = 2,
    REG_BINARY = 3,
    REG_DWORD_LITTLE_ENDIAN = 4,
    REG_DWORD_BIG_ENDIAN = 5,
    REG_LINK = 6,
    REG_MULTI_SZ = 7,
}

impl MSCompatibleIDDescriptor {
    pub fn to_bytes(&self) -> &[u8] {
        // NOTE(unsafe) We return a non-mutable slice into this packed struct's memory at the length
        // of the struct, with a lifetime bound to &self
        unsafe {
            core::slice::from_raw_parts(self as *const _ as *const u8, core::mem::size_of::<Self>())
        }
    }
    pub fn len(&self) -> usize {
        self.dwLength as usize
    }
}

impl MSPropertiesOSDescriptor {
    /// Retrieve the total length of a MSPropertiesOSDescriptor,
    /// including the length of variable string contents once UTF-16 encoded.
    pub fn len(&self) -> usize {
        // Header section
        let mut len = 10;

        for feature in self.features.iter() {
            len += feature.len();
        }

        len
    }

    /// Write descriptor contents into a provided &mut [u8], which must
    /// be at least self.len() long.
    pub fn write_to_buf(&self, buf: &mut [u8]) {
        let len = self.len() as u32;
        buf[0..4].copy_from_slice(&len.to_le_bytes());
        buf[4..6].copy_from_slice(&self.bcdVersion.to_le_bytes());
        buf[6..8].copy_from_slice(&self.wIndex.to_le_bytes());
        buf[8..10].copy_from_slice(&self.wCount.to_le_bytes());
        let mut i = 10;

        for feature in self.features.iter() {
            feature.write_to_buf(&mut buf[i..]);
            i += feature.len();
        }
    }
}

impl MSPropertiesOSDescriptorFeature {
    pub fn len(&self) -> usize {
        // Fixed length parts of feature
        let mut len = 14;

        // String parts
        len += self.name_len();
        len += self.data_len();

        len
    }

    fn name_len(&self) -> usize {
        self.bPropertyName.encode_utf16().count() * 2
    }

    fn data_len(&self) -> usize {
        self.bPropertyData.encode_utf16().count() * 2
    }

    pub fn write_to_buf(&self, buf: &mut [u8]) {
        let len = self.len() as u32;
        let name_len = self.name_len() as u16;
        let data_len = self.data_len() as u32;
        buf[0..4].copy_from_slice(&len.to_le_bytes());
        buf[4..8].copy_from_slice(&self.dwPropertyDataType.to_le_bytes());
        buf[8..10].copy_from_slice(&name_len.to_le_bytes());
        let mut i = 10;
        for cp in self.bPropertyName.encode_utf16() {
            let [u1, u2] = cp.to_le_bytes();
            buf[i] = u1;
            buf[i + 1] = u2;
            i += 2;
        }
        buf[i..i + 4].copy_from_slice(&data_len.to_le_bytes());
        i += 4;
        for cp in self.bPropertyData.encode_utf16() {
            let [u1, u2] = cp.to_le_bytes();
            buf[i] = u1;
            buf[i + 1] = u2;
            i += 2;
        }
    }
}
