use arrayvec::ArrayVec;
use bsp::interrupt;
use imxrt_hal::gpio::Input;
use imxrt_hal::gpio::GPIO;
use imxrt_hal::iomuxc::prelude::consts;
use imxrt_hal::spi::SPI;
use teensy4_bsp as bsp;

use crate::inter::Interrupt;
use crate::inter::InterruptConfiguration;
use crate::lock::Lock;
use crate::mcp23s17::Mcp23S17;

// B1_00 - GPIO2_IO16 - ALT5
type IoExt1InterruptPin = GPIO<bsp::common::P8, Input>;
// B1_01 - GPIO2_IO17 - ALT5
type IoExt2InterruptPin = GPIO<bsp::common::P7, Input>;

pub type IoExtReads = ArrayVec<u16, 64>;

pub fn setup_gpio_interrupts(
    mut pin1: IoExt1InterruptPin,
    mut pin2: IoExt2InterruptPin,
    io_ext1: Mcp23S17<SPI<consts::U4>, bsp::common::P10>,
    io_ext2: Mcp23S17<SPI<consts::U4>, bsp::common::P9>,
    io_ext_reads: Lock<(IoExtReads, IoExtReads)>,
) {
    static mut INT: Option<(
        IoExt1InterruptPin,
        IoExt2InterruptPin,
        Mcp23S17<SPI<consts::U4>, bsp::common::P10>,
        Mcp23S17<SPI<consts::U4>, bsp::common::P9>,
        Lock<(IoExtReads, IoExtReads)>,
    )> = None;

    #[cortex_m_rt::interrupt]
    fn GPIO2_Combined_16_31() {
        cortex_m::interrupt::free(|cs| {
            let (pin1, pin2, io_ext1, io_ext2, reads) = unsafe { INT.as_mut().unwrap() };

            let mut reads = reads.get(cs);

            if pin1.is_interrupt_status() {
                pin1.clear_interrupt_status();
                let x = !io_ext1.read_int_cap(cs).unwrap();
                let y = !io_ext1.read_inputs(cs).unwrap();

                let did_change = reads.0.last().map(|l| *l == x).unwrap_or(true);
                if did_change {
                    reads.0.push(x);
                }

                if y != x {
                    reads.0.push(y);
                }
            }

            if pin2.is_interrupt_status() {
                pin2.clear_interrupt_status();
                let x = !io_ext2.read_int_cap(cs).unwrap();
                let y = !io_ext2.read_inputs(cs).unwrap();

                let did_change = reads.1.last().map(|l| *l == x).unwrap_or(true);
                if did_change {
                    reads.1.push(x);
                }

                if y != x {
                    reads.1.push(y);
                }
            }
        });
    }

    cortex_m::interrupt::free(|_cs| {
        info!("setup GPIO interrupts");

        pin1.set_interrupt_configuration(InterruptConfiguration::RisingEdge);
        pin1.set_interrupt_enable(true);
        pin1.clear_interrupt_status();
        pin2.set_interrupt_configuration(InterruptConfiguration::RisingEdge);
        pin2.set_interrupt_enable(true);
        pin2.clear_interrupt_status();

        unsafe {
            INT = Some((pin1, pin2, io_ext1, io_ext2, io_ext_reads));
        }

        // It just so happens that both pins map to the same interrupt.
        unsafe { cortex_m::peripheral::NVIC::unmask(bsp::interrupt::GPIO2_Combined_16_31) };
    });
}
