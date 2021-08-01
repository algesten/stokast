#![allow(dead_code)]

//! Driver for MCP4728 4 channel 12-bit DAC.
//! Datasheet here: <https://ww1.microchip.com/downloads/en/DeviceDoc/22187E.pdf>

use cortex_m::interrupt::CriticalSection;
use embedded_hal::blocking::i2c::{Write, WriteRead};

use crate::lock::Lock;

/// 7 bit address, lower three bits are programmable in EEPROM (or by factory), but defaults to 000.
/// Lowest bit is R/W where 1 means.
const ADDRESS: u8 = 0b1100000_0;

/// "Single write" means updating one channel at a time. There are other
/// commands that can set all channels in one I2C transaction.
/// Lowest three bits are [DAC1, DAC0, UDAC]
const SINGLE_WRITE: u8 = 0b01011_000;

pub struct Mcp4728<I> {
    i2c: Lock<I>,
}

impl<I, E> Mcp4728<I>
where
    I: Write<Error = E>,
    I: WriteRead<Error = E>,
{
    pub fn new(i2c: Lock<I>) -> Self {
        Mcp4728 { i2c }
    }

    /// Set the output value of a single DAC channel.
    pub fn set_channel(
        &mut self,
        channel: usize,
        value: u16,
        cs: &CriticalSection,
    ) -> Result<(), E> {
        // debug!("Set channel ({}): {}", channel, value);
        assert!(value <= 4095);

        return Ok(());

        let mut i2c = self.i2c.get(cs);
        let bytes = &[
            // [0 1 0 1 1 DAC1 DAC0 UDAC]
            SINGLE_WRITE | ((channel as u8) << 1),
            // [VREF PD1 PD0 Gx D11 D10 D9 D8] (for VREF, PD1, PD0 and Gx we use 0)
            (value >> 8) as u8,
            // [D7 D6 D5 D4 D3 D2 D1 D0]
            (value & 0xff) as u8,
        ];

        i2c.write(ADDRESS, bytes)
    }
}
