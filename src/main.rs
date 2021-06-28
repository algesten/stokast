#![no_std]
#![no_main]

#[macro_use]
extern crate log;

use alg::Rnd;
use bsp::hal::adc;
use bsp::hal::ccm;
use bsp::hal::gpio::GPIO;
use cortex_m::peripheral::DWT;
use embedded_hal::adc::OneShot;
use teensy4_bsp as bsp;

use crate::encoder::Encoder;
use crate::max6958::Digit;

mod encoder;
mod logging;
mod max6958;

#[cortex_m_rt::entry]
fn main() -> ! {
    assert!(logging::init().is_ok());

    let mut p = bsp::Peripherals::take().unwrap();
    let mut cp = cortex_m::Peripherals::take().unwrap();
    let mut systick = bsp::SysTick::new(cp.SYST);

    // needed for encoder
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();

    // Wait so we don't miss the first log message, crashes etc.
    systick.delay(1000);

    let pins = bsp::t40::into_pins(p.iomuxc);
    let mut led = bsp::configure_led(pins.p13);

    let mut input1 = GPIO::new(pins.p11);
    let mut input2 = GPIO::new(pins.p12);
    input1.set_fast(true);
    input2.set_fast(true);
    let mut encoder = Encoder::new(input1, input2, 20_000);

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
    let mut i2c1 = i2c1_builder.build(pins.p19, pins.p18);

    // From datasheet MAX6958, serial max speed is 400KHz
    i2c1.set_clock_speed(bsp::hal::i2c::ClockSpeed::KHz400)
        .unwrap();

    let mut rnd = Rnd::new(1);

    let mut driver = max6958::Max6958::new(i2c1, max6958::Variant::A);
    driver.set_display_test(true).unwrap();
    systick.delay(1000);
    driver.set_display_test(false).unwrap();

    driver.set_shutdown(false).unwrap();

    // driver.set_decode_mode(&[Digit::Digit0]).unwrap();
    driver.set_scan_limit(max6958::ScanLimit::Digit0).unwrap();
    driver.set_intensity(1).unwrap();

    info!("Sure!");

    let _reading: u16 = adc1.read(&mut a1).unwrap();

    systick.delay(1000);

    let mut n = 1_u8;

    loop {
        let dir = encoder.tick();

        if dir < 0 {
            if n == 1 {
                n = 64;
            } else {
                n = n >> 1;
            }
            info!("back {}", n);
            driver.set_digit(Digit::Digit0, n).unwrap();
        } else if dir > 0 {
            if n == 64 {
                n = 1;
            } else {
                n = n << 1;
            }
            info!("forw {}", n);
            driver.set_digit(Digit::Digit0, n).unwrap();
        }
    }
}

#[panic_handler]
fn panic(p: &core::panic::PanicInfo) -> ! {
    error!("{:?}", p);
    loop {}
}
