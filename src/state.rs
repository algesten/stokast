#![allow(dead_code)]

use alg::clock::Time;
use alg::gen::{Generated, Params, STOKAST_PARAMS};
use arrayvec::ArrayVec;

use crate::CPU_SPEED;

pub const TRACK_COUNT: usize = 4;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct State {
    /// Current display mode.
    pub display: Display,

    /// Time when display was last updated. This is used to automatically
    /// switch back to the Run mode once left idle for a bit.
    pub display_last_update: Time<{ CPU_SPEED }>,

    /// Generative parameters for generated.
    pub params: Params<{ TRACK_COUNT }>,

    /// The generated tracks.
    pub generated: Generated<{ TRACK_COUNT }>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Display {
    /// Show increasing steps per incoming clock tick.
    /// This is the default we go back to after showing something else.
    Run,
    /// Seed showing 0-9999.
    Seed(u16),
    /// Length showing 2-32.
    Length(u8),
}

impl Default for Display {
    fn default() -> Self {
        Display::Run
    }
}

pub type OperQueue = ArrayVec<Oper, 64>;

/// Base for seed since starting at 0 is so boring.
const SEED_BASE: i32 = 0x616c67;

/// The operations that can be done on the state.
pub enum Oper {
    Seed(i8),
    Length(i8),
    Offset(usize, i8),
    Steps(usize, i8),
}

impl State {
    pub fn new() -> Self {
        State {
            params: STOKAST_PARAMS,
            generated: Generated::new(STOKAST_PARAMS),
            ..Default::default()
        }
    }

    pub fn update(&mut self, now: Time<{ CPU_SPEED }>, todo: impl Iterator<Item = Oper>) {
        let mut regenerate = false;
        let mut display = None;

        for oper in todo {
            match oper {
                //
                Oper::Seed(x) => {
                    let s = self.params.seed as i32 - SEED_BASE;
                    let n = s + x as i32;
                    // Seed is 0-9999
                    if n >= 0 && n <= 9999 {
                        self.params.seed = (n + SEED_BASE) as u32;
                        regenerate = true;
                        display = Some(Display::Seed(n as u16));
                    }
                }

                Oper::Length(x) => {
                    let s = self.params.pattern_length as i8;
                    let n = s + x;
                    // Patterns must be 2-64.
                    if n >= 2 && n <= 64 {
                        self.params.pattern_length = n as u8;
                        regenerate = true;
                        display = Some(Display::Length(n as u8));
                    }
                }
                Oper::Offset(i, x) => {
                    let t = &mut self.params.tracks[i];
                }
                Oper::Steps(i, x) => {
                    let t = &mut self.params.tracks[i];
                }
            }
        }

        if regenerate {
            self.generated = Generated::new(self.params);
        }

        if let Some(d) = display {
            self.display = d;
            self.display_last_update = now;
        }
    }
}
