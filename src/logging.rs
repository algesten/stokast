//! USB logging support

use bsp::hal::ral::usb::USB1;
use bsp::interrupt;
use log::LevelFilter;
use teensy4_bsp as bsp;

use crate::lock::Lock;

/// Specify any logging filters here
const FILTERS: &[bsp::usb::Filter] = &[
    // ("{{crate_name}}", None),
    // ("i2c", Some(LevelFilter::Info)),
];

/// Initialize the USB logging system, and prepares the
/// USB ISR with the poller
///
/// When `init` returns, the USB interrupt will be enabled,
/// and the host may begin to interface the device.
/// You should only call this once.
///
/// # Panics
///
/// Panics if the imxrt-ral USB1 instance is already taken.
pub fn init() -> Result<bsp::usb::Reader, bsp::usb::Error> {
    let inst = USB1::take().unwrap();
    bsp::usb::init(
        inst,
        bsp::usb::LoggingConfig {
            filters: FILTERS,
            max_level: LevelFilter::Debug,
            ..Default::default()
        },
    )
    .map(|(poller, reader)| {
        setup(poller);
        reader
    })
}

/// Setup the USB ISR with the USB poller
fn setup(poller: bsp::usb::Poller) {
    static mut POLLER: Option<Lock<bsp::usb::Poller>> = None;

    #[cortex_m_rt::interrupt]
    fn USB_OTG1() {
        cortex_m::interrupt::free(|cs| {
            let mut poller = unsafe { POLLER.as_mut().unwrap() }.get(cs);
            poller.poll();
        });
    }

    cortex_m::interrupt::free(|_cs| {
        unsafe {
            POLLER = Some(Lock::new(poller));

            // Safety: invoked in a critical section that also prepares the ISR
            // shared memory. ISR memory is ready by the time the ISR runs.
            cortex_m::peripheral::NVIC::unmask(bsp::interrupt::USB_OTG1);
        }
    });
}
