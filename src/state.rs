#![allow(dead_code)]

use alg::clock::Time;
use alg::gen::{Generated, Params, SEED_BASE, STOKAST_PARAMS};
use alg::rnd::Rnd;
use alg::tempo::Tempo;
use arrayvec::ArrayVec;
use cortex_m::peripheral::DWT;

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

    /// Time when last user action happened. This is so we can
    /// switch back to the Run mode once left idle for a bit.
    pub last_action: Time<{ CPU_SPEED }>,

    /// Generative parameters for generated.
    pub params: Params<{ TRACK_COUNT }>,

    /// The generated tracks.
    pub generated: Generated<{ TRACK_COUNT }>,

    /// The LFOs.
    pub lfo: [Lfo; TRACK_COUNT],

    /// Track sync setting.
    pub track_sync: [TrackSync; TRACK_COUNT],

    /// Current global playhead. Goes from 0..whenever external reset comes.
    pub playhead: u64,

    /// Ever increasing count of the clock tick. Never resets.
    pub tick_count: u64,

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
    Seed,
    /// Show "fate" and wait for a knob twiddle.
    Fate,

    /// Length showing 2-32.
    Length,

    /// Offset showing 0-track length.
    Offset(usize),
    /// Which track lfo is currently active.
    Lfo(usize),

    /// Track steps/length. [length][steps]
    Steps(usize), // (length, steps)
    /// Which track sync mode.
    TrackSync(usize),
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

impl TrackSync {
    const fn len() -> usize {
        3
    }
}

impl From<i8> for TrackSync {
    fn from(mut x: i8) -> Self {
        use TrackSync::*;

        while x < 0 {
            x += Self::len() as i8;
        }

        match x % (Self::len() as i8) {
            0 => Sync,
            1 => Free,
            2 => Loop,
            _ => panic!("Wot wot?"),
        }
    }
}

pub type OperQueue = ArrayVec<Oper, 64>;

#[derive(Debug)]
/// The operations that can be done on the state.
pub enum Oper {
    Tick(Time<{ CPU_SPEED }>),
    Reset,
    Seed(i8),
    SeedClick,
    Length(i8),
    LengthClick,
    Offset(usize, i8),
    OffsetClick(usize),
    Steps(usize, i8),
    StepsClick(usize),
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
        let mut input_mode = None;
        let mut regenerate = false;

        for oper in todo {
            info!("Handle: {:?}", oper);

            match oper {
                Oper::Tick(interval) => {
                    self.last = now;
                    self.predicted = self.tempo.predict(interval);
                    self.tick_count += 1;

                    self.playhead = if self.next_is_reset {
                        self.next_is_reset = false;

                        0
                    } else {
                        self.playhead + 1
                    };

                    self.update_track_playhead();
                }

                Oper::Reset => {
                    // Reset might affect the tempo detection.
                    self.tempo.reset();

                    // Whatever tick is coming next, it's going to reset back to 0.
                    self.next_is_reset = true;
                }

                Oper::Seed(x) => {
                    if self.input_mode == InputMode::Fate {
                        // KABOOM randomize all the things.
                        self.tonight_im_in_the_hands_of_fate();
                    } else {
                        let s = (self.params.seed - SEED_BASE as u32) as i32;
                        let n = s + x as i32;

                        // Seed is 0-9999
                        if n >= 0 && n <= 9999 {
                            self.params.seed = (n + SEED_BASE) as u32;
                            input_mode = Some(InputMode::Seed);
                            regenerate = true;
                        }
                    }
                }

                Oper::SeedClick => {
                    if self.input_mode == InputMode::Fate {
                        input_mode = Some(InputMode::Seed);
                    } else {
                        input_mode = Some(InputMode::Fate);
                    }
                }

                Oper::Length(x) => {
                    let s = self.params.pattern_length as i8;
                    let n = s + x;

                    // Patterns must be 2-64.
                    if n >= 2 && n <= 64 {
                        self.params.pattern_length = n as u8;
                        input_mode = Some(InputMode::Length);
                        regenerate = true;
                    }
                }

                Oper::LengthClick => {
                    todo!() // what did I have planned here?!
                }

                Oper::Offset(tr, x) => {
                    if self.input_mode == InputMode::Lfo(tr) {
                        self.lfo[tr].set_mode(x);
                        self.last_action = now;
                        regenerate = true;
                    } else {
                        let t = &mut self.params.tracks[tr];

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
                        input_mode = Some(InputMode::Offset(tr));
                        regenerate = true;
                    }
                }

                Oper::OffsetClick(tr) => {
                    if self.input_mode == InputMode::Lfo(tr) {
                        input_mode = Some(InputMode::Offset(tr));
                    } else {
                        input_mode = Some(InputMode::Lfo(tr));
                    }
                }

                Oper::Steps(tr, x) => {
                    if self.input_mode == InputMode::TrackSync(tr) {
                        let mut n = self.track_sync[tr] as i8;
                        n += x;
                        self.track_sync[tr] = n.into();
                        self.last_action = now;
                        // no need to regenerate here.
                    } else {
                        let t = &mut self.params.tracks[tr];

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
                        input_mode = Some(InputMode::Steps(tr));
                        regenerate = true;
                    }
                }

                Oper::StepsClick(tr) => {
                    if self.input_mode == InputMode::TrackSync(tr) {
                        input_mode = Some(InputMode::Steps(tr));
                    } else {
                        input_mode = Some(InputMode::TrackSync(tr));
                    }
                }
            }
        }

        if regenerate {
            self.regenerate();
        }

