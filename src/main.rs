#![no_std]
#![no_main]

// use core::time::Duration;

use defmt_rtt as _;
// use embedded_hal::digital::v2::OutputPin;
use embedded_hal::spi::MODE_0;
use fugit::RateExtU32;
use hal::gpio::{FunctionPio0, Pin, PullUp, FunctionSpi};
use hal::{pac, Clock};
use hal::pio::PIOExt;
use hal::Sio;
use hal::spi::Spi;
use panic_halt as _;
use rp2040_hal as hal;
use smart_leds::{SmartLedsWrite, RGB8};
use apa102_spi::Apa102;

/// The linker will place this boot block at the start of our program image. We
/// need this to help the ROM bootloader get our code up and running.
/// Note: This boot block is not necessary when using a rp-hal based BSP
/// as the BSPs already perform this step.
#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;
static DIM_DIVISOR: u8 = 30;

#[derive(Clone, Copy)]
enum TouchState {
    Warmup,
    Idle,
    Short,
    Long
}

enum LightState {
    On,
    Off,
    Rising,
    Falling
}

struct Channel {
    warmup: u32,
    level_lo: u32,
    level_hi: u32,
    level: f32,
    last_state: bool,
    last_touch_state: TouchState,
    counter: u32
}

impl Channel {
    fn new() -> Self {
        Channel {
            warmup: 100,
            level_lo: u32::MAX,
            level_hi: 0,
            level: 0.0,
            last_state: false,
            last_touch_state: TouchState::Idle,
            counter: 0
        }
    }

    fn normalize(&mut self, raw_val: u32) -> Option<f32> {
        if self.warmup > 0 {
            self.warmup -= 1;
            None
        } else {
            self.level_lo = self.level_lo.min(raw_val);
            self.level_hi = self.level_hi.max(raw_val);

            let window = self.level_hi - self.level_lo;
            if window > 64 {
                self.level = 1.0 - (raw_val - self.level_lo) as f32 / window as f32;
                Some(self.level)
            } else {
                None
            }
        }
    }

    fn state(&mut self, raw_val: u32) -> TouchState {
        let level = self.normalize(raw_val);
        let new_state;

        if self.warmup > 0 {
            new_state = TouchState::Warmup;
        } else {
            match level {
                Some(lvl) => {
                    if self.counter > 100 {      // Debounce
                        match lvl < 0.5 {
                            true => {
                                match self.last_state {
                                    true => {
                                        new_state = match self.counter > 2_000 {
                                            true => TouchState::Long,
                                            false => TouchState::Idle
                                        }
                                    }
                                    false => {
                                        // Finger touched, start counting
                                        new_state = TouchState::Idle;
                                        self.counter = 0;
                                    }
                                }
                                self.last_state = true;
                                self.count();
                            }
                            false => {
                                match self.last_state {
                                    true => {   // Finger lifted
                                        match self.counter != 0 && self.counter <= 2_000 {
                                            true => { new_state = TouchState::Short }
                                            false => { new_state = TouchState::Idle }
                                        }
                                        self.counter = 0;
                                    }
                                    false => {
                                        new_state = TouchState::Idle;
                                    }
                                }
                                self.last_state = false;
                                self.count();
                            }
                        }
                    } else {
                        self.count();
                        new_state = self.last_touch_state;
                    }
                }
                None => { new_state = TouchState::Idle; }
            };
        }
        self.last_touch_state = new_state;
        new_state
    }

    fn count(&mut self) {
        self.counter = match self.counter.checked_add(1) {
            Some(val) => val,
            None => u32::MAX
        };
    }
}

/// Entry point to our bare-metal application.
///
/// The `#[rp2040_hal::entry]` macro ensures the Cortex-M start-up code calls this function
/// as soon as all global variables and the spinlock are initialised.
#[rp2040_hal::entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);

    let sio = Sio::new(pac.SIO);
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );
    let clocks = hal::clocks::init_clocks_and_plls(
        12_000_000,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let sclk = pins.gpio10.into_function::<FunctionSpi>();
    let mosi = pins.gpio11.into_function::<FunctionSpi>();
    let spi_pin_layout = (mosi, sclk);
    let spi = Spi::<_, _, _, 8>::new(pac.SPI1, spi_pin_layout)
        .init(&mut pac.RESETS, clocks.peripheral_clock.freq(), 2_500_000u32.Hz(), MODE_0);

    let mut led = Apa102::new_with_custom_postamble(spi, 32, true);
    let mut led_data: [RGB8<>; 1] = [RGB8::default(); 1];
    let mut light_level: u8 = 0;
    let mut sub_count: u8 = 0;
    (led_data[0].r, led_data[0].b, led_data[0].g) = (light_level, light_level, light_level);
    led.write(led_data.iter().cloned()).unwrap();

    let touch_pin: Pin<_, FunctionPio0, _> = pins.gpio16.into_function().into_pull_type::<PullUp>();
    let touch_pin_id = touch_pin.id().num;

    let program_with_defines = pio_proc::pio_file!(
        "./src/touch.pio",
    );
    let program = program_with_defines.program;

    // Initialize and start PIO
    let (mut pio, sm0, _, _, _) = pac.PIO0.split(&mut pac.RESETS);
    let installed = pio.install(&program).unwrap();
    let (sm, mut rx, _tx) = rp2040_hal::pio::PIOBuilder::from_program(installed)
        .set_pins(touch_pin_id, 1)
        .jmp_pin(touch_pin_id)
        .build(sm0);
    sm.start();
    // PIO runs in background, independently from CPU

    let mut channel = Channel::new();
    let mut last_light_state = LightState::Off;

    loop {
        match rx.read() {
            Some(val) => {
                match channel.state(val) {
                    TouchState::Idle => {
                        sub_count += 1;
                        if sub_count >= DIM_DIVISOR {
                            match last_light_state {
                                LightState::Rising => {
                                    light_level = match light_level.checked_add(1) {
                                        Some(val) => val,
                                        None => {
                                            last_light_state = LightState::On;
                                            u8::MAX
                                        }
                                    };
                                    (led_data[0].r, led_data[0].b, led_data[0].g) = (light_level, light_level, light_level);
                                    led.write(led_data.iter().cloned()).unwrap();
                                }
                                LightState::Falling => {
                                    light_level = match light_level.checked_sub(1) {
                                        Some(val) => val,
                                        None => {
                                            last_light_state = LightState::Off;
                                            0
                                        }
                                    };
                                    (led_data[0].r, led_data[0].b, led_data[0].g) = (light_level, light_level, light_level);
                                    led.write(led_data.iter().cloned()).unwrap();
                                }
                                LightState::Off | LightState::On => ()
                            }
                            sub_count = 0;
                        }
                    }
                    TouchState::Long =>  (),
                    TouchState::Short =>  {
                        match last_light_state {
                            LightState::Off =>  {
                                light_level = 0;
                                (led_data[0].r, led_data[0].b, led_data[0].g) = (light_level, light_level, light_level);
                                led.write(led_data.iter().cloned()).unwrap();
                                sub_count = 0;
                                last_light_state = LightState::Rising
                            }
                            LightState::On => {
                                light_level = 0xff;
                                (led_data[0].r, led_data[0].b, led_data[0].g) = (light_level, light_level, light_level);
                                led.write(led_data.iter().cloned()).unwrap();
                                sub_count = 0;
                                last_light_state = LightState::Falling
                            }
                            LightState::Rising | LightState::Falling => ()
                        }
                    }
                    TouchState::Warmup => ()
                };
            }
            None => ()
        }
    }
}