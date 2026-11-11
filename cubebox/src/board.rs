//! Board wiring for the cubebox.
//!
//! Defaults target an ESP32-S3-DevKitC (N16R8) with an ST7789 240x320 IPS panel
//! on SPI2 and four momentary buttons to GND. Adjust the pin numbers here when
//! the real PCB is laid out; nothing else in the firmware hardcodes GPIOs.
//!
//! ```text
//!  ST7789            ESP32-S3
//!  SCLK   ---------  GPIO12
//!  MOSI   ---------  GPIO11
//!  CS     ---------  GPIO10
//!  DC     ---------  GPIO9
//!  RST    ---------  GPIO8
//!  BL     ---------  GPIO7   (PWM-capable)
//!
//!  Buttons (other side to GND, internal pull-up)
//!  BTN_START ------- GPIO4   start / stop, long press = new scramble
//!  BTN_NEXT  ------- GPIO5   next attempt / clear result
//!  BTN_PENALTY ----- GPIO6   +2 / DNF cycle
//!  BTN_MENU  ------- GPIO15  cycle pages
//! ```

/// Display geometry (ST7789 native orientation is 240x320 portrait).
pub const DISPLAY_WIDTH: u16 = 240;
pub const DISPLAY_HEIGHT: u16 = 320;

/// Scanlines buffered at a time. The UI is painted once per band and flushed
/// immediately, so peak framebuffer RAM is `width * DISPLAY_BAND_ROWS * 2`
/// (19.2 KB here) instead of a full 153.6 KB frame - this is what makes the
/// firmware viable on an ESP32-C3 without PSRAM. Larger = fewer SPI window
/// setup round-trips; smaller = less RAM.
pub const DISPLAY_BAND_ROWS: u16 = 40;

/// SPI clock for the panel. ST7789 tolerates 40 MHz on short traces; drop to
/// 20 MHz if the panel shows noise after wiring changes.
pub const DISPLAY_SPI_HZ: u32 = 40_000_000;
