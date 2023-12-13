#![no_std]
#![no_main]

// use defmt::info;
// use defmt::debug;
use defmt_rtt as _;
use embedded_hal::digital::v2::OutputPin;
use hal::gpio::{FunctionPio0, Pin, PullUp};
use hal::pac;
use hal::pio::PIOExt;
use hal::Sio;
use panic_halt as _;
use rp2040_hal as hal;
// use pio_proc::pio_file;

/// The linker will place this boot block at the start of our program image. We
/// need this to help the ROM bootloader get our code up and running.
/// Note: This boot block is not necessary when using a rp-hal based BSP
/// as the BSPs already perform this step.
#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

struct Channel {
    warmup: u32,
    level_lo: u32,
    level_hi: u32,
    level: f32
}

impl Channel {
    fn new() -> Self {
        Channel {
            warmup: 100,
            level_lo: u32::MAX,
            level_hi: 0,
            level: 0.0
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
}

/// Entry point to our bare-metal application.
///
/// The `#[rp2040_hal::entry]` macro ensures the Cortex-M start-up code calls this function
/// as soon as all global variables and the spinlock are initialised.
#[rp2040_hal::entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();

    let sio = Sio::new(pac.SIO);
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    // configure transmit & receive pins for Pio0.
    let trigger: Pin<_, FunctionPio0, _> = pins.gpio14.into_function();
    let echo: Pin<_, FunctionPio0, _> = pins.gpio15.into_function();
    // PIN ids for use inside of PIO
    let _trigger_pin_id = trigger.id().num;
    let _echo_pin_id = echo.id().num;

    let mut led_pin = pins.gpio17.into_push_pull_output();
    // let led_pin: Pin<_, FunctionPio0, _> = pins.gpio22.into_function();
    let _led_pin_id = led_pin.id().num;
    let touch_pin: Pin<_, FunctionPio0, _> = pins.gpio16.into_function().into_pull_type::<PullUp>();
    // let pull_type = touch_pin.pull_type();
    // info!("Pull type is: {}", pull_type as i32);
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
        // .clock_divisor_fixed_point(1, 0)
        .build(sm0);
    // pio.irq0().enable_sm_interrupt(0);
    // The GPIO pin needs to be configured as an output.
    // sm.set_pindirs([(touch_pin_id, hal::pio::PinDir::Input)]);
    sm.start();
    // PIO runs in background, independently from CPU

    // let mut distance: u32 = 0;
    let mut channel = Channel::new();
    let mut toggle: bool = false;

    loop {
        match rx.read() {
            Some(val) => {
                match channel.normalize(val) {
                    Some(level) => {
                        match level > 0.5 {
                            true => {
                                led_pin.set_low().unwrap();
                            }
                            false => {
                                led_pin.set_high().unwrap();
                            }
                        }
                    }
                    None => {
                        if channel.warmup > 0 {
                            if toggle {
                                led_pin.set_high().unwrap();
                            } else {
                                led_pin.set_low().unwrap();
                            }
                            toggle = ! toggle;
                        }
                    }
                };
            }
            None => ()
        }
    }
}