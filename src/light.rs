use super::channel::TouchState;
use apa102_spi::Apa102;
use rp2040_hal::spi::{Spi, SpiDevice, State, ValidSpiPinout};
use smart_leds::{SmartLedsWrite, RGB8};

static DIM_DIVISOR: u8 = 30;
pub enum LightState {
    On,
    Off,
    Rising,
    Falling,
}

pub struct Light<S: State, D: SpiDevice, P: ValidSpiPinout<D>> {
    led: Apa102<Spi<S, D, P>>,
    led_data: [RGB8; 1],
    state: LightState,
    light_level: u8,
    sub_count: u8,
    last_touch_state: TouchState,
}

impl<S: State, D: SpiDevice, P: ValidSpiPinout<D>> Light<S, D, P>
where
    rp2040_hal::Spi<S, D, P>: embedded_hal::blocking::spi::write::Default<u8>,
{
    pub fn new(spi: Spi<S, D, P>) -> Self {
        Light {
            led: Apa102::new_with_custom_postamble(spi, 32, true),
            led_data: [RGB8::default(); 1],
            state: LightState::Off,
            light_level: 0,
            sub_count: 0,
            last_touch_state: TouchState::Warmup,
        }
    }

    fn level(&mut self) {
        (self.led_data[0].r, self.led_data[0].b, self.led_data[0].g) =
            (self.light_level, self.light_level, self.light_level);
        self.led
            .write(self.led_data.iter().cloned())
            .unwrap_or_default();
    }

    pub fn process(&mut self, touch_state: TouchState) {
        match touch_state {
            TouchState::Idle => {
                self.sub_count += 1;
                if self.sub_count >= DIM_DIVISOR {
                    match self.state {
                        LightState::Rising => {
                            self.light_level = match self.light_level.checked_add(1) {
                                Some(val) => val,
                                None => {
                                    self.state = LightState::On;
                                    u8::MAX
                                }
                            };
                            self.level();
                        }
                        LightState::Falling => {
                            self.light_level = match self.light_level.checked_sub(1) {
                                Some(val) => val,
                                None => {
                                    self.state = LightState::Off;
                                    0
                                }
                            };
                            self.level();
                        }
                        LightState::Off | LightState::On => (),
                    }
                    self.sub_count = 0;
                }
            }
            // Long touch -> immediate on/off
            TouchState::Long => {
                if self.last_touch_state != TouchState::Long {
                    match self.state {
                        LightState::Off => {
                            self.light_level = 0xff;
                            self.level();
                            self.sub_count = 0;
                            self.state = LightState::On
                        }
                        LightState::On => {
                            self.light_level = 0;
                            self.level();
                            self.sub_count = 0;
                            self.state = LightState::Off
                        }
                        LightState::Rising | LightState::Falling => (),
                    }
                }
            },
            // Short touch -> gradual on/off
            TouchState::Short => match self.state {
                LightState::Off => {
                    self.light_level = 0;
                    self.level();
                    self.sub_count = 0;
                    self.state = LightState::Rising
                }
                LightState::On => {
                    self.light_level = 0xff;
                    self.level();
                    self.sub_count = 0;
                    self.state = LightState::Falling
                }
                LightState::Rising | LightState::Falling => (),
            },
            TouchState::Warmup => (),
        }
        self.last_touch_state = touch_state;
    }
}
