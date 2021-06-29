//! A twiddly knob encoder reader.
//!
//! ```ignore
//!    +----+    +----+    
//!    |    |    |    |       A
//!  --+    +----+    +----
//!       +----+    +----+    
//!       |    |    |    |    B
//!   ----+    +----+    +--
//!        ^  ^ ^  ^
//!        1  2 3  4
//! ```
//!
//! The states are `AB`:
//!
//! 1. is `11`
//! 2. is `01`
//! 3. is `00`
//! 4. is `10`
//!
//! The only valid transitions clock-wise are:
//!
//! * `11` -> `01`
//! * `01` -> `00`
//! * `00` -> `10`
//! * `10` -> `11`
//!
//! And the reverse counter clock wise.
//!
//! * `01` -> `11`
//! * `00` -> `01`
//! * `10` -> `00`
//! * `11` -> `10`
//!
//! We can make pairs to create a lookup table of valid pairs `1101`, `0100`, and -1 or 1 to denote direction.

const TABLE: [i8; 16] = [
    0,  // 0000
    -1, // 0001
    1,  // 0010
    0,  // 0011
    1,  // 0100
    0,  // 0101
    0,  // 0110
    -1, // 0111
    -1, // 1000
    0,  // 1001
    0,  // 1010
    1,  // 1011
    0,  // 1100
    1,  // 1101
    -1, // 1110
    0,  // 1111
];

use bsp::hal::gpio::{Input, GPIO};
use imxrt_hal::iomuxc::gpio::Pin;
use teensy4_bsp as bsp;

/// Helper do read an encoder hooked up to two GPIO inputs.
pub struct Encoder<PA, PB> {
    pin_a: GPIO<PA, Input>,
    pin_b: GPIO<PB, Input>,
    prev_next: u8,
    state: u8,
}

impl<PA, PB> Encoder<PA, PB>
where
    PA: Pin,
    PB: Pin,
{
    pub fn new(pin_a: GPIO<PA, Input>, pin_b: GPIO<PB, Input>) -> Self {
        Encoder {
            pin_a,
            pin_b,
            prev_next: 0,
            state: 0,
        }
    }

    pub fn tick(&mut self) -> i8 {
        // Rotate up last read 2 bits and discard the rest.
        self.prev_next = (self.prev_next << 2) & 0b1100;

        if self.pin_a.is_set() {
            self.prev_next |= 0b10;
        }

        if self.pin_b.is_set() {
            self.prev_next |= 0b01;
        }

        let direction = TABLE[self.prev_next as usize];

        if direction != 0 {
            // Move current state up to make state for new, and put in the new.
            self.state = (self.state << 4) | self.prev_next;

            // A CCW rotation will always pass through this state
            if self.state == 0b1110_1000 {
                return -1;
            }

            // A CW rotation will always pass through this state
            if self.state == 0b1101_0100 {
                return 1;
            }
        }

        0
    }
}

// use cortex_m::peripheral::DWT;
// struct Debouncer<P> {
//     input: GPIO<P, Input>,
//     high_since: Option<u32>,
//     cutoff: u32,
// }

// impl<P> Debouncer<P>
// where
//     P: Pin,
// {
//     pub fn new(input: GPIO<P, Input>, cutoff: u32) -> Self {
//         Debouncer {
//             input,
//             high_since: None,
//             cutoff,
//         }
//     }

//     pub fn tick(&mut self) -> Option<u32> {
//         if self.input.is_set() {
//             let now = DWT::get_cycle_count();

//             if self.high_since.is_none() {
//                 self.high_since = Some(now);
//             }

//             let since = self.high_since.unwrap();

//             const U31: u32 = 2_u32.pow(31);

//             let diff = if since > U31 && now < U31 {
//                 // handle wrap-around where since hasn't
//                 // wrapped around and now has.
//                 let x = u32::MAX - since;
//                 now + x
//             } else {
//                 now - since
//             };

//             if diff >= self.cutoff {
//                 self.high_since
//             } else {
//                 None
//             }
//         } else {
//             self.high_since = None;
//             None
//         }
//     }
// }
