use alg::clock::Time;
use bsp::hal::gpio::{Output, GPIO};
use imxrt_hal::iomuxc::gpio::Pin;
use teensy4_bsp as bsp;

use crate::state::State;
use crate::CPU_SPEED;

pub struct Outputs<P1, P2, P3, P4> {
    pub play_head_last: usize,
    pub gate1: Gate<P1>,
    pub gate2: Gate<P2>,
    pub gate3: Gate<P3>,
    pub gate4: Gate<P4>,
}

impl<P1, P2, P3, P4> Outputs<P1, P2, P3, P4>
where
    P1: HiLo,
    P2: HiLo,
    P3: HiLo,
    P4: HiLo,
{
    pub fn tick(&mut self, now: Time<{ CPU_SPEED }>, state: &State) {
        use GateSet::*;

        let play_head = state.play_head;

        let (g1, g2, g3, g4) = {
            if play_head != self.play_head_last {
                self.play_head_last = play_head;

                let g1 = state.generated.patterns[0][play_head].into();
                let g2 = state.generated.patterns[1][play_head].into();
                let g3 = state.generated.patterns[2][play_head].into();
                let g4 = state.generated.patterns[3][play_head].into();

                (g1, g2, g3, g4)
            } else {
                (Retain, Retain, Retain, Retain)
            }
        };

        self.gate1.tick(now, g1, &state.predicted);
        self.gate2.tick(now, g2, &state.predicted);
        self.gate3.tick(now, g3, &state.predicted);
        self.gate4.tick(now, g4, &state.predicted);
    }
}

pub struct Gate<H> {
    pin: H,
    duty_percent: i64,
    clear_at: Option<Time<{ CPU_SPEED }>>,
}

impl<H> Gate<H>
where
    H: HiLo,
{
    pub fn new(pin: H, duty_percent: u8) -> Self {
        Gate {
            pin,
            duty_percent: duty_percent as i64,
            clear_at: None,
        }
    }

    /// Tick to drive the gates. Whether to set, clear or retain the gate state.
    ///
    /// The predicted time next clock tick is happening.
    pub fn tick(
        &mut self,
        now: Time<{ CPU_SPEED }>,
        set: GateSet,
        predicted: &Time<{ CPU_SPEED }>,
    ) {
        // These gates are inverted out, so set_hilo(true) is OFF.

        match set {
            GateSet::Retain => {
                if let Some(clear_at) = self.clear_at {
                    if now >= clear_at {
                        self.clear_at.take();
                        self.pin.set_hilo(true);
                    }
                }
            }

            GateSet::Set => {
                self.pin.set_hilo(false);

                let duty_count = ((predicted.count() - now.count()) * self.duty_percent) / 100;

                let mut clear_at = now.clone();
                clear_at.count += duty_count;
                self.clear_at = Some(clear_at);
            }

            GateSet::Clear => {
                self.pin.set_hilo(true);

                self.clear_at.take();
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum GateSet {
    /// Retain the current gate state. Don't change it unless duty cycle comes to an end.
    Retain,
    /// Set the gate and keep it high for the duty cycle.
    Set,
    /// Clear the gate and clear any pending duty cycle.
    Clear,
}

impl From<u8> for GateSet {
    fn from(x: u8) -> Self {
        if x == 0 {
            Self::Clear
        } else {
            Self::Set
        }
    }
}

pub trait HiLo {
    fn set_hilo(&mut self, hi: bool);
}

impl<P> HiLo for GPIO<P, Output>
where
    P: Pin,
{
    fn set_hilo(&mut self, hi: bool) {
        if hi {
            self.set();
        } else {
            self.clear();
        }
    }
}
