#![no_std]
#![no_main]

#[macro_use]
extern crate log;

use alg::Clock;
use alg::Time;
// use alg::Rnd;
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

// TODO can I read this somewhere?
pub const CPU_SPEED: u32 = 600_000_000;

#[cortex_m_rt::entry]
fn main() -> ! {
    assert!(logging::init().is_ok());

    let mut p = bsp::Peripherals::take().unwrap();
    let mut cp = cortex_m::Peripherals::take().unwrap();
    let mut systick = bsp::SysTick::new(cp.SYST);

    // // needed for timer
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();

    // Wait so we don't miss the first log message, crashes etc.
    systick.delay(1000);

    let pins = bsp::t40::into_pins(p.iomuxc);
    let mut led = bsp::configure_led(pins.p13);

    let mut pin_a = GPIO::new(pins.p11);
    let mut pin_b = GPIO::new(pins.p12);
    pin_a.set_fast(true);
    pin_b.set_fast(true);

    // 1.6E-5
    // 16µS

    let mut encoder = Encoder::new(pin_a, pin_b);

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

    // let mut rnd = Rnd::new(1);

    let mut seg = max6958::Max6958::new(i2c1, max6958::Variant::A);
    seg.set_shutdown(false).unwrap();

    // driver.set_decode_mode(&[Digit::Digit0]).unwrap();
    seg.set_scan_limit(max6958::ScanLimit::Digit0).unwrap();
    seg.set_intensity(63).unwrap();
    seg.set_decode_mode(&[Digit::Digit0, Digit::Digit1, Digit::Digit2, Digit::Digit3])
        .unwrap();

    info!("Sure!");

    let _reading: u16 = adc1.read(&mut a1).unwrap();

    systick.delay(1000);

    let mut clock = Clock::<_, CPU_SPEED>::new(DWT::get_cycle_count);
    let mut start = clock.now();
    let mut loop_count = 0_u32;

    let mut n = 1_u8;

    loop {
        clock.tick();

        let now = clock.now();

        let time_lapsed = now - start;

        if time_lapsed >= Time::from_secs(10) {
            // 2021-07-01 this is: 71_424_242
            info!("{} loop count: {}", time_lapsed, loop_count);
            start = now;
            loop_count = 0;
        }

        let dir = encoder.tick();

        if dir > 0 {
            if n == 9 {
                n = 0;
            } else {
                n += 1;
            }
            seg.set_digit(Digit::Digit0, n).unwrap();
            led.toggle();
        } else if dir < 0 {
            if n == 0 {
                n = 9;
            } else {
                n -= 1;
            }
            seg.set_digit(Digit::Digit0, n).unwrap();
            led.toggle();
        }

        loop_count += 1;
    }
}

#[panic_handler]
fn panic(p: &core::panic::PanicInfo) -> ! {
    error!("{:?}", p);
    loop {}
}
