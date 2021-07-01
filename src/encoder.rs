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

use alg::Time;
use bsp::hal::gpio::{Input, GPIO};
use imxrt_hal::iomuxc::gpio::Pin;
use teensy4_bsp as bsp;

use crate::CPU_SPEED;

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
}

pub trait EncoderTick {
    fn tick(&mut self, now: Time<{ CPU_SPEED }>) -> i8;
}

impl<PA, PB> EncoderTick for Encoder<PA, PB>
where
    PA: Pin,
    PB: Pin,
{
    fn tick(&mut self, _now: Time<{ CPU_SPEED }>) -> i8 {
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

pub struct EncoderAccelerator<E> {
    /// Encoder to read impulses from.a
    encoder: E,
    /// This keeps the previous reading.,
    prev: Reading,
    /// Current speed in millionths per milliseconds.
    speed: u32,
    /// Accumulator of millionths.
    acc: u32,
    /// When we last emitted a tick value.
    last_emit: Time<{ CPU_SPEED }>,
}

/// Deceleration in millionths per millisecond
const DECELERATION: u32 = 500;

#[derive(Clone, Copy)]
struct Reading(Time<{ CPU_SPEED }>, i8);

impl<E> EncoderAccelerator<E>
where
    E: EncoderTick,
{
    pub fn new(encoder: E) -> Self {
        EncoderAccelerator {
            encoder,
            prev: Reading(Time::new(0), 0),
            speed: 0,
            acc: 0,
            last_emit: Time::new(0),
        }
    }
}

impl<E> EncoderTick for EncoderAccelerator<E>
where
    E: EncoderTick,
{
    fn tick(&mut self, now: Time<{ CPU_SPEED }>) -> i8 {
        let direction = self.encoder.tick(now);

        if direction != 0 {
            let reading = Reading(now, direction);

            let dt = reading.0 - self.prev.0;

            // speed in millionths per millisecond
            let speed = if reading.1.signum() != self.prev.1.signum() {
                // direction change, however ignore if it happens too fast (due to shitty encoders).
                if dt.subsec_micros() < 2000 {
                    return 0;
                } else {
                    0
                }
            } else {
                // calculate new speed
                if dt.seconds() > 0 {
                    // too slow to impact speed
                    0
                } else if dt.subsec_millis() == 0 {
                    1000_000
                } else {
                    (1000_000 / dt.subsec_millis()) as u32
                }
            };

            if speed > self.speed || speed == 0 {
                info!("speed {}", speed);
                self.speed = speed;
            }

            self.prev = reading;
            self.last_emit = now;

            // rotary movements always affect the reading. emit directly.
            return direction;
        }

        if now - self.last_emit >= Time::from_millis(1) {
            self.last_emit = now; // might not emit actually.

            if self.speed > DECELERATION {
                self.speed -= DECELERATION;
            } else {
                self.speed = 0;
            }

            // build up in the accumulator the millionths.
            self.acc += self.speed;

            if self.acc >= 1000_000 {
                self.acc = self.acc % 1000_000;

                // this will be the correct direction.
                return self.prev.1;
            }
        }

        if self.speed == 0 {
            self.acc = 0;
        }

        0
    }
}
