#![cfg_attr(not(test), no_std)]
#![cfg_attr(not(test), no_main)]

use defmt::*;
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
use touch_switch::channel::Channel;
use crate::light::Light;

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
    let sound_pin: Pin<_, FunctionPio0, _> = pins.gpio21.into_function();
    let sound_pin_id = sound_pin.id().num;

    // Initialize and start PIO
    let (mut pio0, touch_sm, _, _, _) = pac.PIO0.split(&mut pac.RESETS);
    let (mut pio1, clap_sm, _, _, _) = pac.PIO1.split(&mut pac.RESETS);
    let installed1 = pio0.install(&pio::pio_file!("./src/touch.pio").program).unwrap();
    let installed2 = pio1.install(&pio::pio_file!("./src/clap.pio").program).unwrap();
    let (touch_sm, mut touch_rx, mut tx0) = rp2040_hal::pio::PIOBuilder::from_installed_program(installed1)
        .set_pins(touch_pin_id, 1)
        .jmp_pin(touch_pin_id)
        .build(touch_sm);
    touch_sm.start();
    let (clap_sm, mut clap_rx, _tx0) = rp2040_hal::pio::PIOBuilder::from_installed_program(installed2)
        .in_pin_base(sound_pin_id)
        .jmp_pin(sound_pin_id)
        .build(clap_sm);
    clap_sm.start();
    // PIO runs in background, independently from CPU

    let mut channel = Channel::new();
    tx0.write(200_000);  // Initial Y for first measurement

    debug!("Looping now...");

    loop {
        match touch_rx.read() {
            Some(val) => {
                let next = 200_000;
                tx0.write(next);  // Feed Y for next measurement
                light.process(channel.state(val));
            }
            None => ()
        }
        match clap_rx.read() {
            Some(_val) => {
                // Double clap detected (PIO pushes any nonzero value)
                light.off();
            }
            None => ()
        }
    }
}
