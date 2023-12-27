use rp2040_hal::spi::{Spi, State, SpiDevice, ValidSpiPinout};
use smart_leds::{SmartLedsWrite, RGB8};
use apa102_spi::Apa102;

pub struct Light<S: State, D: SpiDevice, P: ValidSpiPinout<D>> {
  led: Apa102<Spi<S, D, P>>,
  led_data: [RGB8<>; 1]
}

impl<S: State, D: SpiDevice, P: ValidSpiPinout<D>> Light<S, D, P> where rp2040_hal::Spi<S, D, P>: embedded_hal::blocking::spi::write::Default<u8> {
  pub fn new(spi: Spi<S, D, P>) -> Self {
    Light {
      led: Apa102::new_with_custom_postamble(spi, 32, true),
      led_data: [RGB8::default(); 1]
    }
  }

  pub fn level(&mut self, lvl: u8) {
    (self.led_data[0].r, self.led_data[0].b, self.led_data[0].g) = (lvl, lvl, lvl);
    self.led.write(self.led_data.iter().cloned()).unwrap_or_default();
  }
}