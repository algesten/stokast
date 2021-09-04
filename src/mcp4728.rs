#![allow(dead_code)]

//! Driver for MCP4728 4 channel 12-bit DAC.
//! Datasheet here: <https://ww1.microchip.com/downloads/en/DeviceDoc/22187E.pdf>

use cortex_m::interrupt::CriticalSection;
use embedded_hal::blocking::i2c::{Read, Write};

use crate::lock::Lock;

/// 7 bit address, lower three bits are programmable in EEPROM (or by factory), but defaults to 000.
const ADDRESS: u8 = 0b1100_000;

pub struct Mcp4728<I> {
    i2c: Lock<I>,
    values: [u16; 4],
}

impl<I, E> Mcp4728<I>
where
    I: Write<Error = E>,
    I: Read<Error = E>,
{
    pub fn new(i2c: Lock<I>) -> Self {
        Mcp4728 {
            i2c,
            values: [0; 4],
        }
    }

    /// Set the output values for all 4 channels.
    pub fn set_channels(
        &mut self,
        update: &[Option<u16>; 4],
        cs: &CriticalSection,
    ) -> Result<(), E> {
        for (i, u) in update.iter().enumerate() {
            if let Some(u) = u {
                assert!(*u <= 4095);
                self.values[i] = *u;
            }
        }

        // Always write all 4 channels. The "single write" command seems broken in this ADC.
        let mut i2c = self.i2c.get(cs);
        let v = &self.values;
        let bytes = &[
            // [0 0 PD1 PD0 D11 D10 D9 D8], [D7 D6 D5 D4 D3 D2 D1 D0] // for PD1 and PD0 we use 0
            (v[0] >> 8) as u8,
            (v[0] & 0xff) as u8,
            (v[1] >> 8) as u8,
            (v[1] & 0xff) as u8,
            (v[2] >> 8) as u8,
            (v[2] & 0xff) as u8,
            (v[3] >> 8) as u8,
            (v[3] & 0xff) as u8,
        ];

        i2c.write(ADDRESS, bytes)?;

        Ok(())
    }
}
