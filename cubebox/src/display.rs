//! ST7789 (240x320 IPS) over SPI, plus a PSRAM-backed canvas that implements
//! `embedded-graphics`' `DrawTarget`.
//!
//! The driver is intentionally hand-rolled: it is ~200 lines, needs no C
//! bindings, and lets us flush arbitrary rectangles (essential when the
//! framebuffer lives in PSRAM and only the timer digits change).

use core::convert::Infallible;

use anyhow::{anyhow, Result};
use embedded_graphics::geometry::{OriginDimensions, Point, Size};
use embedded_graphics::pixelcolor::{Rgb565, RgbColor};
use embedded_graphics::prelude::{Dimensions, DrawTarget, Drawable, Primitive};
use embedded_graphics::primitives::Rectangle;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{Output, PinDriver};
use esp_idf_svc::hal::spi::SpiDeviceDriver;

// --- ST7789 command set (subset) ---
const SWRESET: u8 = 0x01;
const SLPOUT: u8 = 0x11;
const NORON: u8 = 0x13;
const INVON: u8 = 0x21;
const DISPON: u8 = 0x29;
const CASET: u8 = 0x2A;
const RASET: u8 = 0x2B;
const RAMWR: u8 = 0x2C;
const MADCTL: u8 = 0x36;
const COLMOD: u8 = 0x3A;

pub struct Display {
    spi: SpiDeviceDriver<'static, esp_idf_svc::hal::spi::SpiDriver<'static>>,
    dc: PinDriver<'static, Output>,
    rst: PinDriver<'static, Output>,
    bl: PinDriver<'static, Output>,
    width: u16,
    height: u16,
}

impl Display {
    pub fn new(
        spi: SpiDeviceDriver<'static, esp_idf_svc::hal::spi::SpiDriver<'static>>,
        dc: PinDriver<'static, Output>,
        rst: PinDriver<'static, Output>,
        bl: PinDriver<'static, Output>,
        width: u16,
        height: u16,
    ) -> Self {
        Self {
            spi,
            dc,
            rst,
            bl,
            width,
            height,
        }
    }

    fn cmd(&mut self, cmd: u8) -> Result<()> {
        self.dc.set_low().map_err(|e| anyhow!("dc low: {e:?}"))?;
        self.spi
            .write(&[cmd])
            .map_err(|e| anyhow!("spi cmd {cmd:#04x}: {e:?}"))
    }

    fn data(&mut self, data: &[u8]) -> Result<()> {
        self.dc
            .set_high()
            .map_err(|e| anyhow!("dc high: {e:?}"))?;
        self.spi
            .write(data)
            .map_err(|e| anyhow!("spi data: {e:?}"))
    }

    fn cmd_data(&mut self, cmd: u8, data: &[u8]) -> Result<()> {
        self.cmd(cmd)?;
        self.data(data)
    }

    /// Run the panel init sequence.
    pub fn init(&mut self) -> Result<()> {
        // Hardware reset.
        self.rst.set_high().ok();
        FreeRtos::delay_ms(10);
        self.rst.set_low().ok();
        FreeRtos::delay_ms(20);
        self.rst.set_high().ok();
        FreeRtos::delay_ms(120);

        self.cmd(SWRESET)?;
        FreeRtos::delay_ms(150);

        self.cmd(SLPOUT)?;
        FreeRtos::delay_ms(120);

        // 16 bits per pixel.
        self.cmd_data(COLMOD, &[0x55])?;
        // Memory access: top-left origin, RGB order.
        self.cmd_data(MADCTL, &[0x00])?;

        // Porch control.
        self.cmd_data(0xB2, &[0x0C, 0x0C, 0x00, 0x33, 0x33])?;
        // Gate control.
        self.cmd_data(0xB7, &[0x35])?;
        // VCOM.
        self.cmd_data(0xBB, &[0x28])?;
        // LCM control.
        self.cmd_data(0xC0, &[0x2C])?;
        // VDV/VRH enable.
        self.cmd_data(0xC2, &[0x01])?;
        // VRH set.
        self.cmd_data(0xC3, &[0x12])?;
        // VDV set.
        self.cmd_data(0xC4, &[0x20])?;
        // Frame rate 60 Hz.
        self.cmd_data(0xC6, &[0x0F])?;
        // Power control.
        self.cmd_data(0xD0, &[0xA4, 0xA1])?;

        // Gamma. Panel-specific but this curve is the common ST7789 default.
        self.cmd_data(
            0xE0,
            &[
                0xD0, 0x04, 0x0D, 0x11, 0x13, 0x2B, 0x3F, 0x54, 0x4C, 0x18, 0x0D, 0x0B, 0x1F,
                0x23,
            ],
        )?;
        self.cmd_data(
            0xE1,
            &[
                0xD0, 0x04, 0x0C, 0x11, 0x13, 0x2C, 0x3F, 0x44, 0x51, 0x2F, 0x1F, 0x1F, 0x20,
                0x23,
            ],
        )?;

        // IPS panels need colour inversion on.
        self.cmd(INVON)?;
        self.cmd(NORON)?;
        FreeRtos::delay_ms(10);
        self.cmd(DISPON)?;
        FreeRtos::delay_ms(20);

        self.backlight(true);
        Ok(())
    }

    pub fn backlight(&mut self, on: bool) {
        if on {
            self.bl.set_high().ok();
        } else {
            self.bl.set_low().ok();
        }
    }

