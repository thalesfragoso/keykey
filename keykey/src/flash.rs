//! Flash writing abstraction for storing configurations.
//!
//! Each configuration will have a magic byte to mark it as valid and will occupy (in bytes):
//! ```
//! ((NUM_BTS + 1) + 1) & !1
//! ```
//!
//! The `+ 1 & !1` is used to have a multiple of 2 bytes, this is done for convenience when dealing
//! with the flash, because it can only be written 2 bytes at a time.
//!
//! The last page of the device flash is used to store the configuration, they are written one after
//! the other in flash, the last valid configuration is the used one, this is used to avoid flash
//! wear. When the page gets full, the whole page is erased and the desired configuration is saved
//! at the start of the page.

// Remove this later
#![allow(dead_code)]

use super::{Matrix, NUM_BTS};
use core::{ptr, slice};
use static_assertions::const_assert;
use stm32f1xx_hal::{
    flash::Parts,
    pac::{self, FLASH},
};

const FLASH_START: usize = 0x0800_0000;
const PAGE_SIZE: usize = 1024;
const FLASH_SIZE_KB: usize = 64;
const FLASH_END: usize = FLASH_START + FLASH_SIZE_KB * PAGE_SIZE;

/// We will use the last flash page for storing the configuration.
const CONFIG_ADD: usize = FLASH_START + (FLASH_SIZE_KB - 1) * PAGE_SIZE;
// Magic byte to mark a valid config
const MAGIC: u8 = 0x55;

const CONFIG_SIZE: usize = ((NUM_BTS + 1) + 1) & !1;
// How many configs we can fit on one page
const CONFIGS_IN_PAGE: usize = PAGE_SIZE / CONFIG_SIZE;
const_assert!(CONFIGS_IN_PAGE > 0);

const KEY1: u32 = 0x45670123;
const KEY2: u32 = 0xCDEF89AB;

#[derive(Debug)]
pub enum FlashError {
    /// Error during unlocking, this also means that we will not be able to unlock the flash again
    /// until the next reset.
    UnlockError,
    VerificationError,
    EraseError,
    WrongRange,
    ProgrammingError,
    FlashNotErased,
}

pub struct ConfigWriter {
    // Guarantee for the ownership of the registers, zero sized
    _parts: Parts,
    last_valid_index: usize,
}

impl ConfigWriter {
    pub fn new(_parts: Parts) -> Result<Self, FlashError> {
        let mut writer = Self {
            _parts,
            last_valid_index: 0,
        };

        // Do we need to erase the whole thing ?
        if unsafe { ptr::read_volatile(CONFIG_ADD as *const u8) } != MAGIC {
            log!("No saved config found, creating default one");
            writer.write_default()?;
            Ok(writer)
        } else {
            // Look for the last valid index, zero index already checked
            for current_idx in 1..CONFIGS_IN_PAGE {
                let current_addr = CONFIG_ADD + current_idx * CONFIG_SIZE;
                let value = unsafe { ptr::read_volatile(current_addr as *const u8) };
                if value == MAGIC {
                    writer.last_valid_index += 1;
                } else {
                    break;
                }
            }
            Ok(writer)
        }
    }

    /// Writes a default configuration to the start of the config page.
    pub fn write_default(&mut self) -> Result<(), FlashError> {
        self.erase_page()?;
        let mut config = [0u8; CONFIG_SIZE];
        Self::matrix_to_config(Matrix::new(), &mut config);

        self.write(CONFIG_ADD, &config[..])?;
        self.last_valid_index = 0;
        Ok(())
    }

    pub fn get_config(&self) -> Option<Matrix> {
        let last_addr = CONFIG_ADD + self.last_valid_index * CONFIG_SIZE;
        let config = self.read(last_addr + 1, CONFIG_SIZE - 1).ok()?;
        // Remove possible padding byte
        let mut data = [0u8; NUM_BTS];
        data.copy_from_slice(&config[..NUM_BTS]);
        if let Some(matrix) = Matrix::from_bytes(data) {
            Some(matrix)
        } else {
            None
        }
    }

    /// Tries to write a config to the next flash index, if the current index is the last one, this
    /// method will erase the whole page and write to the first place. It will fail if the next
    /// place to write is not already erased.
    pub fn write_config(&mut self, matrix: Matrix) -> Result<(), FlashError> {
        let mut config = [0u8; CONFIG_SIZE];
        Self::matrix_to_config(matrix, &mut config);

        if self.last_valid_index + 1 < CONFIGS_IN_PAGE {
            let next_addr = CONFIG_ADD + (self.last_valid_index + 1) * CONFIG_SIZE;
            let value = unsafe { ptr::read_volatile(next_addr as *const u8) };
            if value != 0xFF {
                log!("Found no erased flash while attempting write");
                return Err(FlashError::FlashNotErased);
            }
            self.write(next_addr, &config[..])?;
            self.last_valid_index += 1;
        } else {
            // No more space in the page, erase and go back to the start
            log!("Got to the end of page, going back to start");
            self.erase_page()?;
            self.write(CONFIG_ADD, &config[..])?;
            self.last_valid_index = 0;
        }
        Ok(())
    }

