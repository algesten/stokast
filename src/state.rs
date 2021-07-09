#![allow(dead_code)]

use alg::clock::Time;
use alg::gen::{Generated, Params, STOKAST_PARAMS};
use arrayvec::ArrayVec;
use core::cell::RefCell;
use cortex_m::interrupt::Mutex;

use crate::CPU_SPEED;

pub static OPER_QUEUE: Mutex<RefCell<Option<OperQueue>>> = Mutex::new(RefCell::new(None));

pub const TRACK_COUNT: usize = 4;

/// App state
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

    /// Current play head. Goes from 0..(params.pattern_length - 1)
    pub play_head: usize,

    /// If next tick is going to reset back to 0.
    pub next_is_reset: bool,
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
    Tick,
    Reset,
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
                Oper::Tick => {
                    self.play_head = if self.next_is_reset {
                        self.next_is_reset = false;

                        0
                    } else {
                        let n = self.play_head + 1;

                        // play_head goes from 0..(pattern_length - 1).
                        if n as u8 >= self.params.pattern_length {
                            0
                        } else {
                            n
                        }
                    };
                }

                Oper::Reset => {
                    // Whatever tick is coming next, it's going to reset back to 0.
                    self.next_is_reset = true;
                }

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
                Oper::Offset(i, _x) => {
                    let _t = &mut self.params.tracks[i];
                }
                Oper::Steps(i, _x) => {
                    let _t = &mut self.params.tracks[i];
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
