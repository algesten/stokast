//! Wrapper for all the errors.

use imxrt_hal::i2c;
use imxrt_hal::spi;
use imxrt_hal::spi::ModeError;

#[derive(Debug)]
pub enum Error {
    SpiClockSpeedError(spi::ClockSpeedError),
    I2CClockSpeedError(i2c::ClockSpeedError),
    SpiError(spi::Error),
    I2CError(i2c::Error),
    ModeError(ModeError),
    Other(&'static str),
}

impl From<spi::ClockSpeedError> for Error {
    fn from(e: spi::ClockSpeedError) -> Self {
        Error::SpiClockSpeedError(e)
    }
}

impl From<i2c::ClockSpeedError> for Error {
    fn from(e: i2c::ClockSpeedError) -> Self {
        Error::I2CClockSpeedError(e)
    }
}

impl From<spi::Error> for Error {
    fn from(e: spi::Error) -> Self {
        Error::SpiError(e)
    }
}

impl From<i2c::Error> for Error {
    fn from(e: i2c::Error) -> Self {
        Error::I2CError(e)
    }
}

impl From<ModeError> for Error {
    fn from(e: ModeError) -> Self {
        Error::ModeError(e)
    }
}

// impl From<FromResidual<Result<(), ()>>> for Error {}
