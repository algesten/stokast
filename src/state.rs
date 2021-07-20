#![allow(dead_code)]

use alg::clock::Time;
use alg::gen::{Generated, Params, SEED_BASE, STOKAST_PARAMS};
use alg::rnd::Rnd;
use alg::tempo::Tempo;
use arrayvec::ArrayVec;

use crate::lfo::SAW_DN;
use crate::lfo::SAW_UP;
use crate::lfo::{self, Lfo};
use crate::max6958::Seg;
use crate::max6958::Segs4;
use crate::CPU_SPEED;

pub const TRACK_COUNT: usize = 4;

/// App state
#[derive(Debug, Default, Clone)]
pub struct State {
    /// Current input mode.
    pub input_mode: InputMode,

    /// Track for current input mode (if relevant)
    pub input_track: Option<usize>,

    /// Time when last user action happened. This is so we can
    /// switch back to the Run mode once left idle for a bit.
    pub last_action: Time<{ CPU_SPEED }>,

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
pub enum InputMode {
    /// Increasing steps per incoming clock tick.
    /// This is the default we go back to after showing something else.
    Run,

    /// Seed showing 0-9999.
    Seed(u16),
    /// Show "fate" and wait for a knob twiddle.
    Fate,

    /// Length showing 2-32.
    Length(u8),

    /// Offset showing 0-track length.
    Offset(u8),
    /// Which track lfo is currently active.
    Lfo(lfo::Mode),

    /// Track steps/length. [length][steps]
    Steps(u8, u8), // (length, steps)
    /// Which track sync mode.
    Sync(TrackSync),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackSync {
    /// Track is restarted at pattern length and reset.
    Sync = 0,
    /// Track is restarted only by reset.
    Free = 1,
    /// Track just keeps looping, ignoring both pattern length and reset.
    Loop = 2,
}

impl Default for InputMode {
    fn default() -> Self {
        InputMode::Run
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
        let mut input_mode = None;
        let mut input_track = None;

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
                        input_mode = Some(InputMode::Seed(n as u16));
                        input_track = None;
                    }
                }

                Oper::Length(x) => {
                    let s = self.params.pattern_length as i8;
                    let n = s + x;

                    // Patterns must be 2-64.
                    if n >= 2 && n <= 64 {
                        self.params.pattern_length = n as u8;
                        regenerate = true;
                        input_mode = Some(InputMode::Length(n as u8));
                        input_track = None;
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
                    input_mode = Some(InputMode::Offset(n as u8));
                    input_track = Some(i);
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
                    input_mode = Some(InputMode::Steps(n2 as u8, n1 as u8));
                    input_track = Some(i);
                }
            }
        }

        if regenerate {
            self.regenerate();
        }

        if let Some(input_mode) = input_mode {
            self.input_mode = input_mode;
            self.input_track = input_track;
            self.last_action = now;
            change = true;
        }

        if change {
            // info!("State: {:#?}", self);
        }
    }

    /// Update the state with passing time.
    pub fn update_time(&mut self, now: Time<{ CPU_SPEED }>) {
        // Reset back the input mode to the default after a timeout.
        if self.input_mode != InputMode::Run && now - self.last_action > Time::from_secs(5) {
            self.input_mode = InputMode::Run;
            self.input_track = None;
        }

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

    /// Represent the current state on the segment display.
    pub fn to_display(&self) -> Segs4 {
        match &self.input_mode {
            InputMode::Run => {
                let mut segs = Segs4::new();

                segs.0[1] = Seg::from(self.playhead as u8 % 10) as u8;
                segs.0[2] = Seg::from((self.playhead as u8 / 10) % 10) as u8;

                // Loop animation over 6 frames synced on playhead.
                const LOOP: [u8; 6] = [
                    Seg::SegA as u8,
                    Seg::SegB as u8,
                    Seg::SegC as u8,
                    Seg::SegD as u8,
                    Seg::SegE as u8,
                    Seg::SegF as u8,
                ];

                let li = self.playhead % 6;
                let c = LOOP[li];
                segs.0[0] = c;
                segs.0[3] = c;

                segs
            }

            InputMode::Seed(v) => (*v).into(),

            InputMode::Fate => "fate".into(),

            InputMode::Length(v) => (*v).into(),

            InputMode::Offset(v) => (*v).into(),

            InputMode::Lfo(l) => match l {
                lfo::Mode::Random => "rand".into(),
                lfo::Mode::SawUp => SAW_UP,
                lfo::Mode::SawDown => SAW_DN,
                lfo::Mode::Sine => "sine".into(),
                lfo::Mode::Sine90 => "si90".into(),
                lfo::Mode::Sine180 => "s180".into(),
                lfo::Mode::Triangle => "tria".into(),
                lfo::Mode::Triangle90 => "tr90".into(),
                lfo::Mode::Triangle180 => "t180".into(),
                lfo::Mode::Square => "puls".into(),
                lfo::Mode::Square90 => "pu90".into(),
                lfo::Mode::Square180 => "p180".into(),
            },

            InputMode::Steps(l, s) => {
                let mut segs = Segs4::new();

                segs.0[0] = Seg::from(s % 10) as u8;
                segs.0[1] = Seg::from((s / 10) % 10) as u8;
                segs.0[2] = Seg::from(l % 10) as u8;
                segs.0[3] = Seg::from((l / 10) % 10) as u8;

                segs
            }

            InputMode::Sync(v) => match v {
                TrackSync::Sync => "sync",
                TrackSync::Free => "free",
                TrackSync::Loop => "loop",
            }
            .into(),
        }
    }
}
