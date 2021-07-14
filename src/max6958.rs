#![allow(dead_code)]

//! Driver for Max6958/Max6959 segment LED controller.
//! Datasheet here: https://datasheets.maximintegrated.com/en/ds/MAX6958-MAX6958.pdf

use cortex_m::interrupt::CriticalSection;
use embedded_hal::blocking::i2c::{Write, WriteRead};

use crate::lock::Lock;

// At power-up, the MAX6958/ MAX6959 are initially set to scan four digits,
// do not decode data in the digit registers or scan key switches,
// and the intensity register is set to a low value (4/64 intensity).

/// Max6958 address variant.
///
/// It's possible to have two max6958 on the same bus, in which case one
/// is addressed 0111000 and the other 0111001. The address is hardcoded in the
/// chip and must be ordered separately.
///
/// * MAX6958AAPE A-variant
/// * MAX6958BAPE B-variant
pub enum Variant {
    A = 0b0111000,
    B = 0b0111001,
}

pub struct Max6958<I> {
    i2c: Lock<I>,
    addr: u8,
}

impl<I, E> Max6958<I>
where
    I: Write<Error = E>,
    I: WriteRead<Error = E>,
{
    pub fn new(i2c: Lock<I>, variant: Variant) -> Self {
        Max6958 {
            i2c,
            addr: variant as u8,
        }
    }

    /// Set shutdown mode. The device starts in shutdown mode on power on.
    pub fn set_shutdown(&mut self, shutdown: bool, cs: &CriticalSection) -> Result<(), E> {
        info!("set_shutdown: {}", shutdown);
        let config_r = self.read_register(Register::Configuration, cs)?;

        info!("current config {:?}", config_r);

        let config = config_r | if shutdown { 0 } else { 1 };

        self.write_register(Register::Configuration, config, cs)?;
        Ok(())
    }

    pub fn set_display_test(&mut self, test: bool, cs: &CriticalSection) -> Result<(), E> {
        info!("set_diplay_test: {}", test);
        self.write_register(Register::DisplayTest, if test { 1 } else { 0 }, cs)
    }

    pub fn set_intensity(&mut self, intensity: u8, cs: &CriticalSection) -> Result<(), E> {
        info!("set_intensity: {}", intensity);
        assert!(intensity <= 0x3f, "Intensity range 0x00-0x3f");
        self.write_register(Register::Intensity, intensity, cs)
    }

    pub fn set_scan_limit(&mut self, limit: ScanLimit, cs: &CriticalSection) -> Result<(), E> {
        info!("set_scan_limit: {:?}", limit);
        self.write_register(Register::ScanLimit, limit as u8, cs)
    }

    pub fn set_decode_mode(&mut self, decode: &[Digit], cs: &CriticalSection) -> Result<(), E> {
        info!("set_decode_mode: {:?}", decode);
        let mut mode = 0;

        for d in decode {
            mode |= 1 << (*d as u8);
        }

        self.write_register(Register::DecodeMode, mode, cs)
    }

    pub fn set_digit(&mut self, digit: Digit, value: u8, cs: &CriticalSection) -> Result<(), E> {
        debug!("set_digit {:?} {}", digit, value);
        self.write_register(digit.as_reg(), value, cs)
    }

    fn write_register(&mut self, reg: Register, data: u8, cs: &CriticalSection) -> Result<(), E> {
        let mut i2c = self.i2c.get(cs);
        i2c.write(self.addr, &[reg.addr(), data])
    }

    fn read_register(&mut self, reg: Register, cs: &CriticalSection) -> Result<u8, E> {
        let mut buf = [0; 1];
        let mut i2c = self.i2c.get(cs);
        i2c.write_read(self.addr, &[reg.addr()], &mut buf)?;
        Ok(buf[0])
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ScanLimit {
    Digit0 = 0x00,
    Digit01 = 0x01,
    Digit012 = 0x02,
    Digit0123 = 0x03,
}

#[derive(Clone, Copy, Debug)]
pub enum Digit {
    Digit0 = 0b0001,
    Digit1 = 0b0010,
    Digit2 = 0b0100,
    Digit3 = 0b1000,
}

impl Digit {
    fn as_reg(&self) -> Register {
        match self {
            Digit::Digit0 => Register::Digit0,
            Digit::Digit1 => Register::Digit1,
            Digit::Digit2 => Register::Digit2,
            Digit::Digit3 => Register::Digit3,
        }
    }
}

#[derive(Clone, Copy, Debug)]
/// Register values from the datasheet.
enum Register {
    NoOp = 0x00,
    DecodeMode = 0x01,
    Intensity = 0x02,
    ScanLimit = 0x03,
    Configuration = 0x04,
    FactoryReserved = 0x05,
    GpIo = 0x06,
    DisplayTest = 0x07,
    ReadKeyDebounced = 0x08,
    ReadKeyPressed = 0x0C,
    Digit0 = 0x20,
    Digit1 = 0x21,
    Digit2 = 0x22,
    Digit3 = 0x23,
    Segments = 0x24,
}

impl Register {
    fn addr(&self) -> u8 {
        *self as u8
    }
}
