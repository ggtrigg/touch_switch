# touch_switch

Bare-metal RP2040 (Raspberry Pi Pico) touch-sensitive LED dimmer with clap detection.

## Build, test & run

```sh
cargo build --target thumbv6m-none-eabi --release     # release build
cargo build --target thumbv6m-none-eabi               # debug build
cargo test --lib                                       # host unit tests (no --target)
cargo run --target thumbv6m-none-eabi                 # flash via probe-rs (Swd @ 20MHz)
cargo embed --target thumbv6m-none-eabi               # alternative flash + RTT logging
```

Target: `thumbv6m-none-eabi`, linker: `flip-link`.

## Architecture

| File | Role |
|------|------|
| `main.rs` | Wires peripherals, main loop polls PIO FIFOs |
| `channel.rs` | Touch state machine: normalizes raw PIO values, debounces, detects short/long touch |
| `light.rs` | APA102 LED driver via direct SPI writes |
| `touch.pio` | PIO program: measures capacitance via RC discharge timing |
| `clap.pio` | PIO program: detects double clap (2 sound events within ~500ms) |
| `test.pio` | Stub — do not use |

## PIO programs

Compiled at build time from `src/*.pio` via `pio::pio_file!` macro:

```rust
let program = pio0.install(&pio::pio_file!("./src/touch.pio").program).unwrap();
```

PIO0 = touch, PIO1 = clap.

⚠️ `pio_file!` generates `Program<32>` — max 32 instructions. A JMP past the last instruction (e.g., to the wrap address) triggers a panic in `PIO::install()` when the program's offset pushes it beyond instruction slot 31.

- Touch PIO (`touch.pio`): 16 instructions. `.wrap_target` is placed before `pull block` so each measurement cycle reloads Y (200,000) from the CPU via TX FIFO. X starts at `!null` (0xFFFFFFFF) and decrements once per charge-discharge cycle through the internal pull-up (~50kΩ) on GPIO16. Higher raw X values = fewer cycles completed = more capacitance (touch). The CPU writes Y back to the TX FIFO after reading each result.
- Clap PIO (`clap.pio`): 31 instructions. Builds its own inner counter (32767) via ISR shift register. Outer loop (Y=4) creates ~505ms detection window. Inner loop samples pin every ~482 cycles (~3.9μs). After a double clap, pushes 32767 to RX FIFO then enters a post-push debounce loop (~1ms) to filter noise spikes before wrapping back to `.wrap_target`. Uses `jmp start` (no `jmp end` past `.wrap`).

## SPI LED control

The apa102-spi crate was removed due to embedded-hal version incompatibility. LED writes use `embedded_hal::blocking::spi::Write` directly with the `Enabled` state:

```rust
use embedded_hal::prelude::_embedded_hal_blocking_spi_Write;
```

The `Light` struct requires `Spi<Enabled, D, P>` (not generic over `S: State`). The SpiBus trait from embedded-hal 1.0 is NOT available for rp2040-hal 0.12's Spi type.

## Key constraints

- `#![no_std]` — no `std::` anything
- `#![no_main]` — uses `#[rp2040_hal::entry]` as entry point
- Inputs: gpio16 (touch sensor, pull-up, PIO0), gpio21 (sound sensor, external ~10kΩ pull-down, PIO1)
- Outputs: gpio10 (SPI1 SCK), gpio11 (SPI1 MOSI) — APA102 LED
- defmt logging via probe-rs RTT; set `DEFMT_LOG=debug` for verbose output
