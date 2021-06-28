//! A twiddly knob encoder.

use bsp::hal::gpio::{Input, GPIO};
use cortex_m::peripheral::DWT;
use imxrt_hal::iomuxc::gpio::Pin;
use teensy4_bsp as bsp;

/// Helper do read an encoder hooked up to two GPIO inputs.
pub struct Encoder<P1, P2> {
    d1: Debouncer<P1>,
    d2: Debouncer<P2>,
    high: bool,
}

impl<P1, P2> Encoder<P1, P2>
where
    P1: Pin,
    P2: Pin,
{
    pub fn new(d1: GPIO<P1, Input>, d2: GPIO<P2, Input>, cutoff: u32) -> Self {
        Encoder {
            d1: Debouncer::new(d1, cutoff),
            d2: Debouncer::new(d2, cutoff),
            high: false,
        }
    }

    pub fn tick(&mut self) -> i8 {
        if let (Some(since1), Some(since2)) = (self.d1.tick(), self.d2.tick()) {
            if self.high {
                // already emitted. wait for signals to go low.
                0
            } else {
                self.high = true;
                if since1 < since2 {
                    -1
                } else {
                    1
                }
            }
        } else {
            self.high = false;
            0
        }
    }
}

struct Debouncer<P> {
    input: GPIO<P, Input>,
    high_since: Option<u32>,
    cutoff: u32,
}

impl<P> Debouncer<P>
where
    P: Pin,
{
    pub fn new(input: GPIO<P, Input>, cutoff: u32) -> Self {
        Debouncer {
            input,
            high_since: None,
            cutoff,
        }
    }

    pub fn tick(&mut self) -> Option<u32> {
        if self.input.is_set() {
            let now = DWT::get_cycle_count();

            if self.high_since.is_none() {
                self.high_since = Some(now);
            }

            let since = self.high_since.unwrap();

            const U31: u32 = 2_u32.pow(31);

            let diff = if since > U31 && now < U31 {
                // handle wrap-around where since hasn't
                // wrapped around and now has.
                let x = u32::MAX - since;
                now + x
            } else {
                now - since
            };

            if diff >= self.cutoff {
                self.high_since
            } else {
                None
            }
        } else {
            self.high_since = None;
            None
        }
    }
}
