#![no_std]
#![no_main]

#[macro_use]
extern crate log;

use alg::Rnd;
use embedded_hal::adc::OneShot;
use teensy4_bsp as bsp;
// use teensy4_panic as _;

use bsp::hal::adc;
use bsp::hal::ccm;

use crate::max6959::Digit;

mod logging;
mod max6959;

#[cortex_m_rt::entry]
fn main() -> ! {
    assert!(logging::init().is_ok());

    let mut p = bsp::Peripherals::take().unwrap();
    let mut systick = bsp::SysTick::new(cortex_m::Peripherals::take().unwrap().SYST);

    // Wait so we don't miss the first log message, crashes etc.
    systick.delay(1000);

    let pins = bsp::t40::into_pins(p.iomuxc);
    let mut led = bsp::configure_led(pins.p13);

    let (adc1_builder, _) = p.adc.clock(&mut p.ccm.handle);

    let mut adc1 = adc1_builder.build(adc::ClockSelect::default(), adc::ClockDivision::default());
    let mut a1 = adc::AnalogInput::new(pins.p14);

    let (i2c1_builder, _, _, _) = p.i2c.clock(
        &mut p.ccm.handle,
        ccm::i2c::ClockSelect::OSC, // 24MHz
        // TODO: Investigate what this is.
        ccm::i2c::PrescalarSelect::DIVIDE_3,
    );

    // The return of "builder.build()" is a configured I2C master running at 100KHz.
    let i2c1 = i2c1_builder.build(pins.p19, pins.p18);

    // From datasheet MAX6959, serial max speed is 400KHz
    // i2c1.set_clock_speed(bsp::hal::i2c::ClockSpeed::KHz400)
    //     .unwrap();

    let mut rnd = Rnd::new(1);

    let mut driver = max6959::Max6959::new(i2c1, max6959::Variant::A);
    driver.set_display_test(true).unwrap();
    systick.delay(1000);
    driver.set_display_test(false).unwrap();

    driver.set_shutdown(false).unwrap();

    // driver.set_decode_mode(&[Digit::Digit0]).unwrap();
    driver.set_scan_limit(max6959::ScanLimit::Digit0).unwrap();
    driver.set_intensity(20).unwrap();

    let mut num = 255;

    info!("Sure!");

    loop {
        let reading: u16 = adc1.read(&mut a1).unwrap();

        led.toggle();

        driver.set_digit(Digit::Digit0, num).unwrap();

        num = num << 1;
        if num == 0 {
            num = 1;
        }

        let delay = rnd.next() / (u32::max_value() / 500) + reading as u32;
        systick.delay(100);
    }
}

#[panic_handler]
fn panic(p: &core::panic::PanicInfo) -> ! {
    error!("{:?}", p);
    loop {}
}
