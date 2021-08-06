#![no_std]
#![no_main]

#[macro_use]
extern crate log;

use alg::clock::Clock;
use alg::clock::Time;
use alg::encoder::BitmaskQuadratureSource;
use alg::encoder::Encoder;
use alg::encoder::EncoderAccelerator;
use alg::input::{BitmaskDigitalInput, DigitalEdgeInput};
use bsp::hal::ccm;
use bsp::interrupt;
use cortex_m::interrupt::CriticalSection;
use cortex_m::peripheral::DWT;
use embedded_hal::blocking::spi::{Transfer, Write};
use embedded_hal::spi;
use imxrt_hal::gpio::GPIO;
use imxrt_hal::gpio::{Input, Output};
use imxrt_hal::iomuxc::gpio::Pin;
use teensy4_bsp as bsp;

use crate::input::Inputs;
use crate::inter::InterruptConfiguration;
use crate::lock::Lock;
use crate::max6958::Segs4;
use crate::mcp23s17::Mcp23S17;
use crate::output::Gate;
use crate::output::Outputs;
use crate::state::Oper;
use crate::state::OperQueue;
use crate::state::State;

mod input;
mod inter;
mod lfo;
mod lock;
mod logging;
mod max6958;
mod mcp23s17;
mod mcp4728;
mod output;
mod state;

/// 600MHz
pub const CPU_SPEED: u32 = ccm::PLL1::ARM_HZ;

/// LED used to communicate panics etc.
type LedPcbPin = GPIO<bsp::common::P5, Output>;

static mut LED_PCB: Option<LedPcbPin> = None;

