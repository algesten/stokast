#![no_std]
#![no_main]

#[macro_use]
extern crate log;

use alg::clock::Clock;
use alg::clock::Time;
use alg::encoder::BitmaskQuadratureSource;
use alg::encoder::Encoder;
use alg::encoder::EncoderAccelerator;
use alg::input::{BitmaskDigitalInput, DigitalInput};
use bsp::hal::ccm;
use cortex_m::interrupt::CriticalSection;
use cortex_m::peripheral::DWT;
use embedded_hal::blocking::spi::{Transfer, Write};
use embedded_hal::spi;
use imxrt_hal::gpio::Output;
use imxrt_hal::gpio::GPIO;
use imxrt_hal::iomuxc::gpio::Pin;
use teensy4_bsp as bsp;

use crate::error::Error;
use crate::input::Inputs;
use crate::input::PinDigitalIn;
use crate::irq::setup_gpio_interrupts;
use crate::irq::IoExtReads;
use crate::lock::Lock;
use crate::max6958::Segs4;
use crate::mcp23s17::Mcp23S17;
use crate::output::Gate;
use crate::output::Outputs;
use crate::state::OperQueue;
use crate::state::State;

mod error;
mod input;
mod inter;
mod irq;
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
    if let Err(e) = do_run() {
        panic!("main failed: {:?}", e);
    }

    unreachable!();
}

