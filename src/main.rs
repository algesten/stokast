#![no_std]
#![no_main]

use alg::Rnd;
use embedded_hal::adc::OneShot;
use teensy4_bsp as bsp;
use teensy4_panic as _;

use bsp::hal::adc;

#[cortex_m_rt::entry]
fn main() -> ! {
    let mut p = bsp::Peripherals::take().unwrap();
    let mut systick = bsp::SysTick::new(cortex_m::Peripherals::take().unwrap().SYST);

    let pins = bsp::t40::into_pins(p.iomuxc);
    let mut led = bsp::configure_led(pins.p13);

    let (adc1_builder, _) = p.adc.clock(&mut p.ccm.handle);

    let mut adc1 = adc1_builder.build(adc::ClockSelect::default(), adc::ClockDivision::default());
    let mut a1 = adc::AnalogInput::new(pins.p14);

    let mut rnd = Rnd::new(1);

    loop {
        let reading: u16 = adc1.read(&mut a1).unwrap();

        led.toggle();

        let delay = rnd.next() / (u32::max_value() / 500) + reading as u32;
        systick.delay(delay);
    }
}