#[cortex_m_rt::entry]
fn main() -> ! {
    assert!(logging::init().is_ok());

    let mut p = bsp::Peripherals::take().unwrap();
    let mut cp = cortex_m::Peripherals::take().unwrap();
    let mut systick = bsp::SysTick::new(cp.SYST);

    // Wait so we don't miss the first log message, crashes etc.
    systick.delay(3000);

    info!("Set clock frequency to: {:?}", ccm::PLL1::ARM_HZ);

    // Set clock to the recommended 600 MHz.
    p.ccm
        .pll1
        .set_arm_clock(ccm::PLL1::ARM_HZ, &mut p.ccm.handle, &mut p.dcdc);

    info!("Enable trace and cycle counter");

    // // needed for timer
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();

    let mut clock = Clock::<_, CPU_SPEED>::new(DWT::get_cycle_count);

    let pins = bsp::t40::into_pins(p.iomuxc);

    info!("Get gate pins");

    let pin_gate1 = GPIO::new(pins.p0).output();
    let pin_gate2 = GPIO::new(pins.p1).output();
    let pin_gate3 = GPIO::new(pins.p2).output();
    let pin_gate4 = GPIO::new(pins.p3).output();

    // Flags indicating reset or clock has fired.
    let clk_flags = Lock::new((false, None));

    info!("Get rst/clk pins");

    // Reset and clock pins. Notice convention here. "clk" means the external
    // clock signal and "clock" means our internal cycle clock.
    let pin_rst = GPIO::new(pins.p20);
    let pin_clk = GPIO::new(pins.p21);

    setup_clock_interrupts(pin_rst, pin_clk, clk_flags.clone());

    // Interrupt pints for ext1 and ext2
    let ext1_irq = GPIO::new(pins.p8);
    let ext2_irq = GPIO::new(pins.p7);

    let mut led_pcb = GPIO::new(pins.p5).output();
    led_pcb.set();
    unsafe { LED_PCB = Some(led_pcb) };

    // 1.6E-5
    // 16µS

    let (_, _, _, spi4_builder) = p.spi.clock(
        &mut p.ccm.handle,
        ccm::spi::ClockSelect::Pll2,
        ccm::spi::PrescalarSelect::LPSPI_PODF_5,
    );

    // Last reading of io_ext1.
    let mut io_ext1_read = 0;
    // Last reading of io_ext2.
    let mut io_ext2_read = 0;

    // Flags to indicate that an interrupt has fired that means we are to
    // read io_ext1 or io_ext2 respectively.
    let io_ext_flags = Lock::new((false, false));

    setup_ioext_interrupts(ext1_irq, ext2_irq, io_ext_flags.clone());

    let mut spi_io = spi4_builder.build(pins.p11, pins.p12, pins.p13);

    // // From datasheet for MCP23S17 we see that max speed is 10MHz
    spi_io
        .set_clock_speed(bsp::hal::spi::ClockSpeed(5_000_000))
        .unwrap();

    spi_io.set_mode(spi::MODE_0).unwrap();
    spi_io.clear_fifo();

    let spi_lock = Lock::new(spi_io);

    let spi_cs_ext1 = GPIO::new(pins.p10).output();
    let spi_cs_ext2 = GPIO::new(pins.p9).output();

    let mut io_ext1 = mcp23s17::builder()
        .enable_all_interrupts(mcp23s17::InterruptMode::CompareAgainstPrevious)
        .set_all_pull_up(true)
        .build(spi_lock.clone(), spi_cs_ext1)
        .unwrap();
    let mut io_ext2 = mcp23s17::builder()
        .enable_all_interrupts(mcp23s17::InterruptMode::CompareAgainstPrevious)
        .set_all_pull_up(true)
        .build(spi_lock.clone(), spi_cs_ext2)
        .unwrap();

    fn verify<E, I, P>(cs: &CriticalSection, io_ext: &mut Mcp23S17<I, P>) -> Result<(), E>
    where
        I: Transfer<u16, Error = E>,
        I: Write<u16, Error = E>,
        P: Pin,
    {
        io_ext.verify_config(cs)?;
        io_ext.read_int_cap(cs)?;
        io_ext.read_inputs(cs)?;
        Ok(())
    }

    cortex_m::interrupt::free(|cs| {
        if let Err(e) = verify(cs, &mut io_ext1) {
            return Err(e);
        }
        if let Err(e) = verify(cs, &mut io_ext2) {
            return Err(e);
        }
        Ok(())
    })
    .unwrap();

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
    let mut i2c = i2c1_builder.build(pins.p19, pins.p18);

    // From datasheet MAX6958, serial max speed is 400KHz
    i2c.set_clock_speed(bsp::hal::i2c::ClockSpeed::KHz400)
        .unwrap();

    // let mut rnd = Rnd::new(1);
    let i2c_lock = Lock::new(i2c);

    let mut seg = max6958::Max6958::new(i2c_lock.clone(), max6958::Variant::A);
    let mut dac = mcp4728::Mcp4728::new(i2c_lock.clone());

    cortex_m::interrupt::free(|cs| {
        seg.set_shutdown(false, cs).unwrap();

        // At intensity 40 + scan limit 0123, we get 2mA per led segment.
        // 8 segments * 2mA x 4 chars = 64mA for the display.
        seg.set_scan_limit(max6958::ScanLimit::Digit0123, cs)
            .unwrap();
        seg.set_intensity(40, cs).unwrap();
    });

    // Let's assume the u16 is transferred as:
    // [A7, A6, A5, A4,   A3, A2, A1, A0,   B7, B6, B5, B4,   B3, B2, B1, B0]

    let mut inputs = Inputs {
        // Clock signal in. Inverted.

        // ext1 b4 - pin_a
        // ext1 a3 - pin_b
        seed: EncoderAccelerator::new(Encoder::new(BitmaskQuadratureSource::new(
            &io_ext1_read,
            0b0000_0000_0001_0000,
            0b0000_1000_0000_0000,
        ))),
        // ext1 a4
        seed_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(&io_ext1_read, 0b0001_0000_0000_0000),
            false,
        ),

        // ext2 b1 - pin_a
        // ext2 b0 - pin_b
        length: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext2_read,
            0b0000_0000_0000_0010,
            0b0000_0000_0000_0001,
        )),
        // ext2 b2
        length_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(&io_ext2_read, 0b0000_0000_0000_0100),
            false,
        ),

        // ext1 a1 - pin_a
        // ext1 b5 - pin_b
        offs1: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext1_read,
            0b0000_0010_0000_0000,
            0b0000_0000_0010_0000,
        )),
        // ext1 a2
        offs1_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(&io_ext1_read, 0b0000_0100_0000_0000),
            false,
        ),
        // ext1 b7 - pin_a
        // ext1 a0 - pin_b
        step1: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext1_read,
            0b0000_0000_1000_0000,
            0b0000_0001_0000_0000,
        )),
        // ext1 b6
        step1_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(&io_ext1_read, 0b0000_0000_0100_0000),
            false,
        ),

        // ext1 b2 - pin_a
        // ext1 b0 - pin_b
        offs2: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext1_read,
            0b0000_0000_0000_0100,
            0b0000_0000_0000_0001,
        )),
        // ext1 b1
        offs2_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(&io_ext1_read, 0b0000_0000_0000_0010),
            false,
        ),
        // ext1 a5 - pin_a
        // ext1 a6 - pin_b
        step2: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext1_read,
            0b0010_0000_0000_0000,
            0b0100_0000_0000_0000,
        )),
        // ext1 a7
        step2_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(&io_ext1_read, 0b1000_0000_0000_0000),
            false,
        ),

        // ext2 b5 - pin_a
        // ext2 b4 - pin_b
        offs3: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext2_read,
            0b0000_0000_0010_0000,
            0b0000_0000_0001_0000,
        )),
        // ext2 a1
        offs3_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(&io_ext2_read, 0b0000_0010_0000_0000),
            false,
        ),
        // ext2 b7 - pin_a
        // ext2 b6 - pin_b
        step3: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext2_read,
            0b0000_0000_1000_0000,
            0b0000_0000_0100_0000,
        )),
        // ext2 a0
        step3_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(&io_ext2_read, 0b0000_0001_0000_0000),
            false,
        ),

        // ext2 a2 - pin_a
        // ext2 a3 - pin_b
        offs4: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext2_read,
            0b0000_0100_0000_0000,
            0b0000_1000_0000_0000,
        )),
        // ext2 a4
        offs4_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(&io_ext2_read, 0b0001_0000_0000_0000),
            false,
        ),
        // ext2 a5 - pin_a
        // ext2 a6 - pin_b
        step4: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext2_read,
            0b0010_0000_0000_0000,
            0b0100_0000_0000_0000,
        )),
        // ext2 a7
        step4_btn: DigitalEdgeInput::new(
            BitmaskDigitalInput::new(&io_ext2_read, 0b1000_0000_0000_0000),
            false,
        ),
    };

    let mut outputs = Outputs {
        playhead_last: 0,
        gate1: Gate::new(pin_gate1, 50),
        gate2: Gate::new(pin_gate2, 50),
        gate3: Gate::new(pin_gate3, 50),
        gate4: Gate::new(pin_gate4, 50),
    };

    let mut start = clock.now();
    let mut loop_count = 0_u32;
    let mut last_time_update = start;
    let mut last_display_update = start;
    let mut last_segs = Segs4::new();

    // Last tick, since we want intervals.
    let mut clk_last = None;

    let mut state = State::new();

    let mut opers = OperQueue::new();

    info!("Start main loop");

    loop {
        clock.tick();

        let now = clock.now();

        let time_lapsed = now - start;
        if time_lapsed >= Time::from_secs(10) {
            // 2021-07-01 this is: 71_424_181
            //  rotary enc decel   52_566_664
            //  after locks etc:   11_904_273 0.84µS per loop
            //  io_ext_change:     20_832_340 0.48µS
            //  -- full LFO etc (now tests are with clock pulse)
            //  first optimize:     3_590_341 2.79µS
            //  minimize lfo upd:  14_310_145 0.70µS
            //
            // We know a reset happens roughly 800ns before the
            // next clock pulse.
            //
            // ----+---------------_----------+---->
            //     |               |   800ns  |
            info!(
                "{} loop count: {}, {:.02?}µS/loop",
                time_lapsed,
                loop_count,
                10_000_000.0 / loop_count as f32
            );
            info!("State: {:#?}", state);
            start = now;
            loop_count = 0;
        }

        // This is quite expensive. By doing it every 10µs we are quite confident to
        // do 4096 updates in the minimum length a track can be. Breaks down if
        // the clock pulse is very high.
        if now - last_time_update >= Time::from_micros(10) {
            last_time_update = now;
            state.update_time(now);
        }

        let mut lfo_upd = [
            state.lfo[0].tick(),
            state.lfo[1].tick(),
            state.lfo[2].tick(),
            state.lfo[3].tick(),
        ];

        let any_lfo_upd = lfo_upd.iter().any(|l| l.is_some());

        // "dirty" read whether there might be clk/rst changes.
        let clk_change = {
            let (f1, f2) = clk_flags.read();
            *f1 || f2.is_some()
        };

        // set to true if we really have an io_ext change. that way
        // we can avoid a gazillion tick() in inputs.tick().
        let mut io_ext_change = false;

        // "dirty" read whether there might be io ext input.
        let io_ext_flags_change = {
            let (f1, f2) = io_ext_flags.read();
            *f1 || *f2
        };

        // Update the display. Only do this 100Hz, if needed
        let mut display_update = false;
        if now - last_display_update >= Time::from_millis(10) {
            last_display_update = now;

            let segs = state.to_display();

            // Do we have a change in display?
            if segs != last_segs {
                display_update = true;
                last_segs = segs;
            }
        }

        // We want to avoid taking the free lock as much as possible. It costs
        // 8µS to take it, and this way we only take it if we really need to.
        if any_lfo_upd || clk_change || io_ext_flags_change || display_update {
            //
            cortex_m::interrupt::free(|cs| {
                if any_lfo_upd {
                    for (i, upd) in lfo_upd.iter_mut().enumerate() {
                        if let Some(value) = upd.take() {
                            if let Err(e) = dac.set_channel(i.into(), value, cs) {
                                error!("dac.set_channel: {:?}", e);
                            }
                        }
                    }
                }

                {
                    let mut flags = clk_flags.get(cs);

                    // Deliberately read reset before clock, since if we for some reason end up
                    // reading both reset and clock in the same cycle, we must handle the reset
                    // before the clock pulse.
                    if flags.0 {
                        opers.push(Oper::Reset);
                        flags.0 = false;
                    }

                    if let Some(cycle_count) = flags.1.take() {
                        if let Some(last) = clk_last {
                            let interval = clock.time_relative(cycle_count) - last;
                            opers.push(Oper::Tick(interval));
                        }
                        clk_last = Some(now);
                    }
                }

                {
                    let mut flags = io_ext_flags.get(cs);

                    if flags.0 {
                        flags.0 = false;

                        let x = !io_ext1.read_inputs(cs).unwrap();

                        if x != io_ext1_read {
                            debug!("ext1 reading: {:016b}", x);
                            io_ext1_read = x;
                            io_ext_change = true;
                        }
                    }

                    // interrupt for io_ext2 has fired
                    if flags.1 {
                        flags.1 = false;

                        let x = !io_ext2.read_inputs(cs).unwrap();

                        if x != io_ext2_read {
                            debug!("ext2 reading: {:016b}", x);
                            io_ext2_read = x;
                            io_ext_change = true;
                        }
                    }
                }

                if display_update {
                    seg.set_segs(last_segs, cs).unwrap();
                }
            });
        }

        // Read all potential input and turn it into operations.
        inputs.tick(now, &mut opers, io_ext_change);

        // Current length of operations.
        let len = opers.len();

        if len > 0 {
            // Apply the operations to the state.
            state.update(now, opers.drain(0..len));
        }

        // Update output gates.
        outputs.tick(now, &state);

        loop_count += 1;
    }
}