    fn matrix_to_config(matrix: Matrix, config: &mut [u8; CONFIG_SIZE]) {
        let bytes = matrix.to_bytes();
        config[0] = MAGIC;
        config[1..=NUM_BTS].copy_from_slice(&bytes[..]);
    }

    fn erase_page(&mut self) -> Result<(), FlashError> {
        self.unlock()?;
        self.flash().cr.modify(|_, w| w.per().set_bit());

        // NOTE(unsafe) valid address to write to far
        self.flash()
            .ar
            .write(|w| unsafe { w.far().bits(CONFIG_ADD as u32) });

        // Start Operation
        self.flash().cr.modify(|_, w| w.strt().set_bit());

        // Wait for operation to finish
        while self.flash().sr.read().bsy().bit_is_set() {}

        // Check for errors
        let sr = self.flash().sr.read();
        self.flash().cr.modify(|_, w| w.per().clear_bit());

        // Re-lock flash
        self.lock();

        if sr.wrprterr().bit_is_set() {
            self.flash().sr.modify(|_, w| w.wrprterr().clear_bit());
            Err(FlashError::EraseError)
        } else {
            // Verifying
            for address in CONFIG_ADD..CONFIG_ADD + PAGE_SIZE {
                // NOTE(unsafe) This is a valid address to read from
                let verify = unsafe { ptr::read_volatile(address as *const u16) };
                if verify != 0xFFFF {
                    log!("Verification error during erasing");
                    return Err(FlashError::VerificationError);
                }
            }
            Ok(())
        }
    }

    /// Helper method to give us access to the registers.
    #[inline(always)]
    fn flash(&self) -> &pac::flash::RegisterBlock {
        // NOTE(unsafe) We own the registers through the Parts' abstraction
        unsafe { &*FLASH::ptr() }
    }

    fn unlock(&mut self) -> Result<(), FlashError> {
        // Wait for ongoing operations
        while self.flash().sr.read().bsy().bit_is_set() {}

        // NOTE(unsafe)
        unsafe {
            self.flash().keyr.write(|w| w.key().bits(KEY1));
            self.flash().keyr.write(|w| w.key().bits(KEY2));
        }

        if self.flash().cr.read().lock().bit_is_clear() {
            Ok(())
        } else {
            log!("Flash unlocking error");
            Err(FlashError::UnlockError)
        }
    }

    fn lock(&mut self) {
        //Wait for ongoing operations
        while self.flash().sr.read().bsy().bit_is_set() {}

        self.flash().cr.modify(|_, w| w.lock().set_bit());
    }

    fn read(&self, start: usize, length: usize) -> Result<&[u8], FlashError> {
        if Self::valid_range(start, length) {
            // NOTE(unsafe) Valid range, as per test above.
            unsafe { Ok(slice::from_raw_parts(start as *const u8, length)) }
        } else {
            Err(FlashError::WrongRange)
        }
    }

    fn write(&mut self, start: usize, data: &[u8]) -> Result<(), FlashError> {
        if !Self::valid_range(start, data.len()) || data.len() & 1 != 0 {
            return Err(FlashError::WrongRange);
        }
        self.unlock()?;

        for (idx, addr) in (start..start + data.len()).enumerate().step_by(2) {
            self.flash().cr.modify(|_, w| w.pg().set_bit());

            while self.flash().sr.read().bsy().bit_is_set() {}

            // Flash is written 16 bits at a time, so combine two bytes to get a half-word
            let hword: u16 = (data[idx] as u16) | (data[idx + 1] as u16) << 8;

            // NOTE(unsafe) Write to FLASH area with no side effects
            unsafe { core::ptr::write_volatile(addr as *mut u16, hword) };

            // Wait for write
            while self.flash().sr.read().bsy().bit_is_set() {}
            self.flash().cr.modify(|_, w| w.pg().clear_bit());

            // Check for errors
            let sr = self.flash().sr.read();

            if sr.pgerr().bit_is_set() || sr.wrprterr().bit_is_set() {
                self.flash()
                    .sr
                    .modify(|_, w| w.pgerr().clear_bit().wrprterr().clear_bit());

                self.lock();
                return Err(FlashError::ProgrammingError);
            }

            let verify = unsafe { core::ptr::read_volatile(addr as *mut u16) };
            if verify != hword {
                self.lock();
                log!("Verification error during programming");
                return Err(FlashError::VerificationError);
            }
        }
        // Lock Flash and report success
        self.lock();
        Ok(())
    }

    fn valid_range(start: usize, length: usize) -> bool {
        (start >= CONFIG_ADD) && (start + length < FLASH_END)
    }
}