        if let Some(input_mode) = input_mode {
            self.input_mode = input_mode;
            self.last_action = now;
        }
    }

    /// Current playhead, 0-63 for instance (depends on pattern length).
    pub fn playhead(&self) -> usize {
        (self.playhead % self.params.pattern_length as u64) as usize
    }

    /// Update the state with passing time.
    pub fn update_time(&mut self, now: Time<{ CPU_SPEED }>) {
        // Reset back the input mode to the default after a timeout.
        if self.input_mode != InputMode::Run && now - self.last_action > Time::from_secs(5) {
            self.input_mode = InputMode::Run;
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

        for i in 0..TRACK_COUNT {
            self.track_per_tick[i] = (u32::MAX / (self.params.tracks[i].length as u32)) as u64
        }
    }

    fn update_track_playhead(&mut self) {
        let parm = &self.params;
        let plen = parm.pattern_length as usize;
        let playhead = self.playhead();

        for i in 0..TRACK_COUNT {
            self.track_playhead[i] = match self.track_sync[i] {
                TrackSync::Sync => playhead % plen.min(parm.tracks[0].length as usize),
                TrackSync::Free => (self.playhead % parm.tracks[0].length as u64) as usize,
                TrackSync::Loop => (self.tick_count % parm.tracks[0].length as u64) as usize,
            };
        }
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

        let mut offs = [0; TRACK_COUNT];

        for i in 0..TRACK_COUNT {
            offs[i] = (ph[i] as u64 * pt[i] + pred(lapsed, predicted, pt[i])) as u32;
        }

        offs
    }

    /// Represent the current state on the segment display.
    pub fn to_display(&self) -> Segs4 {
        match &self.input_mode {
            InputMode::Run => {
                let mut segs = Segs4::new();

                let playhead = self.playhead();
                segs.0[1] = Seg::from(playhead as u8 % 10) as u8;
                segs.0[2] = Seg::from((playhead as u8 / 10) % 10) as u8;

                // Loop animation over 6 frames synced on playhead.
                const LOOP: [u8; 6] = [
                    Seg::SegA as u8,
                    Seg::SegB as u8,
                    Seg::SegC as u8,
                    Seg::SegD as u8,
                    Seg::SegE as u8,
                    Seg::SegF as u8,
                ];

                let li = playhead % 6;
                let c = LOOP[li];
                segs.0[0] = c;
                segs.0[3] = c;

                segs
            }

            InputMode::Seed => (self.params.seed - SEED_BASE as u32).into(),

            InputMode::Fate => "fate".into(),

            InputMode::Length => self.params.pattern_length.into(),

            InputMode::Offset(tr) => self.params.tracks[*tr].offset.into(),

            InputMode::Lfo(tr) => match self.lfo[*tr].mode {
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

            InputMode::Steps(tr) => {
                let (s, l) = {
                    let p = &self.params.tracks[*tr];
                    (p.steps, p.length)
                };

                let mut segs = Segs4::new();

                segs.0[0] = Seg::from(s % 10) as u8;
                segs.0[1] = Seg::from((s / 10) % 10) as u8;
                segs.0[2] = Seg::from(l % 10) as u8;
                segs.0[3] = Seg::from((l / 10) % 10) as u8;

                segs
            }

            InputMode::TrackSync(tr) => match self.track_sync[*tr] {
                TrackSync::Sync => "sync",
                TrackSync::Free => "free",
                TrackSync::Loop => "loop",
            }
            .into(),
        }
    }

    fn tonight_im_in_the_hands_of_fate(&mut self) {
        // TODO: the logic here should maybe be moved into alg?

        // Cycle count is probably random enough as starting point.
        let seed = DWT::get_cycle_count();
        let mut rnd = Rnd::new(seed);

        // do tracks before global seed since the seed is further used
        // for randomization and we don't want the same values.
        for i in 0..TRACK_COUNT {
            // always generate both x and y also when y isn't used.

            // Length
            {
                let x = rnd.next();
                let y = rnd.next();
                self.params.tracks[i].length = if x < u32::MAX / 2 {
                    // half the time, we do some power of 2.
                    let n = (y / (u32::MAX / 6)) + 1;
                    2_u8.pow(n).max(2).min(64)
                } else {
                    // at other times some interesting offset.
                    const LENGTHS: &[u8] =
                        &[3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 24];
                    LENGTHS[(y / (u32::MAX / LENGTHS.len() as u32)) as usize]
                };
            }

            // Steps
            {
                let x = rnd.next();
                let y = rnd.next();
                self.params.tracks[i].steps = if x < u32::MAX / 2 {
                    // 0 in steps causes the awesome generative randomization to kick in.
                    // we want that to happen... often.
                    0
                } else {
                    let len = self.params.tracks[i].length as u32;
                    (y / (u32::MAX / (len + 1))) as u8
                };
            }

            // Offset
            {
                let x = rnd.next();
                let y = rnd.next();
                self.params.tracks[i].offset = if x < u32::MAX / 2 {
                    // Half the time, no offset.
                    0
                } else {
                    let len = self.params.tracks[i].length as u32;
                    (y / (u32::MAX / len)) as u8
                };
            }
        }

        self.params.seed = (rnd.next() / (u32::MAX / 9999)) + SEED_BASE as u32;

        // Make moar magic happen
        self.regenerate();

        // Next tick will start from 0
        self.next_is_reset = true;
    }
}

impl Default for TrackSync {
    fn default() -> Self {
        TrackSync::Sync
    }
}

impl Default for InputMode {
    fn default() -> Self {
        InputMode::Run
    }
}
