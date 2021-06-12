#![no_std]
#![no_main]

use core::panic::PanicInfo;
use cortex_m_rt::{entry, exception, ExceptionFrame};
use stm32h7::stm32h747cm7;

#[entry]
fn main() -> ! {
    let peripherals = stm32h747cm7::Peripherals::take().unwrap();
    let gpioa = &peripherals.GPIOA;

    loop {
        gpioa.odr.modify(|_, w| w.odr0().set_bit());
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        //
    }
}

// More here: https://docs.rs/cortex-m-rt/0.6.14/cortex_m_rt/attr.exception.html
#[exception]
fn HardFault(_ef: &ExceptionFrame) -> ! {
    loop {
        //
    }
}
