use alg::{Generated, Params, Time};

use crate::CPU_SPEED;

pub const TRACK_COUNT: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    /// Current display mode.
    pub display: Display,

    /// Time when display was last updated.
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
    Seed,
}

impl Default for Display {
    fn default() -> Self {
        Display::Run
    }
}

/// The operations that can be done on the state.
pub enum Oper {}