    fn set_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) -> Result<()> {
        let xs = x0.to_be_bytes();
        let xe = x1.to_be_bytes();
        self.cmd(CASET)?;
        self.data(&[0x00, xs[0], 0x00, xs[1], 0x00, xe[0], 0x00, xe[1]])?;

        let ys = y0.to_be_bytes();
        let ye = y1.to_be_bytes();
        self.cmd(RASET)?;
        self.data(&[0x00, ys[0], 0x00, ys[1], 0x00, ye[0], 0x00, ye[1]])?;

        self.cmd(RAMWR)
    }

    /// Push a rectangle of the canvas to the panel. Coordinates are inclusive.
    pub fn flush_rect(
        &mut self,
        canvas: &Canvas,
        x0: u16,
        y0: u16,
        x1: u16,
        y1: u16,
    ) -> Result<()> {
        let x1 = x1.min(self.width.saturating_sub(1));
        let y1 = y1.min(self.height.saturating_sub(1));
        if x0 > x1 || y0 > y1 {
            return Ok(());
        }
        self.set_window(x0, y0, x1, y1)?;

        // 16-bit big-endian pixel stream, chunked to keep the tx buffer small.
        const CHUNK_PX: usize = 2048;
        let mut buf = [0u8; CHUNK_PX * 2];
        let mut used = 0usize;

        self.dc.set_high().ok();
        for y in y0..=y1 {
            for x in x0..=x1 {
                let v = canvas.get(x, y);
                buf[used] = (v >> 8) as u8;
                buf[used + 1] = (v & 0xFF) as u8;
                used += 2;
                if used == buf.len() {
                    self.spi
                        .write(&buf[..used])
                        .map_err(|e| anyhow!("spi pixels: {e:?}"))?;
                    used = 0;
                }
            }
        }
        if used > 0 {
            self.spi
                .write(&buf[..used])
                .map_err(|e| anyhow!("spi pixels tail: {e:?}"))?;
        }
        Ok(())
    }

    /// Push the whole canvas.
    pub fn flush_all(&mut self, canvas: &Canvas) -> Result<()> {
        self.flush_rect(canvas, 0, 0, self.width - 1, self.height - 1)
    }
}

/// RGB565 framebuffer. On the S3 this allocation lands in PSRAM.
pub struct Canvas {
    width: u16,
    height: u16,
    buf: Vec<u16>,
}

impl Canvas {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            buf: vec![0u16; width as usize * height as usize],
        }
    }

    pub fn bytes(&self) -> usize {
        self.buf.len() * 2
    }

    #[inline]
    pub fn get(&self, x: u16, y: u16) -> u16 {
        if x >= self.width || y >= self.height {
            return 0;
        }
        self.buf[y as usize * self.width as usize + x as usize]
    }

    #[inline]
    pub fn put(&mut self, x: i32, y: i32, color: Rgb565) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let idx = y as usize * self.width as usize + x as usize;
        self.buf[idx] = pack(color);
    }

    pub fn clear(&mut self, color: Rgb565) {
        let v = pack(color);
        self.buf.iter_mut().for_each(|p| *p = v);
    }
}

#[inline]
fn pack(c: Rgb565) -> u16 {
    ((c.r() as u16 & 0xF8) << 8) | ((c.g() as u16 & 0xFC) << 3) | ((c.b() as u16) >> 3)
}

impl DrawTarget for Canvas {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Rgb565>>,
    {
        for embedded_graphics::Pixel(pos, color) in pixels.into_iter() {
            self.put(pos.x, pos.y, color);
        }
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Rgb565>,
    {
        let full = *area;
        if full.size.width == 0 || full.size.height == 0 {
            return Ok(());
        }
        let clipped = full.intersection(&self.bounding_box());
        if clipped.size.width == 0 || clipped.size.height == 0 {
            return Ok(());
        }
        let cx0 = clipped.top_left.x;
        let cy0 = clipped.top_left.y;
        let cx1 = cx0 + clipped.size.width as i32 - 1;
        let cy1 = cy0 + clipped.size.height as i32 - 1;

        let mut it = colors.into_iter();
        for y in full.top_left.y..full.top_left.y + full.size.height as i32 {
            for x in full.top_left.x..full.top_left.x + full.size.width as i32 {
                let Some(c) = it.next() else { return Ok(()) };
                if x >= cx0 && x <= cx1 && y >= cy0 && y <= cy1 {
                    self.put(x, y, c);
                }
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Self::Error> {
        let area = area.intersection(&self.bounding_box());
        if area.size.width == 0 || area.size.height == 0 {
            return Ok(());
        }
        let v = pack(color);
        let x0 = area.top_left.x;
        let y0 = area.top_left.y;
        for y in y0..y0 + area.size.height as i32 {
            if y < 0 || y >= self.height as i32 {
                continue;
            }
            let row = y as usize * self.width as usize;
            for x in x0..x0 + area.size.width as i32 {
                if x < 0 || x >= self.width as i32 {
                    continue;
                }
                self.buf[row + x as usize] = v;
            }
        }
        Ok(())
    }
}

impl OriginDimensions for Canvas {
    fn size(&self) -> Size {
        Size::new(self.width as u32, self.height as u32)
    }
}

impl Canvas {
    /// Convenience: solid rectangle with top-left origin (used by the UI).
    pub fn rect(&mut self, x: i32, y: i32, w: u32, h: u32, color: Rgb565) {
        use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};
        let _ = Rectangle::new(Point::new(x, y), Size::new(w, h))
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(self);
    }
}
