use core::cell::RefCell;

use alg::clock::Time;
use alg::input::DeltaInput;
use alg::input::DigitalInput;
use alg::input::Edge;
use alg::input::EdgeInput;
use alg::input::HiLo;
use bsp::hal::gpio::{Input, GPIO};
use cortex_m::interrupt::Mutex;
use imxrt_hal::iomuxc::gpio::Pin;
use teensy4_bsp as bsp;

use crate::state::Oper;
use crate::state::OperQueue;
use crate::CPU_SPEED;

pub static IO_EXT1: Mutex<RefCell<u16>> = Mutex::new(RefCell::new(0));
pub static IO_EXT2: Mutex<RefCell<u16>> = Mutex::new(RefCell::new(0));

pub struct Inputs<
    Digi1,
    Digi2,
    RSeed,
    RLen,
    Roffs1,
    RStep1,
    Roffs2,
    RStep2,
    Roffs3,
    RStep3,
    Roffs4,
    RStep4,
> {
    pub clock: Digi1,
    pub reset: Digi2,

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

impl<Digi1, Digi2, RSeed, RLen, Roffs1, RStep1, Roffs2, RStep2, Roffs3, RStep3, Roffs4, RStep4>
    Inputs<
        Digi1,
        Digi2,
        RSeed,
        RLen,
        Roffs1,
        RStep1,
        Roffs2,
        RStep2,
        Roffs3,
        RStep3,
        Roffs4,
        RStep4,
    >
where
    Digi1: EdgeInput<{ CPU_SPEED }>,
    Digi2: EdgeInput<{ CPU_SPEED }>,
    RSeed: DeltaInput<{ CPU_SPEED }>,
    RLen: DeltaInput<{ CPU_SPEED }>,
    Roffs1: DeltaInput<{ CPU_SPEED }>,
    RStep1: DeltaInput<{ CPU_SPEED }>,
    Roffs2: DeltaInput<{ CPU_SPEED }>,
    RStep2: DeltaInput<{ CPU_SPEED }>,
    Roffs3: DeltaInput<{ CPU_SPEED }>,
    RStep3: DeltaInput<{ CPU_SPEED }>,
    Roffs4: DeltaInput<{ CPU_SPEED }>,
    RStep4: DeltaInput<{ CPU_SPEED }>,
{
    pub fn tick(&mut self, now: Time<{ CPU_SPEED }>, todo: &mut OperQueue) {
        // Clock input
        {
            let x = self.clock.tick(now);
            // falling since inverted
            if let Some(Edge::Falling(_)) = x {
                todo.push(Oper::Tick);
            }
        }

        // Reset input
        {
            let x = self.reset.tick(now);
            // falling since inverted
            if let Some(Edge::Falling(_)) = x {
                todo.push(Oper::Reset);
            }
        }

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

/// Wrapper type because we're not allowed to do:
/// impl<P> DigitalInput<{ CPU_SPEED }> for GPIO<P, Input> {}
pub struct PinDigitalIn<P>(pub GPIO<P, Input>);

impl<P, const CLK: u32> DigitalInput<CLK> for PinDigitalIn<P>
where
    P: Pin,
{
    fn tick(&mut self, now: Time<CLK>) -> HiLo<CLK> {
        if self.0.is_set() {
            HiLo::Hi(now)
        } else {
            HiLo::Lo(now)
        }
    }
}
