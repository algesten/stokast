//! 12-bit LFO with different modes.

#[derive(Debug, Clone, Default)]
/// A 12-bit LFO.
pub struct Lfo {
    offset: u16,
    prev: u16,
    mode: Mode,
    rnd_seed: u32,
}

impl Lfo {
    pub fn new() -> Self {
        Lfo {
            ..Default::default()
        }
    }

    pub fn set_seed(&mut self, rnd_seed: u32) {
        self.rnd_seed = rnd_seed;
    }

    pub fn update_mode(&mut self, d: i8) {
        let mut n = self.mode as i8 + d;

        if n < 0 {
            n += Mode::len() as i8;
        }

        self.mode = n.into();
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
