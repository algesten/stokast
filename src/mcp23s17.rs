//! Driver for MCP23S17 which is a 16-Bit I/O Expander.
//!
//! Datasheet here: https://ww1.microchip.com/downloads/en/DeviceDoc/20001952C.pdf

// The MCP23S7 starts in 16-bit mode.

use embedded_hal::blocking::spi::{Transfer, Write};

pub struct Mcp23S17<I> {
    spi: I,
}

pub fn builder() -> Builder {
    Builder {
        ..Default::default()
    }
}

impl<I, E> Mcp23S17<I>
where
    I: Transfer<u16, Error = E>,
    I: Write<u16, Error = E>,
{
    fn configure(&mut self, params: Builder) {
        //
    }

    pub fn config(&mut self) -> Result<(), E> {
        let mut buf2 = [address(false, 0x00), 0];
        info!("read buf: {:0x?}", buf2);
        self.spi.transfer(&mut buf2)?;
        info!("0x00 is {:0x?}", buf2[1]);

        let mut buf2 = [address(false, 0x02), 0];
        info!("read buf: {:0x?}", buf2);
        self.spi.transfer(&mut buf2)?;
        info!("0x02 is {:0x?}", buf2[1]);

        let mut buf2 = [address(true, 0x02), 0xffff];
        info!("write buf: {:0x?}", buf2);
        self.spi.transfer(&mut buf2)?;

        let mut buf2 = [address(false, 0x02), 0];
        self.spi.transfer(&mut buf2)?;
        info!("0x02 after write is {:0x?}", buf2[1]);

        Ok(())
    }

    pub fn try_read(&mut self) -> Result<(), E> {
        let mut buf2 = [address(false, 0x12), 0];
        self.spi.transfer(&mut buf2)?;
        info!("0x12 is {:0x?}", buf2[1]);

        Ok(())
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
    pub fn build<I, E>(self, spi: I) -> Mcp23S17<I>
    where
        I: Transfer<u16, Error = E>,
        I: Write<u16, Error = E>,
    {
        let mut m = Mcp23S17 { spi };
        m.configure(self);
        m
    }

    /// Configure an input pin. Pins are enumerated from 0. Where pin 0 is
    /// bank A and pin 8 is the first in bank B.
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

pub trait SetBit {
    fn set_bit(&mut self, bit: u8, on: bool);
}

impl SetBit for u16 {
    fn set_bit(&mut self, bit: u8, on: bool) {
        if on {
            *self = *self | 1 << bit;
        } else {
            *self = *self & !(1 << bit);
        }
    }
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptMode {
    CompareAgainstPrevious,
    CompareAgainst(bool),
}
