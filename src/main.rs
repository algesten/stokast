#![no_std]
#![no_main]

#[macro_use]
extern crate log;

use alg::clock::Clock;
use alg::clock::Time;
use bsp::hal::ccm;
use cortex_m::peripheral::DWT;
use embedded_hal::spi;
use imxrt_hal::gpio::GPIO;
use teensy4_bsp as bsp;

use crate::input::Inputs;
use crate::max6958::Digit;
use crate::state::OperQueue;
use crate::state::State;

mod input;
mod logging;
mod max6958;
mod mcp23s17;
mod state;

pub const CPU_SPEED: u32 = ccm::PLL1::ARM_HZ;

#[cortex_m_rt::entry]
fn main() -> ! {
    assert!(logging::init().is_ok());

    let mut p = bsp::Peripherals::take().unwrap();
    let mut cp = cortex_m::Peripherals::take().unwrap();
    let mut systick = bsp::SysTick::new(cp.SYST);

    // Set clock to the recommended 600 MHz.
    p.ccm
        .pll1
        .set_arm_clock(ccm::PLL1::ARM_HZ, &mut p.ccm.handle, &mut p.dcdc);

    // // needed for timer
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();

    let mut clock = Clock::<_, CPU_SPEED>::new(DWT::get_cycle_count);

    // Wait so we don't miss the first log message, crashes etc.
    systick.delay(1000);

    let pins = bsp::t40::into_pins(p.iomuxc);

    // 1.6E-5
    // 16µS

    let (_, _, _, spi4_builder) = p.spi.clock(
        &mut p.ccm.handle,
        ccm::spi::ClockSelect::Pll2,
        ccm::spi::PrescalarSelect::LPSPI_PODF_5,
    );

    let mut spi_io = spi4_builder.build(pins.p11, pins.p12, pins.p13);

    // // From datasheet for MCP23S17 we see that max speed is 10MHz
    spi_io
        .set_clock_speed(bsp::hal::spi::ClockSpeed(5_000_000))
        .unwrap();

    let spi_cs = GPIO::new(pins.p10);
    let spi_cs = spi_cs.output();

    spi_io.set_mode(spi::MODE_0).unwrap();
    spi_io.clear_fifo();

    let _io = mcp23s17::builder()
        //
        .input(0)
        .enable_interrupt(mcp23s17::InterruptMode::CompareAgainstPrevious)
        .done()
        //
        .input(1)
        .enable_interrupt(mcp23s17::InterruptMode::CompareAgainstPrevious)
        .done()
        //
        .input(2)
        .enable_interrupt(mcp23s17::InterruptMode::CompareAgainstPrevious)
        .done()
        //
        .build(spi_io, spi_cs)
        .unwrap();

    // let enc_inner = Encoder::new(pin_a, pin_b);
    // let mut encoder = EncoderAccelerator::new(enc_inner);

    // How to configure an ADC
    // let (adc1_builder, _) = p.adc.clock(&mut p.ccm.handle);
    // let mut adc1 = adc1_builder.build(adc::ClockSelect::default(), adc::ClockDivision::default());
    // let mut a1 = adc::AnalogInput::new(pins.p14);
    // let _reading: u16 = adc1.read(&mut a1).unwrap();

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
    seg.set_intensity(21).unwrap();
    seg.set_decode_mode(&[Digit::Digit0, Digit::Digit1, Digit::Digit2, Digit::Digit3])
        .unwrap();

    info!("Sure!");

    let mut start = clock.now();
    let mut loop_count = 0_u32;

    let mut inputs = Inputs {
        seed: (),
        length: (),
        offs1: (),
        step1: (),
        offs2: (),
        step2: (),
        offs3: (),
        step3: (),
        offs4: (),
        step4: (),
    };

    let mut state = State {
        ..Default::default()
    };

    let mut opers = OperQueue::new();

    loop {
        clock.tick();

        let now = clock.now();

        let time_lapsed = now - start;
        if time_lapsed >= Time::from_secs(10) {
            // 2021-07-01 this is: 71_424_181
            //  rotary enc decel   52_566_664
            info!("{} loop count: {}", time_lapsed, loop_count);
            start = now;
            loop_count = 0;
        }

        // Read all potential input and turn it into operations.
        inputs.tick(now, &mut opers);

        // Apply the operations to the state.
        state.update(now, opers.drain(0..opers.len()));

        loop_count += 1;
    }
}

#[panic_handler]
fn panic(p: &core::panic::PanicInfo) -> ! {
    error!("{:?}", p);
    loop {}
}
