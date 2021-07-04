use alg::Time;

use crate::{
    state::{Oper, OperQueue},
    CPU_SPEED,
};

/// An input that produces deltas. Think rotary encoder that will give us
/// -1, 0 or 1.
pub trait DeltaInput {
    /// Polled every main-loop run. Returns 0 as long as there isn't a value.
    fn tick(&mut self, now: Time<{ CPU_SPEED }>) -> i8;
}

pub struct Inputs<RSeed, RLen, Roffs1, RStep1, Roffs2, RStep2, Roffs3, RStep3, Roffs4, RStep4> {
    pub seed: RSeed,
    pub length: RLen,

    pub offs1: Roffs1,
    pub step1: RStep1,

    pub offs2: Roffs2,
    pub step2: RStep2,

    pub offs3: Roffs3,
    pub step3: RStep3,

    pub offs4: Roffs4,
    pub step4: RStep4,
}

impl<RSeed, RLen, Roffs1, RStep1, Roffs2, RStep2, Roffs3, RStep3, Roffs4, RStep4>
    Inputs<RSeed, RLen, Roffs1, RStep1, Roffs2, RStep2, Roffs3, RStep3, Roffs4, RStep4>
where
    RSeed: DeltaInput,
    RLen: DeltaInput,
    Roffs1: DeltaInput,
    RStep1: DeltaInput,
    Roffs2: DeltaInput,
    RStep2: DeltaInput,
    Roffs3: DeltaInput,
    RStep3: DeltaInput,
    Roffs4: DeltaInput,
    RStep4: DeltaInput,
{
    pub fn tick(&mut self, now: Time<{ CPU_SPEED }>, todo: &mut OperQueue) {
        // Global seed
        {
            let x = self.seed.tick(now);
            if x != 0 {
                todo.push(Oper::Seed(x));
            }
        }

        // Global length
        {
            let x = self.length.tick(now);
            if x != 0 {
                todo.push(Oper::Length(x));
            }
        }

        // Track offsets
        {
            let x = self.offs1.tick(now);
            if x != 0 {
                todo.push(Oper::Offset(0, x));
            }
        }
        {
            let x = self.offs2.tick(now);
            if x != 0 {
                todo.push(Oper::Offset(1, x));
            }
        }
        {
            let x = self.offs3.tick(now);
            if x != 0 {
                todo.push(Oper::Offset(2, x));
            }
        }
        {
            let x = self.offs4.tick(now);
            if x != 0 {
                todo.push(Oper::Offset(3, x));
            }
        }

        // Track steps
        {
            let x = self.step1.tick(now);
            if x != 0 {
                todo.push(Oper::Steps(0, x));
            }
        }
        {
            let x = self.step2.tick(now);
            if x != 0 {
                todo.push(Oper::Steps(1, x));
            }
        }
        {
            let x = self.step3.tick(now);
            if x != 0 {
                todo.push(Oper::Steps(2, x));
            }
        }
        {
            let x = self.step4.tick(now);
            if x != 0 {
                todo.push(Oper::Steps(3, x));
            }
        }
    }
}

impl DeltaInput for () {
    fn tick(&mut self, _now: Time<{ CPU_SPEED }>) -> i8 {
        0
    }
}
