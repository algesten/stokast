//! This module is very temporary until we land this PR:
//! https://github.com/imxrt-rs/imxrt-hal/pull/110

#![allow(dead_code)]

use bsp::hal::gpio::Input;
use imxrt_hal::gpio::GPIO;
use imxrt_hal::iomuxc::{consts::Unsigned, gpio::Pin};
use imxrt_ral as ral;
use imxrt_ral::gpio::{self, RegisterBlock};
use teensy4_bsp as bsp;

pub trait Interrupt {
    fn register_block(&self) -> *const RegisterBlock;
    fn mask(&self) -> u32;
    fn module(&self) -> usize;
    fn set_interrupt_enable(&mut self, enable: bool);
    fn is_interrupt_enabled(&self) -> bool;
    fn set_interrupt_configuration(&mut self, interrupt_configuration: InterruptConfiguration);
    fn is_interrupt_status(&self) -> bool;
    fn clear_interrupt_status(&mut self);
}

/// GPIO input interrupt configurations.
///
/// These configurations do not take effect until
/// GPIO input interrupts are enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum InterruptConfiguration {
    // TODO isn't it a bit repetitive to have "Sensitive" on all here?
    LowLevel = 0,
    HighLevel = 1,
    RisingEdge = 2,
    FallingEdge = 3,
    EitherEdge = 4,
}

impl<P> Interrupt for GPIO<P, Input>
where
    P: Pin,
{
    fn register_block(&self) -> *const RegisterBlock {
        const REGISTER_BLOCKS: [*const RegisterBlock; 9] = [
            gpio::GPIO1,
            gpio::GPIO2,
            gpio::GPIO3,
            gpio::GPIO4,
            gpio::GPIO5,
            gpio::GPIO6,
            gpio::GPIO7,
            gpio::GPIO8,
            gpio::GPIO9,
        ];
        REGISTER_BLOCKS[self.module().wrapping_sub(1)]
    }

    fn mask(&self) -> u32 {
        1u32 << <P as Pin>::Offset::USIZE
    }

    fn module(&self) -> usize {
        <P as Pin>::Module::USIZE
    }

    /// Enable (`true`) or disable (`false`) interrupts for this GPIO input.
    fn set_interrupt_enable(&mut self, enable: bool) {
        cortex_m::interrupt::free(|_| unsafe {
            ral::modify_reg!(ral::gpio, self.register_block(), IMR, |imr| if enable {
                imr | self.mask()
            } else {
                imr & !self.mask()
            })
        });
    }

    /// Indicates if interrupts are (`true`) or are not (`false`) enabled for this GPIO input.
    fn is_interrupt_enabled(&self) -> bool {
        unsafe { ral::read_reg!(ral::gpio, self.register_block(), IMR) & self.mask() != 0u32 }
    }

    /// Set the interrupt configuration for this GPIO input.
    fn set_interrupt_configuration(&mut self, interrupt_configuration: InterruptConfiguration) {
        cortex_m::interrupt::free(|_| unsafe {
            if InterruptConfiguration::EitherEdge == interrupt_configuration {
                ral::modify_reg!(ral::gpio, self.register_block(), EDGE_SEL, |edge_sel| {
                    edge_sel | self.mask()
                });
            } else {
                ral::modify_reg!(ral::gpio, self.register_block(), EDGE_SEL, |edge_sel| {
                    edge_sel & !self.mask()
                });
                // TODO is it ok to assume the numeric value of the enum is the icr?
                let icr = interrupt_configuration as u32;
                info!("icr is: {:?}", icr);
                let icr_offset = (<P as Pin>::Offset::USIZE % 16) * 2;
                let icr_modify = |reg| reg & !(0b11 << icr_offset) | (icr << icr_offset);
                if <P as Pin>::Offset::USIZE < 16 {
                    ral::modify_reg!(ral::gpio, self.register_block(), ICR1, icr_modify);
                } else {
                    ral::modify_reg!(ral::gpio, self.register_block(), ICR2, icr_modify);
                }
            }
        })
    }

    /// Indicates whether this GPIO input triggered an interrupt.
    fn is_interrupt_status(&self) -> bool {
        unsafe { ral::read_reg!(ral::gpio, self.register_block(), ISR) & self.mask() != 0u32 }
    }

    /// Clear the interrupt status flag.
    fn clear_interrupt_status(&mut self) {
        // TODO is it okay to unconditonally write to clear, or do we need to
        // check it's actually set first?
        unsafe { ral::write_reg!(ral::gpio, self.register_block(), ISR, self.mask()) }
    }
}
