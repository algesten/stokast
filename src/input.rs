use alg::clock::Time;
use alg::input::DeltaInput;
use alg::input::DigitalInput;
use alg::input::Edge;
use alg::input::EdgeInput;
use alg::input::HiLo;
use bsp::hal::gpio::{Input, GPIO};
use imxrt_hal::iomuxc::gpio::Pin;
use teensy4_bsp as bsp;

use crate::state::Oper;
use crate::state::OperQueue;
use crate::CPU_SPEED;

/// Holder of all hardware input.
///
/// The type parameters here looks rather nuts. The reason is that we want to hide all
/// concrete input pins/types underneath.
pub struct Inputs<
    Digi1,
    Digi2,
    RSeed,
    RSeedBtn,
    RLen,
    RLenBtn,
    Roffs1,
    Roffs1Btn,
    RStep1,
    RStep1Btn,
    Roffs2,
    Roffs2Btn,
    RStep2,
    RStep2Btn,
    Roffs3,
    Roffs3Btn,
    RStep3,
    RStep3Btn,
    Roffs4,
    Roffs4Btn,
    RStep4,
    RStep4Btn,
> {
    pub clock: Digi1,
    pub reset: Digi2,

    pub seed: RSeed,
    pub seed_btn: RSeedBtn,

    pub length: RLen,
    pub length_btn: RLenBtn,

    pub offs1: Roffs1,
    pub offs1_btn: Roffs1Btn,
    pub step1: RStep1,
    pub step1_btn: RStep1Btn,

    pub offs2: Roffs2,
    pub offs2_btn: Roffs2Btn,
    pub step2: RStep2,
    pub step2_btn: RStep2Btn,

    pub offs3: Roffs3,
    pub offs3_btn: Roffs3Btn,
    pub step3: RStep3,
    pub step3_btn: RStep3Btn,

    pub offs4: Roffs4,
    pub offs4_btn: Roffs4Btn,
    pub step4: RStep4,
    pub step4_btn: RStep4Btn,
}

impl<
        Digi1,
        Digi2,
        RSeed,
        RSeedBtn,
        RLen,
        RLenBtn,
        Roffs1,
        Roffs1Btn,
        RStep1,
        RStep1Btn,
        Roffs2,
        Roffs2Btn,
        RStep2,
        RStep2Btn,
        Roffs3,
        Roffs3Btn,
        RStep3,
        RStep3Btn,
        Roffs4,
        Roffs4Btn,
        RStep4,
        RStep4Btn,
    >
    Inputs<
        Digi1,
        Digi2,
        RSeed,
        RSeedBtn,
        RLen,
        RLenBtn,
        Roffs1,
        Roffs1Btn,
        RStep1,
        RStep1Btn,
        Roffs2,
        Roffs2Btn,
        RStep2,
        RStep2Btn,
        Roffs3,
        Roffs3Btn,
        RStep3,
        RStep3Btn,
        Roffs4,
        Roffs4Btn,
        RStep4,
        RStep4Btn,
    >
where
    Digi1: EdgeInput<{ CPU_SPEED }>,
    Digi2: EdgeInput<{ CPU_SPEED }>,
    RSeed: DeltaInput<{ CPU_SPEED }>,
    RSeedBtn: EdgeInput<{ CPU_SPEED }>,
    RLen: DeltaInput<{ CPU_SPEED }>,
    RLenBtn: EdgeInput<{ CPU_SPEED }>,
    Roffs1: DeltaInput<{ CPU_SPEED }>,
    Roffs1Btn: EdgeInput<{ CPU_SPEED }>,
    RStep1: DeltaInput<{ CPU_SPEED }>,
    RStep1Btn: EdgeInput<{ CPU_SPEED }>,
    Roffs2: DeltaInput<{ CPU_SPEED }>,
    Roffs2Btn: EdgeInput<{ CPU_SPEED }>,
    RStep2: DeltaInput<{ CPU_SPEED }>,
    RStep2Btn: EdgeInput<{ CPU_SPEED }>,
    Roffs3: DeltaInput<{ CPU_SPEED }>,
    Roffs3Btn: EdgeInput<{ CPU_SPEED }>,
    RStep3: DeltaInput<{ CPU_SPEED }>,
    RStep3Btn: EdgeInput<{ CPU_SPEED }>,
    Roffs4: DeltaInput<{ CPU_SPEED }>,
    Roffs4Btn: EdgeInput<{ CPU_SPEED }>,
    RStep4: DeltaInput<{ CPU_SPEED }>,
    RStep4Btn: EdgeInput<{ CPU_SPEED }>,
{
    pub fn tick(&mut self, now: Time<{ CPU_SPEED }>, todo: &mut OperQueue, io_ext_change: bool) {
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

        // All below this line is about io_ext chip changes. Early return if there are no changes.
        if !io_ext_change {
            return;
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
