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

    pub fn set_segs<const X: usize>(&mut self, s: Segs<X>, cs: &CriticalSection) -> Result<(), E> {
        let mut buf = s.0;
        buf[0] = Register::Digit0.addr();

        let mut i2c = self.i2c.get(cs);
        i2c.write(self.addr, &buf)
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

/// Translation from numbers/chars to segments.
///
/// ```ignore
/// d7 d6 d5 d4 d3 d2 d1 d0
///  X  a  b  c  d  e  f  g
///
///         +- a -+
///         f     b
///         |- g -|
///         e     c
///         +- d -+
/// ```
///
/// The translation.
pub enum Seg {
    SP = 0b00000000,
    N0 = 0b01111110,
    N1 = 0b00110000,
    N2 = 0b01101101,
    N3 = 0b01111001,
    N4 = 0b00110011,
    N5 = 0b01011011,
    N6 = 0b01011111,
    N7 = 0b01110000,
    N8 = 0b01111111,
    N9 = 0b01110011,
    A = 0b01110111,
    B = 0b00011111,
    C = 0b00001101,
    D = 0b00111101,
    E = 0b01001111,
    F = 0b01000111,
    G = 0b01111011,
    H = 0b00010111,
    I = 0b00000110,
    J = 0b01111100,
    // K = 0b00000000,
    L = 0b00001110,
    // M = 0b00000000,
    N = 0b00010101,
    O = 0b00011101,
    P = 0b01100111,
    // Q = 0b00000000,
    R = 0b00000101,
    // S = 0b00000000,
    T = 0b00001111,
    U = 0b00011100,
    // V = 0b00000000,
    // W = 0b00000000,
    // X = 0b00000000,
    Y = 0b00111011,

    CornerBr = 0b00011000,
    CornerMru = 0b00100001,
    CornerTr = 0b01100000,
    CornerMrd = 0b00010001,

    SegA = 0b01000000,
    SegB = 0b00100000,
    SegC = 0b00010000,
    SegD = 0b00001000,
    SegE = 0b00000100,
    SegF = 0b00000010,
    SegG = 0b00000001,
}

//  -
// | |
//  -
// | |
//  -

/// Holder of X-1 number of segments (needs 1 byte extra). Use [`Segs4`].
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segs<const X: usize>(pub [u8; X]);

impl<const X: usize> Segs<X> {
    pub fn new() -> Self {
        Segs([0; X])
    }

    pub fn as_buf(&mut self) -> &mut [u8] {
        // first byte is reserved for the command
        &mut self.0[1..]
    }
}

/// Type for sending 4 chars in one go. Can be converted from a &str or number.
///
/// The extra byte is for the i2c command.
pub type Segs4 = Segs<5>;

impl<const X: usize> From<&str> for Segs<X> {
    fn from(s: &str) -> Self {
        assert!(s.len() <= X);

        // Write chars reversed in to the buffer, so we can send "SYNC" and get
        // C on position 0, N on position 1 etc.
        let mut buf = [0; X];
        for (i, c) in s.bytes().rev().enumerate() {
            // Convert each via the Chars conversion enum.
            buf[i + 1] = Seg::from(c) as u8;
        }

        Segs(buf)
    }
}

impl<const X: usize> From<u32> for Segs<X> {
    fn from(mut n: u32) -> Self {
        let mut buf = [0; X];

        for i in 1..X {
            buf[i] = Seg::from((n % 10) as u8) as u8;
            n /= 10;
        }

        Segs(buf)
    }
}

impl<const X: usize> From<u16> for Segs<X> {
    fn from(n: u16) -> Self {
        Segs::from(n as u32)
    }
}

impl<const X: usize> From<u8> for Segs<X> {
    fn from(n: u8) -> Self {
        Segs::from(n as u32)
    }
}

impl From<u8> for Seg {
    fn from(mut n: u8) -> Self {
        use Seg::*;

        // ascii number range
        if n >= 48 && n <= 57 {
            n -= 48;
        }

        // ascii lowercase
        if n >= 97 && n <= 122 {
            n -= 32;
        }

        match n {
            // if we send a "raw" number
            0 => N0,
            1 => N1,
            2 => N2,
            3 => N3,
            4 => N4,
            5 => N5,
            6 => N6,
            7 => N7,
            8 => N8,
            9 => N9,

            b' ' => SP,

            // ascii uppercase
            b'A' => A,
            b'B' => B,
            b'C' => C,
            b'D' => D,
            b'E' => E,
            b'F' => F,
            b'G' => G,
            b'H' => H,
            b'I' => I,
            b'J' => J,
            // 75 => K,
            b'L' => L,
            // 77 => M,
            b'N' => N,
            b'O' => O,
            b'P' => P,
            // 81 => Q,
            b'R' => R,
            b'S' => N5,
            b'T' => T,
            b'U' => U,
            // 86 => V,
            // 87 => W,
            // 88 => X,
            b'Y' => Y,
            b'Z' => N2,

            _ => panic!("Unmapped char"),
        }
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
