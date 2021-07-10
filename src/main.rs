#![no_std]
#![no_main]

#[macro_use]
extern crate log;

use alg::clock::Clock;
use alg::clock::Time;
use alg::encoder::BitmaskQuadratureSource;
use alg::encoder::Encoder;
use alg::input::{BitmaskDigitalInput, DigitalEdgeInput};
use bsp::hal::ccm;
use cortex_m::peripheral::DWT;
use embedded_hal::spi;
use imxrt_hal::gpio::GPIO;
use teensy4_bsp as bsp;

use crate::input::Inputs;
use crate::input::PinDigitalIn;
use crate::lock::Lock;
use crate::max6958::Digit;
use crate::state::OperQueue;
use crate::state::State;

mod input;
mod inter;
mod lock;
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

    let pin_clk = GPIO::new(pins.p3);
    let pin_rst = GPIO::new(pins.p4);

    let pin7 = GPIO::new(pins.p7);
    let pin8 = GPIO::new(pins.p8);

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

    spi_io.set_mode(spi::MODE_0).unwrap();
    spi_io.clear_fifo();

    let spi_lock = Lock::new(spi_io);

    let spi_cs_ext1 = GPIO::new(pins.p9).output();
    let spi_cs_ext2 = GPIO::new(pins.p10).output();

    let mut io_ext1 = mcp23s17::builder()
        .enable_all_interrupts(mcp23s17::InterruptMode::CompareAgainstPrevious)
        .build(spi_lock.clone(), spi_cs_ext1)
        .unwrap();
    let mut io_ext2 = mcp23s17::builder()
        .build(spi_lock.clone(), spi_cs_ext2)
        .unwrap();

    // Flags to indicate that an interrupt has fired that means we are to
    // read io_ext1 or io_ext2 respectively.
    let io_ext_flags = Lock::new((false, false));

    // Holders of the last reading of the io ext.
    let io_ext1_read = Lock::new(0_u16);
    let io_ext2_read = Lock::new(0_u16);

    setup_gpio_interrupts(pin7, pin8, io_ext_flags.clone());

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
    // At intensity 40 + scan limit 0123, we get 2mA per led segment.
    // 8 segments * 2mA x 4 chars = 64mA for the display.
    seg.set_scan_limit(max6958::ScanLimit::Digit0123).unwrap();
    seg.set_intensity(40).unwrap();
    seg.set_decode_mode(&[Digit::Digit0, Digit::Digit1, Digit::Digit2, Digit::Digit3])
        .unwrap();

    info!("Sure!");

    // Let's assume the u16 is transferred as:
    // [A7, A6, A5, A4,   A3, A2, A1, A0,   B7, B6, B5, B4,   B3, B2, B1, B0]

    let mut inputs = Inputs {
        /// Clock signal in. Inverted.
        clock: DigitalEdgeInput::new(PinDigitalIn(pin_clk), true),
        /// Reset signal in. Inverted.
        reset: DigitalEdgeInput::new(PinDigitalIn(pin_rst), true),

        // ext1 b0 - pin_a
        // ext1 b1 - pin_b
        seed: Encoder::new(BitmaskQuadratureSource::new(
            io_ext1_read.as_ptr(),
            0b0000_0000_0000_0001,
            0b0000_0000_0000_0010,
        )),
        // ext1 a2
        seed_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(io_ext1_read.as_ptr(), 0b0000_0100_0000_0000),
            false,
        ),

        // ext2 b0 - pin_a
        // ext2 b1 - pin_b
        length: Encoder::new(BitmaskQuadratureSource::new(
            io_ext2_read.as_ptr(),
            0b0000_0000_0000_0001,
            0b0000_0000_0000_0010,
        )),
        // ext2 a2
        length_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(io_ext2_read.as_ptr(), 0b0000_0100_0000_0000),
            false,
        ),

        // ext1 b2 - pin_a
        // ext1 b3 - pin_b
        offs1: Encoder::new(BitmaskQuadratureSource::new(
            io_ext1_read.as_ptr(),
            0b0000_0000_0000_0100,
            0b0000_0000_0000_1000,
        )),
        // ext1 a3
        offs1_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(io_ext1_read.as_ptr(), 0b0000_1000_0000_0000),
            false,
        ),
        // ext b4 - pin_a
        // ext b5 - pin_b
        step1: Encoder::new(BitmaskQuadratureSource::new(
            io_ext1_read.as_ptr(),
            0b0000_0000_0001_0000,
            0b0000_0000_0010_0000,
        )),
        // ext a4
        step1_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(io_ext1_read.as_ptr(), 0b0001_0000_0000_0000),
            false,
        ),

        // ext1 b6 - pin_a
        // ext1 b7 - pin_b
        offs2: Encoder::new(BitmaskQuadratureSource::new(
            io_ext1_read.as_ptr(),
            0b0000_0000_0100_0000,
            0b0000_0000_1000_0000,
        )),
        // ext1 a5
        offs2_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(io_ext1_read.as_ptr(), 0b0010_0000_0000_0000),
            false,
        ),
        // ext1 a0 - pin_a
        // ext1 a1 - pin_b
        step2: Encoder::new(BitmaskQuadratureSource::new(
            io_ext1_read.as_ptr(),
            0b0000_0001_0000_0000,
            0b0000_0010_0000_0000,
        )),
        // ext1 a6
        step2_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(io_ext1_read.as_ptr(), 0b0100_0000_0000_0000),
            false,
        ),

        // ext2 b2 - pin_a
        // ext2 b3 - pin_b
        offs3: Encoder::new(BitmaskQuadratureSource::new(
            io_ext2_read.as_ptr(),
            0b0000_0000_0000_0100,
            0b0000_0000_0000_1000,
        )),
        // ext2 a3
        offs3_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(io_ext2_read.as_ptr(), 0b0000_1000_0000_0000),
            false,
        ),
        // ext2 b4 - pin_a
        // ext2 b5 - pin_b
        step3: Encoder::new(BitmaskQuadratureSource::new(
            io_ext2_read.as_ptr(),
            0b0000_0000_0001_0000,
            0b0000_0000_0010_0000,
        )),
        // ext2 a4
        step3_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(io_ext2_read.as_ptr(), 0b0001_0000_0000_0000),
            false,
        ),

        // ext2 b6 - pin_a
        // ext2 b7 - pin_b
        offs4: Encoder::new(BitmaskQuadratureSource::new(
            io_ext2_read.as_ptr(),
            0b0000_0000_0100_0000,
            0b0000_0000_1000_0000,
        )),
        // ext2 a5
        offs4_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(io_ext2_read.as_ptr(), 0b0010_0000_0000_0000),
            false,
        ),
        // ext2 a0 - pin_a
        // ext2 a1 - pin_b
        step4: Encoder::new(BitmaskQuadratureSource::new(
            io_ext2_read.as_ptr(),
            0b0000_0001_0000_0000,
            0b0000_0010_0000_0000,
        )),
        // ext2 a6
        step4_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(io_ext2_read.as_ptr(), 0b0100_0000_0000_0000),
            false,
        ),
    };

    let mut start = clock.now();
    let mut loop_count = 0_u32;

    let mut state = State {
        ..Default::default()
    };

    let opers = Lock::new(OperQueue::new());

    loop {
        clock.tick();

        let now = clock.now();

        let time_lapsed = now - start;
        if time_lapsed >= Time::from_secs(10) {
            // 2021-07-01 this is: 71_424_181
            //  rotary enc decel   52_566_664
            //  after locks etc:   11_904_273 ~0.84µS per loop
            info!(
                "{} loop count: {}, {:.02?}µS/loop",
                time_lapsed,
                loop_count,
                10_000_000.0 / loop_count as f32
            );
            start = now;
            loop_count = 0;
        }

        cortex_m::interrupt::free(|cs| {
            {
                let mut flags = io_ext_flags.get(cs);

                // interrupt for io_ext1 has fired
                if flags.0 {
                    flags.0 = false;

                    let x = io_ext1.read_inputs(cs).unwrap();
                    let mut read = io_ext1_read.get(cs);

                    *read = x;
                }

                // interrupt for io_ext2 has fired
                if flags.1 {
                    flags.1 = false;

                    let x = io_ext2.read_inputs(cs).unwrap();
                    let mut read = io_ext2_read.get(cs);

                    *read = x;
                }
            }

            let mut opers = opers.get(cs);

            // Read all potential input and turn it into operations.
            inputs.tick(now, &mut opers);

            // Current length of operations.
            let len = opers.len();

            // Apply the operations to the state.
            state.update(now, opers.drain(0..len));
        });

        loop_count += 1;
    }
}

