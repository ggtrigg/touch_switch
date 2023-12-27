#![no_std]
#![no_main]

use defmt_rtt as _;
use embedded_hal::spi::MODE_0;
use fugit::RateExtU32;
use hal::gpio::{FunctionPio0, Pin, PullUp, FunctionSpi};
use hal::{pac, Clock};
use hal::pio::PIOExt;
use hal::Sio;
use hal::spi::Spi;
use panic_halt as _;
use rp2040_hal as hal;
use crate::channel::Channel;
use crate::light::Light;

pub mod channel;
pub mod light;

/// The linker will place this boot block at the start of our program image. We
/// need this to help the ROM bootloader get our code up and running.
/// Note: This boot block is not necessary when using a rp-hal based BSP
/// as the BSPs already perform this step.
#[link_section = ".boot2"]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

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

    let mut light = Light::new(spi);

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

    loop {
        match rx.read() {
            Some(val) => {
                light.process(channel.state(val));
            }
            None => ()
        }
    }
}