use alg::encoder::QuadratureSource;
use bsp::hal::gpio::{Input, GPIO};
use imxrt_hal::iomuxc::gpio::Pin;
use teensy4_bsp as bsp;

/// QuadratureSource hooked up to two GPIO pins.
pub struct GpioQuadratureSource<PA, PB> {
    pin_a: GPIO<PA, Input>,
    pin_b: GPIO<PB, Input>,
}

impl<PA, PB> GpioQuadratureSource<PA, PB>
where
    PA: Pin,
    PB: Pin,
{
    pub fn new(pin_a: GPIO<PA, Input>, pin_b: GPIO<PB, Input>) -> Self {
        GpioQuadratureSource { pin_a, pin_b }
    }
}

impl<PA, PB> QuadratureSource for GpioQuadratureSource<PA, PB>
where
    PA: Pin,
    PB: Pin,
{
    fn pin_a(&self) -> bool {
        self.pin_a.is_set()
    }

    fn pin_b(&self) -> bool {
        self.pin_b.is_set()
    }
}