#[panic_handler]
fn panic(p: &core::panic::PanicInfo) -> ! {
    error!("{:?}", p);
    loop {}
}

use crate::inter::InterruptConfiguration;
use bsp::interrupt;
use imxrt_hal::gpio::Input;

// B1_01 - GPIO2_IO17 - ALT5
type IoExt1InterruptPin = GPIO<bsp::common::P7, Input>;
// B1_00 - GPIO2_IO16 - ALT5
type IoExt2InterruptPin = GPIO<bsp::common::P8, Input>;

pub fn setup_gpio_interrupts(
    mut pin1: IoExt1InterruptPin,
    mut pin2: IoExt2InterruptPin,
    io_ext_flags: Lock<(bool, bool)>,
) {
    use crate::inter::Interrupt;

    static mut INT: Option<(IoExt1InterruptPin, IoExt2InterruptPin, Lock<(bool, bool)>)> = None;

    #[cortex_m_rt::interrupt]
    fn GPIO2_Combined_16_31() {
        cortex_m::interrupt::free(|cs| {
            let (pin1, pin2, flags) = unsafe { INT.as_mut().unwrap() };

            let mut flags = flags.get(cs);

            if pin1.is_interrupt_status() {
                pin1.clear_interrupt_status();
                flags.0 = true;
            }

            if pin2.is_interrupt_status() {
                pin2.clear_interrupt_status();
                flags.1 = true;
            }
        });
    }

    cortex_m::interrupt::free(|_cs| {
        pin1.set_interrupt_configuration(InterruptConfiguration::RisingEdge);
        pin1.set_interrupt_enable(true);
        pin2.set_interrupt_configuration(InterruptConfiguration::RisingEdge);
        pin2.set_interrupt_enable(true);

        unsafe {
            INT = Some((pin1, pin2, io_ext_flags));
        }

        // It just so happens that both pins map to the same interrupt.
        unsafe { cortex_m::peripheral::NVIC::unmask(bsp::interrupt::GPIO2_Combined_16_31) };
    });
}