#[panic_handler]
fn panic(p: &core::panic::PanicInfo) -> ! {
    error!("{:?}", p);

    // Might as well take it, we're not going to resume.
    let mut led = unsafe { LED_PCB.take().unwrap() };

    loop {
        led.clear();
        delay(1);
        led.set();
        delay(1);
    }
}

fn delay(factor: u32) {
    for _ in 0..(factor * 50_000_000) {
        core::hint::spin_loop();
    }
}

// B1_00 - GPIO2_IO16 - ALT5
type IoExt1InterruptPin = GPIO<bsp::common::P8, Input>;
// B1_01 - GPIO2_IO17 - ALT5
type IoExt2InterruptPin = GPIO<bsp::common::P7, Input>;

pub fn setup_ioext_interrupts(
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
        info!("setup GPIO IoExt interrupts");

        pin1.set_interrupt_configuration(InterruptConfiguration::RisingEdge);
        pin1.set_interrupt_enable(true);
        pin1.clear_interrupt_status();
        pin2.set_interrupt_configuration(InterruptConfiguration::RisingEdge);
        pin2.set_interrupt_enable(true);
        pin2.clear_interrupt_status();

        unsafe {
            INT = Some((pin1, pin2, io_ext_flags));
        }

        // It just so happens that both pins map to the same interrupt.
        unsafe { cortex_m::peripheral::NVIC::unmask(bsp::interrupt::GPIO2_Combined_16_31) };
    });
}

