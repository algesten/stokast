#![allow(dead_code)]

use alg::clock::Time;
use alg::gen::{Generated, Params, SEED_BASE, STOKAST_PARAMS};
use alg::rnd::Rnd;
use alg::tempo::Tempo;
use arrayvec::ArrayVec;

use crate::lfo::Lfo;
use crate::CPU_SPEED;

pub const TRACK_COUNT: usize = 4;

/// App state
#[derive(Debug, Default, Clone)]
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

    /// The LFOs.
    pub lfo: [Lfo; TRACK_COUNT],

    /// Current global playhead. Goes from 0..(params.pattern_length - 1)
    pub playhead: usize,

    /// Playhead for each track.
    pub track_playhead: [usize; TRACK_COUNT],

    /// Amount between 0..u32::MAX that each tick increases.
    pub track_per_tick: [u64; TRACK_COUNT],

    /// If next tick is going to reset back to 0.
    pub next_is_reset: bool,

    // BPM detection/prediction.
    pub tempo: Tempo<{ CPU_SPEED }>,

    // Last clock tick.
    pub last: Time<{ CPU_SPEED }>,

    // Interval to next predicted clock tick.
    pub predicted: Time<{ CPU_SPEED }>,
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
    /// Offset showing 0-track length.
    Offset(u8),
    /// Track steps/length. [length][steps]
    Steps(u8, u8), // (length, steps)
}

impl Default for Display {
    fn default() -> Self {
        Display::Run
    }
}

pub type OperQueue = ArrayVec<Oper, 64>;

#[derive(Debug)]
/// The operations that can be done on the state.
pub enum Oper {
    Tick(Time<{ CPU_SPEED }>),
    Reset,
    Seed(i8),
    Length(i8),
    Offset(usize, i8),
    Steps(usize, i8),
}

impl State {
    pub fn new() -> Self {
        let mut st = State {
            params: STOKAST_PARAMS,
            generated: Generated::new(STOKAST_PARAMS),
            ..Default::default()
        };

        st.regenerate();

        st
    }

    pub fn update(&mut self, now: Time<{ CPU_SPEED }>, todo: impl Iterator<Item = Oper>) {
        let mut change = false;
        let mut regenerate = false;
        let mut display = None;

        for oper in todo {
            change = true;
            info!("Handle: {:?}", oper);

            match oper {
                Oper::Tick(interval) => {
                    self.last = now;
                    self.predicted = self.tempo.predict(interval);

                    self.playhead = if self.next_is_reset {
                        self.next_is_reset = false;

                        0
                    } else {
                        let n = self.playhead + 1;

                        // play_head goes from 0..(pattern_length - 1).
                        if n as u8 >= self.params.pattern_length {
                            0
                        } else {
                            n
                        }
                    };

                    self.track_playhead = self.track_playhead();
                }

                Oper::Reset => {
                    // Reset might affect the tempo detection.
                    self.tempo.reset();

                    // Whatever tick is coming next, it's going to reset back to 0.
                    self.next_is_reset = true;
                }

                Oper::Seed(x) => {
                    let s = (self.params.seed - SEED_BASE as u32) as i32;
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

                    let s = t.offset as i8;
                    let l = t.length as i8;
                    let mut n = s + x;

                    // Offset is 0 to step track length, wrapping around.
                    while n < 0 {
                        n += l;
                    }

                    while n >= l {
                        n -= l;
                    }

                    t.offset = n as u8;
                    regenerate = true;
                    display = Some(Display::Offset(n as u8));
                }

                Oper::Steps(i, x) => {
                    let t = &mut self.params.tracks[i];

                    let s1 = t.steps as i8;

                    let mut n1 = s1 + x;
                    let mut n2 = t.length as i8;

                    // Step cannot be longer than track length. Wrap around increases length.
                    if n1 > n2 {
                        n1 = 0;
                        n2 += 1;
                    }

                    // Step cannot < 0. Wrap around and decrease length.
                    if n1 < 0 {
                        n2 -= 1;
                        n1 = n2;
                    }

                    // Clamp values to min/max.
                    if n1 < 0 {
                        n1 = 0;
                    }
                    if n2 < 2 {
                        n2 = 2;
                    }
                    if n2 > 64 {
                        n2 = 64;
                    }
                    if n1 > n2 {
                        n1 = n2;
                    }

                    t.steps = n1 as u8;
                    t.length = n2 as u8;
                    regenerate = true;
                    display = Some(Display::Steps(n2 as u8, n1 as u8));
                }
            }
        }

        if regenerate {
            self.regenerate();
        }

        if let Some(d) = display {
            self.display = d;
            self.display_last_update = now;
            change = true;
        }

        if change {
            // info!("State: {:#?}", self);
        }
    }

    pub fn set_lfo_offset(&mut self, now: Time<{ CPU_SPEED }>) {
        let offset = self.track_offset(now);

        for (i, lfo) in self.lfo.iter_mut().enumerate() {
            lfo.set_offset(offset[i]);
        }
    }

    fn regenerate(&mut self) {
        self.generated = Generated::new(self.params);

        let mut rnd = Rnd::new(self.generated.rnd.next());

        for (i, lfo) in self.lfo.iter_mut().enumerate() {
            let length = self.params.tracks[i].length;
            lfo.set_seed_length(rnd.next(), length);
        }

        self.track_per_tick = [
            (u32::MAX / (self.params.tracks[0].length as u32)) as u64,
            (u32::MAX / (self.params.tracks[1].length as u32)) as u64,
            (u32::MAX / (self.params.tracks[2].length as u32)) as u64,
            (u32::MAX / (self.params.tracks[3].length as u32)) as u64,
        ];
    }

    fn track_playhead(&self) -> [usize; TRACK_COUNT] {
        let playhead = self.playhead;
        let parm = &self.params;
        let plen = parm.pattern_length as usize;

        [
            playhead % plen.min(parm.tracks[0].length as usize),
            playhead % plen.min(parm.tracks[1].length as usize),
            playhead % plen.min(parm.tracks[2].length as usize),
            playhead % plen.min(parm.tracks[3].length as usize),
        ]
    }

    fn track_offset(&self, now: Time<{ CPU_SPEED }>) -> [u32; TRACK_COUNT] {
        let lapsed = (now - self.last).count().max(0) as u64;
        let predicted = self.predicted.count() as u64;

        let ph = &self.track_playhead;
        let pt = &self.track_per_tick;

        #[inline(always)]
        fn pred(lapsed: u64, predicted: u64, per_tick: u64) -> u64 {
            if predicted > 0 {
                (lapsed.min(predicted) * per_tick) / predicted
            } else {
                0
            }
        }

        [
            (ph[0] as u64 * pt[0] + pred(lapsed, predicted, pt[0])) as u32,
            (ph[1] as u64 * pt[1] + pred(lapsed, predicted, pt[1])) as u32,
            (ph[2] as u64 * pt[2] + pred(lapsed, predicted, pt[2])) as u32,
            (ph[3] as u64 * pt[3] + pred(lapsed, predicted, pt[3])) as u32,
        ]
    }
}