fn do_run() -> Result<(), Error> {
    // this fails if there is no USB connected. To get it working,
    // connect the USB and power cycle.
    let _ = logging::init();

    let mut p = bsp::Peripherals::take().unwrap();
    let mut cp = cortex_m::Peripherals::take().unwrap();
    let mut systick = bsp::SysTick::new(cp.SYST);

    // Wait so we don't miss the first log message, crashes etc.
    systick.delay(1000);

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

    info!("Get rst/clk pins");

    // Reset and clock pins. Notice convention here. "clk" means the external
    // clock signal and "clock" means our internal cycle clock.
    let pin_rst = GPIO::new(pins.p20);
    let pin_clk = GPIO::new(pins.p21);

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

    // Last reading to proces of io_ext1.
    let mut io_ext1_read = 0;
    // Last reading to process of io_ext2.
    let mut io_ext2_read = 0;

    // Flags to indicate that an interrupt has fired that means we are to
    // read io_ext1 or io_ext2 respectively.
    let io_ext_reads = Lock::new((IoExtReads::new(), IoExtReads::new()));

    let mut spi_io = spi4_builder.build(pins.p11, pins.p12, pins.p13);

    // // From datasheet for MCP23S17 we see that max speed is 10MHz
    spi_io.set_clock_speed(bsp::hal::spi::ClockSpeed(5_000_000))?;

    spi_io.set_mode(spi::MODE_0)?;
    spi_io.clear_fifo();

    let spi_lock = Lock::new(spi_io);

    let spi_cs_ext1 = GPIO::new(pins.p10).output();
    let spi_cs_ext2 = GPIO::new(pins.p9).output();

    let mut io_ext1 = mcp23s17::builder()
        .enable_all_interrupts(mcp23s17::InterruptMode::CompareAgainstPrevious)
        .set_all_pull_up(true)
        .build(spi_lock.clone(), spi_cs_ext1)?;
    let mut io_ext2 = mcp23s17::builder()
        .enable_all_interrupts(mcp23s17::InterruptMode::CompareAgainstPrevious)
        .set_all_pull_up(true)
        .build(spi_lock.clone(), spi_cs_ext2)?;

    fn verify<E, I, P>(cs: &CriticalSection, io_ext: &mut Mcp23S17<I, P>) -> Result<(), Error>
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
        verify(cs, &mut io_ext1)?;
        verify(cs, &mut io_ext2)?;
        Ok::<_, Error>(())
    })?;

    setup_gpio_interrupts(ext1_irq, ext2_irq, io_ext1, io_ext2, io_ext_reads.clone());

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
    i2c.set_clock_speed(bsp::hal::i2c::ClockSpeed::KHz400)?;

    // let mut rnd = Rnd::new(1);
    let i2c_lock = Lock::new(i2c);

    let mut seg = max6958::Max6958::new(i2c_lock.clone(), max6958::Variant::A);
    let mut dac = mcp4728::Mcp4728::new(i2c_lock.clone());

    cortex_m::interrupt::free(|cs| {
        seg.set_shutdown(false, cs)?;

        // At intensity 40 + scan limit 0123, we get 2mA per led segment.
        // 8 segments * 2mA x 4 chars = 64mA for the display.
        seg.set_scan_limit(max6958::ScanLimit::Digit0123, cs)?;

        seg.set_intensity(40, cs)?;

        Ok::<_, Error>(())
    })?;

    // Let's assume the u16 is transferred as:
    // [A7, A6, A5, A4,   A3, A2, A1, A0,   B7, B6, B5, B4,   B3, B2, B1, B0]

    let mut inputs = Inputs {
        // Clock signal in. Inverted.
        clock: PinDigitalIn(pin_clk).edge(),
        // Last tick, since we want intervals.
        clock_last: None,

        // Reset signal in. Inverted.
        reset: PinDigitalIn(pin_rst).edge(),

        // ext1 b4 - pin_a
        // ext1 a3 - pin_b
        seed: EncoderAccelerator::new(Encoder::new(BitmaskQuadratureSource::new(
            &io_ext1_read,
            0b0000_0000_0001_0000,
            0b0000_1000_0000_0000,
        ))),
        // ext1 a4
        seed_btn: BitmaskDigitalInput::new(&io_ext1_read, 0b0001_0000_0000_0000)
            .debounce()
            .edge(),

        // ext2 b1 - pin_a
        // ext2 b0 - pin_b
        length: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext2_read,
            0b0000_0000_0000_0010,
            0b0000_0000_0000_0001,
        )),
        // ext2 b2
        length_btn: BitmaskDigitalInput::new(&io_ext2_read, 0b0000_0000_0000_0100)
            .debounce()
            .edge(),

        // ext1 a1 - pin_a
        // ext1 b5 - pin_b
        offs1: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext1_read,
            0b0000_0010_0000_0000,
            0b0000_0000_0010_0000,
        )),
        // ext1 a2
        offs1_btn: BitmaskDigitalInput::new(&io_ext1_read, 0b0000_0100_0000_0000)
            .debounce()
            .edge(),

        // ext1 b7 - pin_a
        // ext1 a0 - pin_b
        step1: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext1_read,
            0b0000_0000_1000_0000,
            0b0000_0001_0000_0000,
        )),
        // ext1 b6
        step1_btn: BitmaskDigitalInput::new(&io_ext1_read, 0b0000_0000_0100_0000)
            .debounce()
            .edge(),

        // ext1 b2 - pin_a
        // ext1 b0 - pin_b
        offs2: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext1_read,
            0b0000_0000_0000_0100,
            0b0000_0000_0000_0001,
        )),
        // ext1 b1
        offs2_btn: BitmaskDigitalInput::new(&io_ext1_read, 0b0000_0000_0000_0010)
            .debounce()
            .edge(),

        // ext1 a5 - pin_a
        // ext1 a6 - pin_b
        step2: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext1_read,
            0b0010_0000_0000_0000,
            0b0100_0000_0000_0000,
        )),
        // ext1 a7
        step2_btn: BitmaskDigitalInput::new(&io_ext1_read, 0b1000_0000_0000_0000)
            .debounce()
            .edge(),

        // ext2 b5 - pin_a
        // ext2 b4 - pin_b
        offs3: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext2_read,
            0b0000_0000_0010_0000,
            0b0000_0000_0001_0000,
        )),
        // ext2 a1
        offs3_btn: BitmaskDigitalInput::new(&io_ext2_read, 0b0000_0010_0000_0000)
            .debounce()
            .edge(),

        // ext2 b7 - pin_a
        // ext2 b6 - pin_b
        step3: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext2_read,
            0b0000_0000_1000_0000,
            0b0000_0000_0100_0000,
        )),
        // ext2 a0
        step3_btn: BitmaskDigitalInput::new(&io_ext2_read, 0b0000_0001_0000_0000)
            .debounce()
            .edge(),

        // ext2 a2 - pin_a
        // ext2 a3 - pin_b
        offs4: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext2_read,
            0b0000_0100_0000_0000,
            0b0000_1000_0000_0000,
        )),
        // ext2 a4
        offs4_btn: BitmaskDigitalInput::new(&io_ext2_read, 0b0001_0000_0000_0000)
            .debounce()
            .edge(),

        // ext2 a5 - pin_a
        // ext2 a6 - pin_b
        step4: Encoder::new(BitmaskQuadratureSource::new(
            &io_ext2_read,
            0b0010_0000_0000_0000,
            0b0100_0000_0000_0000,
        )),
        // ext2 a7
        step4_btn: BitmaskDigitalInput::new(&io_ext2_read, 0b1000_0000_0000_0000)
            .debounce()
            .edge(),
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

        let lfo_upd = [
            state.lfo[0].tick(),
            state.lfo[1].tick(),
            state.lfo[2].tick(),
            state.lfo[3].tick(),
        ];

        let any_lfo_upd = lfo_upd.iter().any(|l| l.is_some());

        // set to true if we really have an io_ext change. that way
        // we can avoid a gazillion tick() in inputs.tick().
        let mut io_ext_change = false;
        let io_ext_reads_ro = io_ext_reads.read();
        let got_io_ext1_reads = !io_ext_reads_ro.0.is_empty();
        let got_io_ext2_reads = !io_ext_reads_ro.1.is_empty();

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
        if any_lfo_upd || got_io_ext1_reads || got_io_ext2_reads || display_update {
            //
            cortex_m::interrupt::free(|cs| {
                if any_lfo_upd {
                    dac.set_channels(&lfo_upd, cs)?;
                }

                {
                    let mut reads = io_ext_reads.get(cs);

                    if got_io_ext1_reads {
                        let x = reads.0.remove(0);

                        if x != io_ext1_read {
                            debug!("ext1 reading: {:016b}", x);
                            io_ext1_read = x;
                            io_ext_change = true;
                        }
                    }

                    // interrupt for io_ext2 has fired
                    if got_io_ext2_reads {
                        let x = reads.1.remove(0);

                        if x != io_ext2_read {
                            debug!("ext2 reading: {:016b}", x);
                            io_ext2_read = x;
                            io_ext_change = true;
                        }
                    }
                }

                if display_update {
                    seg.set_segs(last_segs, cs)?;
                }

                Ok::<_, Error>(())
            })?;
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

        // Propagate output gate states to LFOs.
        state.lfo[0].set_gate_high(outputs.gate1.is_high());
        state.lfo[1].set_gate_high(outputs.gate2.is_high());
        state.lfo[2].set_gate_high(outputs.gate3.is_high());
        state.lfo[3].set_gate_high(outputs.gate4.is_high());

        loop_count += 1;
    }
}

#[panic_handler]
fn panic(p: &core::panic::PanicInfo) -> ! {
    // since usb debugging requires the interrupts to work, we re-enable them here.
    // this should be safe since the "main" stack pointer is gone.
    unsafe { cortex_m::interrupt::enable() };

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
