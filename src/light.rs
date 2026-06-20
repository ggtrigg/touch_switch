use touch_switch::channel::TouchState;
use embedded_hal::prelude::_embedded_hal_blocking_spi_Write;
use rp2040_hal::spi::{Spi, SpiDevice, ValidSpiPinout, Enabled};
use defmt::*;
use defmt_rtt as _;

static DIM_DIVISOR: u16 = 512;

const GAMMA: [u8; 256] = {
    let mut g = [0u8; 256];
    let mut i = 0;
    while i < 256 {
        g[i] = ((i * i) / 255) as u8;
        i += 1;
    }
    g
};

#[derive(Clone, Copy, PartialEq)]
pub enum LightState {
    On,
    Off,
    Rising,
    Falling,
    Steady,
}

pub struct Light<D: SpiDevice, P: ValidSpiPinout<D>> {
    spi: Spi<Enabled, D, P>,
    state: LightState,
    light_level: u8,
    sub_count: u16,
    last_touch_state: TouchState,
}

impl<D: SpiDevice, P: ValidSpiPinout<D>> Light<D, P> {
    pub fn new(spi: Spi<Enabled, D, P>) -> Self {
        let mut light = Light {
            spi,
            state: LightState::Off,
            light_level: 0,
            sub_count: 0,
            last_touch_state: TouchState::Warmup,
        };
        light.write_led(0, 0, 0);
        light
    }

    fn write_led(&mut self, r: u8, g: u8, b: u8) {
        let brightness = 0xE0 | 0x1F;
        let start_frame = [0u8; 4];
        let led_frame = [brightness, GAMMA[b as usize], GAMMA[g as usize], GAMMA[r as usize]];
        let end_frame = [0xFFu8; 4];
        
        self.spi.write(&start_frame).ok();
        self.spi.write(&led_frame).ok();
        self.spi.write(&end_frame).ok();
    }

    pub fn off(&mut self) {
        self.level(0);
        self.state = LightState::Off;
    }

    pub fn on(&mut self) {
        self.level(0xff);
        self.state = LightState::On;
    }

    fn level(&mut self, amount: u8) {
        self.light_level = amount;
        self.write_led(amount, amount, amount);
    }

    pub fn process(&mut self, touch_state: TouchState) {
        match touch_state {
            TouchState::Idle => {
                self.sub_count += 1;
                if self.sub_count >= DIM_DIVISOR {
                    match self.state {
                        LightState::Rising => {
                            self.increment();
                        }
                        LightState::Falling => {
                            self.decrement();
                        }
                        LightState::Off | LightState::On | LightState::Steady => (),
                    }
                    self.sub_count = 0;
                }
            }
            TouchState::Long => {
                if self.last_touch_state != TouchState::Long {
                    match self.state {
                        LightState::Off => {
                            debug!("Long touch ⇒ on");
                            self.on()
                        }
                        LightState::On | LightState::Rising | LightState::Falling | LightState::Steady => {
                            debug!("Long touch ⇒ off");
                            self.off()
                        }
                    }
                }
            },
            TouchState::Short => match self.state {
                LightState::Off => {
                    debug!("Short touch: Off→on");
                    self.level(15);
                    self.sub_count = 0;
                    self.state = LightState::Rising
                }
                LightState::On => {
                    debug!("Short touch: On→off");
                    self.level(0x7f);
                    self.sub_count = 0;
                    self.state = LightState::Falling
                }
                LightState::Rising | LightState::Falling | LightState::Steady => (),
            },
            TouchState::Warmup => (),
        }
        self.last_touch_state = touch_state;
    }

    fn increment(&mut self) {
        let newval = match self.light_level.checked_add(1) {
            Some(val) => val,
            None => {
                self.state = LightState::On;
                u8::MAX
            }
        };
        self.level(newval);
    }

    fn decrement(&mut self) {
        let newval = match self.light_level.checked_sub(1) {
            Some(val) => val,
            None => {
                self.state = LightState::Off;
                0
            }
        };
        self.level(newval);
    }
}