// B1_10 GPIO1_IO26 - ALT5
type ResetInterruptPin = GPIO<bsp::common::P20, Input>;
// B1_11 - GPIO1_IO27 - ALT5
type ClockInterruptPin = GPIO<bsp::common::P21, Input>;

pub fn setup_clock_interrupts(
    mut rst: ResetInterruptPin,
    mut clk: ClockInterruptPin,
    clk_flags: Lock<(bool, Option<u32>)>,
) {
    use crate::inter::Interrupt;

    static mut INT: Option<(
        ResetInterruptPin,
        ClockInterruptPin,
        Lock<(bool, Option<u32>)>,
    )> = None;

    #[cortex_m_rt::interrupt]
    fn GPIO1_Combined_16_31() {
        cortex_m::interrupt::free(|cs| {
            let (rst, clk, flags) = unsafe { INT.as_mut().unwrap() };

            let mut flags = flags.get(cs);

            if rst.is_interrupt_status() {
                rst.clear_interrupt_status();
                flags.0 = true;
            }

            if clk.is_interrupt_status() {
                clk.clear_interrupt_status();
                // Store the cycle count when the interrupt fires, will be used
                // as a time code in the main loop.
                flags.1 = Some(DWT::get_cycle_count());
            }
        });
    }

    cortex_m::interrupt::free(|_cs| {
        info!("setup GPIO Clock interrupts");

        // falling since inverted
        rst.set_interrupt_configuration(InterruptConfiguration::FallingEdge);
        rst.set_interrupt_enable(true);
        rst.clear_interrupt_status();
        clk.set_interrupt_configuration(InterruptConfiguration::FallingEdge);
        clk.set_interrupt_enable(true);
        clk.clear_interrupt_status();

        unsafe {
            INT = Some((rst, clk, clk_flags));
        }

        unsafe { cortex_m::peripheral::NVIC::unmask(bsp::interrupt::GPIO1_Combined_16_31) };
    });
}
