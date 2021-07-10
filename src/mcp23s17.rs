#![allow(dead_code)]

//! Driver for MCP23S17 which is a 16-Bit I/O Expander.
//!
//! Datasheet here: https://ww1.microchip.com/downloads/en/DeviceDoc/20001952C.pdf

// The MCP23S7 starts in 16-bit mode.

// SPI has no addressing mechanic (like I2C), so instead it selects the chip to talk to
// using another pin. Since we use a single chip, we can set it like this.
// _However_ it seems the MCP23S17 specifically, in addition to the CS pin also can run in
// with an address set by some pins (HAEN).

// This seems totally broken. Let's not do that, and take control over the CS ourselves.
// spi.enable_chip_select_0(pins.p10);

use alg::SetBit;
use bsp::hal::gpio::{Output, GPIO};
use cortex_m::interrupt::CriticalSection;
use embedded_hal::blocking::spi::{Transfer, Write};
use embedded_hal::digital::v2::OutputPin;
use imxrt_hal::iomuxc::gpio::Pin;
use teensy4_bsp as bsp;

use crate::lock::Lock;

pub struct Mcp23S17<I, P> {
    spi_lock: Lock<I>,
    cs: GPIO<P, Output>,
}

pub fn builder() -> Builder {
    Builder {
        // By default, all pins are configured as inputs.
        dir: 0xffff,
        ..Default::default()
    }
}

impl<I, P, E> Mcp23S17<I, P>
where
    I: Transfer<u16, Error = E>,
    I: Write<u16, Error = E>,
    P: Pin,
{
    fn configure(&mut self, params: Builder, cs: &CriticalSection) -> Result<(), E> {
        // high when not active.
        self.cs.set_high().unwrap();

        // since we read all pins in one 16 bit read, we might as well have the
        // interrupt pins mirror each other.
        self.transfer(address(true, 0x0a), 0b0100_0000_0100_0000, cs)?;

        self.transfer(address(true, 0x00), params.dir, cs)?;
        self.transfer(address(true, 0x02), params.pol, cs)?;
        self.transfer(address(true, 0x04), params.int, cs)?;
        self.transfer(address(true, 0x06), params.def, cs)?;
        self.transfer(address(true, 0x08), params.con, cs)?;
        self.transfer(address(true, 0x0c), params.pul, cs)?;

        Ok(())
    }

    fn transfer(&mut self, addr: u16, value: u16, cs: &CriticalSection) -> Result<u16, E> {
        self.cs.set_low().unwrap();

        let mut buf = [addr, value];
        let mut spi = self.spi_lock.get(cs);
        spi.transfer(&mut buf)?;

        self.cs.set_high().unwrap();
        Ok(buf[1])
    }

    pub fn read_inputs(&mut self, cs: &CriticalSection) -> Result<u16, E> {
        self.transfer(address(false, 0x12), 0, cs)
    }
}

fn address(write: bool, addr: u8) -> u16 {
    // 0100-A2-A1-A0-RW-<addr>
    // The Write command (slave address with R/W bit cleared).
    0b_0100_0000_0000_0000 | if write { 0 } else { 1 << 8 } | (addr as u16)
}

#[derive(Debug, Default)]
pub struct Builder {
    /// Direction, 0 = output, 1 = input.
    /// Defaults to inputs.
    dir: u16,
    /// Polarity, 0 = normal, 1 = inverted
    pol: u16,

    // All three of int, def and con must be set to enable an interrupt
    /// interrupt on change (0 = not, 1 = yes)
    int: u16,
    /// default value. when con is 1, interrupt occurs when the pin has the opposite value to def.
    def: u16,
    /// control register. 0 = compare against previous, 1 = compare against def.
    con: u16,

    /// Pull-up for inputs. 0 = no pull-up, 1 = pulled up (100k resistor)
    pul: u16,
}

impl Builder {
    pub fn build<I, E, P>(self, spi_lock: Lock<I>, cs: GPIO<P, Output>) -> Result<Mcp23S17<I, P>, E>
    where
        I: Transfer<u16, Error = E>,
        I: Write<u16, Error = E>,
        P: Pin,
    {
        let mut m = Mcp23S17 { spi_lock, cs };
        cortex_m::interrupt::free(|cs| m.configure(self, cs))?;

        Ok(m)
    }

    /// Enable interrupts on all pins (currently) configured as inputs.
    pub fn enable_all_interrupts(mut self, mode: InterruptMode) -> Self {
        for pin in 0..15 {
            if self.dir.is_bit(pin) {
                self = self.input(pin).enable_interrupt(mode).done()
            }
        }
        self
    }

    /// Configure an input pin. Pins are enumerated from 0. Where pin 0 is
    /// bank A and pin 8 is the first in bank B.
    ///
    /// By default, pins are configured as inputs.
    pub fn input(mut self, pin: u8) -> Input {
        self.dir.set_bit(pin, true);
        Input { builder: self, pin }
    }

    /// Configure an output pin. Pins are enumerated from 0. Where pin 0 is
    /// bank A and pin 8 is the first in bank B.
    pub fn output(mut self, pin: u8) -> Self {
        self.dir.set_bit(pin, false);
        self
    }
}

#[derive(Debug, Default)]
pub struct Input {
    builder: Builder,
    pin: u8,
}

impl Input {
    /// Invert the polarity of the pin.
    pub fn polarity(mut self, inverted: bool) -> Self {
        self.builder.pol.set_bit(self.pin, inverted);
        self
    }

    /// Set whether the pin should have a pull-up (100k resistor).
    pub fn pull_up(mut self, pull_up: bool) -> Self {
        self.builder.pul.set_bit(self.pin, pull_up);
        self
    }

    pub fn enable_interrupt(mut self, mode: InterruptMode) -> Self {
        self.builder.int.set_bit(self.pin, true);
        if let InterruptMode::CompareAgainst(def_value) = mode {
            self.builder.def.set_bit(self.pin, def_value);
            self.builder.con.set_bit(self.pin, true);
        } else {
            self.builder.def.set_bit(self.pin, false);
            self.builder.con.set_bit(self.pin, false);
        }
        self
    }

    pub fn done(self) -> Builder {
        self.builder
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptMode {
    CompareAgainstPrevious,
    CompareAgainst(bool),
}
