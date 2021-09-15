//! 12-bit LFO with different modes.

use core::fmt::Debug;

use alg::geom::{sin, tri};
use alg::rnd::Rnd;

use crate::max6958::Seg;
use crate::max6958::Segs;

#[derive(Debug, Clone)]
/// A 12-bit LFO.
pub struct Lfo {
    offset: u32,
    prev: u16,
    pub mode: Mode,
    gate_high: bool,

    rnd: RndStep,
    length: u8,

    last: u16,
    next: Option<u16>,
}

#[derive(Clone)]
struct RndStep([u32; 64]);

impl Debug for RndStep {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "RndStep")
    }
}

impl Default for Lfo {
    fn default() -> Self {
        Lfo {
            offset: 0,
            prev: 0,
            mode: Mode::Random,
            gate_high: false,
            rnd: RndStep([0; 64]),
            length: 2,
            last: 0,
            next: None,
        }
    }
}

impl Lfo {
    pub fn set_offset(&mut self, offset: u32) {
        self.offset = offset;

        self.update(false);
    }

    pub fn set_seed_length(&mut self, rnd_seed: u32, length: u8) {
        self.length = length;

        let mut rnd = Rnd::new(rnd_seed);
        for i in 0..self.rnd.0.len() {
            self.rnd.0[i] = rnd.next();
        }

        self.update(false);
    }

    pub fn set_mode(&mut self, d: i8) {
        let mut n = self.mode as i8 + d;

        if n < 0 {
            n += Mode::len() as i8;
        }

        self.mode = n.into();

        self.update(false);
    }

    pub fn set_gate_high(&mut self, high: bool) {
        if self.gate_high != high {
            self.update(!self.gate_high && high);
            self.gate_high = high;
        }
    }

    fn update(&mut self, gate_rise: bool) {
        if self.mode == Mode::Random {
            // Random mode is tied to gate changing to high.
            if !gate_rise {
                return;
            }
            let n = self.mode.output(self.offset, &self.rnd.0, self.length);
            self.next = Some(n);
        } else {
            let n = self.mode.output(self.offset, &self.rnd.0, self.length);

            if n != self.last {
                self.last = n;
                self.next = Some(n);
            }
        }
    }

    pub fn tick(&mut self) -> Option<u16> {
        self.next.take()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    fn output(&self, offset: u32, rnd: &[u32], length: u8) -> u16 {
        assert!(length > 0);

        match self {
            Mode::Random => {
                let x = (offset / (u32::MAX / (length - 1) as u32)) as usize;

                let n = rnd[x];

                (n >> 20) as u16
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

    (n & 0xffff) as u16
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

pub const SAW_UP: Segs<5> = Segs([
    0,
    //  -
    Seg::SegA as u8,
    //  _|
    Seg::CornerMru as u8,
    //  -
    Seg::SegG as u8,
    //  _|
    Seg::CornerBr as u8,
]);

pub const SAW_DN: Segs<5> = Segs([
    0,
    // _
    Seg::SegD as u8,
    // -|
    Seg::CornerMrd as u8,
    // -
    Seg::SegG as u8,
    // -|
    Seg::CornerTr as u8,
]);
