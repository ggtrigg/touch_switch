# Touch Switch

Bare-metal firmware for the **Raspberry Pi Pico (RP2040)** that turns a touch sensor and an APA102 LED into a bedside ambient light with gradual fade and double-clap detection.

## Features

- **Touch-sensitive dimmer** — short touch fades the light on/off, long touch toggles instantly
- **Double-clap detection** — clap twice to turn the light off from a distance
- **Gradual fade** — smooth rising/falling brightness ramps (no sudden light changes)
- **APA102 smart LED** — bright single-LED output with SPI control, chainable for more LEDs
- **PIO-based sensing** — touch and sound are handled entirely by the RP2040's programmable I/O, leaving the CPU free for application logic

## Hardware Requirements

| Component | Notes |
|-----------|-------|
| Raspberry Pi Pico (RP2040) | Any board with the RP2040 |
| Touch sensor | Capacitive touch sensor module (e.g., TTP223) or bare wire + RC circuit |
| Sound sensor | Digital sound sensor module (e.g., KY-038 / LM393) with HIGH on clap |
| APA102 LED | Addressable RGB LED (also known as DotStar) |
| Debug probe | SWD debugger (e.g., Raspberry Pi Debug Probe, J-Link, CMSIS-DAP) |
| 3.3V power supply | Sufficient for Pico + LED |

### Pin Connections

| GPIO | Function | Connection |
|------|----------|------------|
| GPIO16 | PIO0 — touch input | Touch sensor digital output |
| GPIO21 | PIO1 — sound input | Sound sensor digital output |
| GPIO10 | SPI1 SCK | APA102 clock |
| GPIO11 | SPI1 MOSI | APA102 data |

> **Sound sensor note:** GPIO21 uses an external ~10kΩ pull-down resistor to prevent false clap detections when the sensor is idle.

## Getting Started

### Prerequisites

- Rust toolchain with `thumbv6m-none-eabi` target
- `flip-link` linker: `cargo install flip-link`
- `probe-rs` tools: `cargo install probe-rs --features cli`
- Debug probe connected via SWD

### Build

```sh
# Release build (LTO fat, no overflow checks) — recommended for deployment
cargo build --target thumbv6m-none-eabi --release

# Debug build (overflow checks on, no LTO)
cargo build --target thumbv6m-none-eabi

# Host unit tests (no --target needed)
cargo test --lib
```

### Flash & Run

```sh
# Flash + RTT logging via SWD
cargo run --target thumbv6m-none-eabi

# Alternative: flash + reset without RTT
probe-rs download --chip RP2040 target/thumbv6m-none-eabi/release/touch_switch
probe-rs reset --chip RP2040
```

Use `DEFMT_LOG=debug` for verbose defmt output (set automatically in `.cargo/config.toml`).

## Usage

| Action | Behavior |
|--------|----------|
| Short touch | Toggle fade: Off → rising brightness, On → falling brightness |
| Long touch (hold ~10s) | Instant on/off |
| Double clap | Immediate off |

After a short touch, the brightness ramps gradually at a rate set by `DIM_DIVISOR` in `light.rs`.

## Architecture

```
┌─────────────┐    PIO FIFO     ┌──────────┐    SPI      ┌──────────┐
│  touch.pio   │ ──────────────> │  main.rs  │ ────────> │ APA102   │
│  (PIO0)      │    raw values   │           │            │  LED     │
└─────────────┘                 │  loop:    │            └──────────┘
                                │  poll     │
┌─────────────┐    PIO FIFO     │  process  │
│  clap.pio    │ ──────────────> │  state    │
│  (PIO1)      │   clap events   │           │
└─────────────┘                 └─────┬─────┘
                                      │
                               ┌──────┴──────┐
                               │  channel.rs  │
                               │  normalize,  │
                               │  debounce,   │
                               │  classify    │
                               └─────────────┘
```

| File | Role |
|------|------|
| `main.rs` | Wires peripherals, main loop polls PIO FIFOs |
| `channel.rs` | Touch state machine: normalizes raw PIO values, debounces, detects short/long touch |
| `light.rs` | APA102 LED driver via direct SPI writes |
| `touch.pio` | PIO program: measures capacitance via RC discharge timing |
| `clap.pio` | PIO program: detects double clap (2 sound events within ~500ms) |
| `test.pio` | Stub — do not use |

### PIO Programs

PIO0 runs the touch sensor, PIO1 runs the clap detector. Programs are compiled at build time from `.pio` files via the `pio_file!` macro and installed into the PIO's 32-slot instruction memory.

- **touch.pio** (16 instructions): `.wrap_target` is before `pull block` so each measurement cycle reloads Y (200,000) from the CPU via TX FIFO. X starts at `!null` (0xFFFFFFFF) and decrements once per charge-discharge cycle through the internal pull-up (~50kΩ) on GPIO16. Higher raw X values = fewer cycles completed = more capacitance (touch). The CPU writes Y back to the TX FIFO after reading each result.
- **clap.pio** (31 instructions): Builds its own inner counter (32767) via the ISR shift register. The outer loop (Y=4) creates a ~505ms detection window. The inner loop samples the sound pin every ~3.9μs. On a double clap, pushes 32767 to the RX FIFO then enters a post-push debounce loop (~1ms) to filter noise before wrapping back to `.wrap_target`. Uses `jmp start` instead of `jmp end` to stay within the 32-instruction limit.

> ⚠️ PIO programs are limited to 32 instructions. A `JMP` past the last instruction triggers a panic in `PIO::install()`.

## Technical Notes

- **`#![no_std]`** — no standard library; bare-metal Rust
- **`#![no_main]`** — uses `#[rp2040_hal::entry]` as the entry point
- **Linker:** `flip-link` for stack overflow protection (stack placed at bottom of RAM)
- **LED driver:** Direct `embedded_hal::blocking::spi::Write` calls (the `apa102-spi` crate was removed due to embedded-hal version incompatibility)
- **Logging:** defmt over RTT, captured by `probe-rs run` or `cargo embed`
- **Panic handler:** `panic-halt` — halts the CPU on panic (infinite loop)

## License

MIT
