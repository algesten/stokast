#![no_std]
#![no_main]

use alg::Rnd;
use teensy4_bsp as bsp;
use teensy4_panic as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = bsp::Peripherals::take().unwrap();
    let mut systick = bsp::SysTick::new(cortex_m::Peripherals::take().unwrap().SYST);

    let pins = bsp::t40::into_pins(p.iomuxc);
    let mut led = bsp::configure_led(pins.p13);

    let mut rnd = Rnd::new(1);

    loop {
        led.toggle();

        let delay = rnd.next() / (u32::max_value() / 500);
        systick.delay(delay);
    }
}
