//! 12-bit LFO with different modes.

use alg::geom::{sin, tri};
use alg::rnd::Rnd;

#[derive(Debug, Clone, Default)]
/// A 12-bit LFO.
pub struct Lfo {
    offset: u32,
    prev: u16,
    mode: Mode,

    rnd_seed: u32,
    steps: u8,

    last: u16,
    next: Option<u16>,
}

impl Lfo {
    pub fn set_offset(&mut self, offset: u32) {
        self.offset = offset;

        self.update();
    }

    pub fn set_seed_steps(&mut self, rnd_seed: u32, steps: u8) {
        self.rnd_seed = rnd_seed;
        self.steps = steps;

        self.update();
    }

    pub fn set_mode(&mut self, d: i8) {
        let mut n = self.mode as i8 + d;

        if n < 0 {
            n += Mode::len() as i8;
        }

        self.mode = n.into();

        self.update();
    }

    fn update(&mut self) {
        let n = self.mode.output(self.offset, self.rnd_seed, self.steps);
        if n != self.last {
            self.last = n;
            self.next = Some(n);
        }
    }

    pub fn tick(&mut self) -> Option<u16> {
        self.next.take()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Random = 0,
    SawUp = 1,
    SawDown = 2,
    Sine = 3,
    Sine90 = 4,
    Sine180 = 5,
    Triangle = 6,
    Triangle90 = 7,
    Triangle180 = 8,
    Square = 9,
    Square90 = 10,
    Square180 = 11,
}

impl Default for Mode {
    fn default() -> Self {
        Self::Random
    }
}

impl Mode {
    pub const fn len() -> usize {
        12
    }

    pub fn output(&self, offset: u32, rnd_seed: u32, steps: u8) -> u16 {
        match self {
            Mode::Random => {
                // TODO cpu cycles can be saved here.
                let mut rnd = Rnd::new(rnd_seed);

                let x = offset / (u32::MAX / steps as u32);

                let mut n = 0;
                for _ in 0..(x + 1) {
                    n = rnd.next();
                }

                (n >> 16) as u16
            }

            Mode::SawUp => saw_12(offset),
            Mode::SawDown => saw_12(u32::MAX - offset),

            Mode::Sine => sin_12(offset),
            Mode::Sine90 => sin_12(offset.wrapping_add(u32::MAX / 4)),
            Mode::Sine180 => sin_12(offset.wrapping_add(u32::MAX / 2)),

            Mode::Triangle => tri_12(offset),
            Mode::Triangle90 => tri_12(offset.wrapping_add(u32::MAX / 4)),
            Mode::Triangle180 => tri_12(offset.wrapping_add(u32::MAX / 2)),

            Mode::Square => sqr_12(offset),
            Mode::Square90 => sqr_12(offset.wrapping_add(u32::MAX / 4)),
            Mode::Square180 => sqr_12(offset.wrapping_add(u32::MAX / 2)),
        }
    }
}

fn sin_12(offset: u32) -> u16 {
    ((0x8000 + sin(offset) as i32) >> 4) as u16
}

fn saw_12(offset: u32) -> u16 {
    // we need a 12 bit output. that's the 12 highest bits in a 32 bit max.
    const DELTA: u32 = 1 << (32 - 12);

    let n = offset / DELTA;

    (n >> 16) as u16
}

fn tri_12(offset: u32) -> u16 {
    ((0x8000 + tri(offset) as i32) >> 4) as u16
}

fn sqr_12(offset: u32) -> u16 {
    if offset < u32::MAX / 2 {
        0xfff
    } else {
        0
    }
}

impl From<i8> for Mode {
    fn from(v: i8) -> Self {
        use Mode::*;
        match v % (Mode::len() as i8) {
            0 => Random,
            1 => SawUp,
            2 => SawDown,
            3 => Sine,
            4 => Sine90,
            5 => Sine180,
            6 => Triangle,
            7 => Triangle90,
            8 => Triangle180,
            9 => Square,
            10 => Square90,
            11 => Square180,
            _ => panic!("Unhandled Mode number"),
        }
    }
}
