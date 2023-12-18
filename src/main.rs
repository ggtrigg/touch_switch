#![no_std]
#![no_main]

// use core::time::Duration;

use defmt_rtt as _;
use embedded_hal::digital::v2::OutputPin;
use hal::gpio::{FunctionPio0, Pin, PullUp};
use hal::pac;
use hal::pio::PIOExt;
use hal::Sio;
// use hal::rtc::DateTime;
use panic_halt as _;
use rp2040_hal as hal;
// use rtic_monotonic;
// use rp2040_monotonic;
// use pio_proc::pio_file;

/// The linker will place this boot block at the start of our program image. We
/// need this to help the ROM bootloader get our code up and running.
/// Note: This boot block is not necessary when using a rp-hal based BSP
/// as the BSPs already perform this step.
#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

#[derive(Clone, Copy)]
enum TouchState {
    Warmup,
    Idle,
    Long
}

struct Channel {
    warmup: u32,
    level_lo: u32,
    level_hi: u32,
    level: f32,
    last_state: bool,
    timer: rp2040_hal::Timer,
    last_dt: u64
}

impl Channel {
    fn new(timer: rp2040_hal::Timer) -> Self {
        Channel {
            warmup: 100,
            level_lo: u32::MAX,
            level_hi: 0,
            level: 0.0,
            last_state: false,
            timer,
            last_dt: 0
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
                    match lvl < 0.5 {
                        true => {
                            match self.last_state {
                                true => {
                                    let diff = self.timer.get_counter().ticks() - self.last_dt;
                                    new_state = match diff > 1_000 {
                                        true => TouchState::Long,
                                        false => TouchState::Idle
                                    }
                                }
                                false => {
                                    // Off to on, start timer
                                    self.last_dt = self.timer.get_counter().ticks();
                                    new_state = TouchState::Idle;
                                }
                            }
                            self.last_state = true;
                        }
                        false => {
                            new_state = TouchState::Idle;
                            self.last_state = false;
                        }
                    }
                }
                None => { new_state = TouchState::Idle; }
            }
        }
        new_state
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
    watchdog.enable_tick_generation(12);
    let clocks = hal::clocks::init_clocks_and_plls(12_000_000, pac.XOSC, pac.CLOCKS, pac.PLL_SYS, pac.PLL_USB, &mut pac.RESETS, &mut watchdog).ok().unwrap();
    let timer = hal::timer::Timer::new(pac.TIMER, &mut pac.RESETS, &clocks);
    // let mono = rp2040_monotonic::Rp2040Monotonic::new(pac.TIMER);

    let sio = Sio::new(pac.SIO);
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let mut led_pin = pins.gpio17.into_push_pull_output();
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

    let mut channel = Channel::new(timer);
    let mut toggle: bool = false;

    loop {
        match rx.read() {
            Some(val) => {
                match channel.state(val) {
                    TouchState::Idle =>  led_pin.set_low().unwrap(),
                    TouchState::Long =>  led_pin.set_high().unwrap(),
                    TouchState::Warmup => {
                        if toggle {
                            led_pin.set_high().unwrap();
                        } else {
                            led_pin.set_low().unwrap();
                        }
                        toggle = ! toggle;
                    }
                };
            }
            None => ()
        }
    }
}