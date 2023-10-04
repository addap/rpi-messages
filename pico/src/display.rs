use core::convert::Infallible;
use core::result::Result;

use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{DrawTarget, OriginDimensions, Point, RawData, Size};
use embedded_graphics::primitives::{PointsIter, Rectangle};
use embedded_graphics::Pixel;
use embedded_hal_02::blocking::delay::DelayMs;
use embedded_hal_02::blocking::spi;

pub trait OutputPin: embedded_hal_02::digital::v2::OutputPin<Error = Infallible> {}
impl<T> OutputPin for T where T: embedded_hal_02::digital::v2::OutputPin<Error = Infallible> {}

pub trait SpiWrite: spi::Write<u8, Error = embassy_rp::spi::Error> {}
impl<T> SpiWrite for T where T: spi::Write<u8, Error = embassy_rp::spi::Error> {}

pub struct ST7735<SPI, DC, RST, CS, BL>
where
    SPI: SpiWrite,
    DC: OutputPin,
    RST: OutputPin,
    CS: OutputPin,
    BL: OutputPin,
{
    /// SPI
    spi: SPI,

    /// Data/command pin.
    dc: DC,

    /// Reset pin.
    rst: RST,

    /// Chip select pin.
    cs: CS,

    /// Backlight pin.
    _bl: BL,

    width: u8,
    height: u8,
    dx: u8,
    dy: u8,
}

impl<SPI, DC, RST, CS, BL> ST7735<SPI, DC, RST, CS, BL>
where
    SPI: SpiWrite,
    DC: OutputPin,
    RST: OutputPin,
    CS: OutputPin,
    BL: OutputPin,
{
    pub fn new(spi: SPI, dc: DC, rst: RST, cs: CS, bl: BL, width: u8, height: u8) -> Self {
        Self {
            spi,
            dc,
            rst,
            cs,
            _bl: bl,
            dx: 0,
            dy: 0,
            width,
            height,
        }
    }

    pub fn set_offset(&mut self, dx: u8, dy: u8) {
        self.dx = dx;
        self.dy = dy;
    }

    pub fn init<DELAY>(&mut self, delay: &mut DELAY)
    where
        DELAY: DelayMs<u8>,
    {
        self.module_init();
        self.reset(delay);
        self.init_reg(delay);
    }

    fn write_cmd(&mut self, cmd: u8) {
        self.cs.set_high().unwrap();
        self.dc.set_low().unwrap();
        self.cs.set_low().unwrap();
        self.spi.write(&[cmd]).unwrap();
        self.cs.set_high().unwrap();
    }

    fn write_data(&mut self, data: u8) {
        self.cs.set_high().unwrap();
        self.dc.set_high().unwrap();
        self.cs.set_low().unwrap();
        self.spi.write(&[data]).unwrap();
        self.cs.set_high().unwrap();
    }

    fn module_init(&mut self) {
        // Don't select chip in the beginning
        self.cs.set_high().unwrap();

        // # configure pwm
        // pwm = PWM(self.bl)
        // pwm.freq(1000)
        // pwm.duty_u16(32768)  # max 65535
    }

    fn reset<DELAY>(&mut self, delay: &mut DELAY)
    where
        DELAY: DelayMs<u8>,
    {
        self.rst.set_high().unwrap();
        delay.delay_ms(10);
        self.rst.set_low().unwrap();
        delay.delay_ms(10);
        self.rst.set_high().unwrap();
    }

    fn init_reg<DELAY>(&mut self, delay: &mut DELAY)
    where
        DELAY: DelayMs<u8>,
    {
        ////////////////////////////////////
        // 65k mode
        // the other one does it in the end
        self.write_cmd(0x3A);
        self.write_data(0x05);
        //////////////////////////////////////

        // ST7735R Frame Rate
        self.write_cmd(0xB1);
        self.write_data(0x01);
        self.write_data(0x2C);
        self.write_data(0x2D);

        self.write_cmd(0xB2);
        self.write_data(0x01);
        self.write_data(0x2C);
        self.write_data(0x2D);

        self.write_cmd(0xB3);
        self.write_data(0x01);
        self.write_data(0x2C);
        self.write_data(0x2D);
        self.write_data(0x01);
        self.write_data(0x2C);
        self.write_data(0x2D);

        // Column inversion
        self.write_cmd(0xB4);
        self.write_data(0x07);

        // ST7735R Power Sequence
        self.write_cmd(0xC0);
        self.write_data(0xA2);
        self.write_data(0x02);
        self.write_data(0x84);
        self.write_cmd(0xC1);
        self.write_data(0xC5);

        self.write_cmd(0xC2);
        self.write_data(0x0A);
        self.write_data(0x00);

        self.write_cmd(0xC3);
        self.write_data(0x8A);
        self.write_data(0x2A);
        self.write_cmd(0xC4);
        self.write_data(0x8A);
        self.write_data(0xEE);

        self.write_cmd(0xC5); // VCOM
        self.write_data(0x0E);

        // ST7735R Gamma Sequence
        self.write_cmd(0xe0);
        self.write_data(0x0f);
        self.write_data(0x1a);
        self.write_data(0x0f);
        self.write_data(0x18);
        self.write_data(0x2f);
        self.write_data(0x28);
        self.write_data(0x20);
        self.write_data(0x22);
        self.write_data(0x1f);
        self.write_data(0x1b);
        self.write_data(0x23);
        self.write_data(0x37);
        self.write_data(0x00);
        self.write_data(0x07);
        self.write_data(0x02);
        self.write_data(0x10);

        self.write_cmd(0xe1);
        self.write_data(0x0f);
        self.write_data(0x1b);
        self.write_data(0x0f);
        self.write_data(0x17);
        self.write_data(0x33);
        self.write_data(0x2c);
        self.write_data(0x29);
        self.write_data(0x2e);
        self.write_data(0x30);
        self.write_data(0x30);
        self.write_data(0x39);
        self.write_data(0x3f);
        self.write_data(0x00);
        self.write_data(0x07);
        self.write_data(0x03);
        self.write_data(0x10);

        self.write_cmd(0xF0); // Enable test command
        self.write_data(0x01);

        self.write_cmd(0xF6); // Disable ram power save mode
        self.write_data(0x00);

        delay.delay_ms(100);
        // set orientation to landscape
        self.write_cmd(0x36);
        self.write_data(0x60);

        //////////////////////////////////////////////////////////
        // sleep out
        self.write_cmd(0x11);
        // DEV_Delay_ms(120);;

        // Turn on the LCD display
        self.write_cmd(0x29)
    }

    fn set_address_window(&mut self, sx: u8, sy: u8, ex: u8, ey: u8) {
        self.write_cmd(0x2A);
        self.write_data(0);
        self.write_data(sx + self.dx);
        self.write_data(0x00);
        self.write_data(ex + self.dx);

        self.write_cmd(0x2B);
        self.write_data(0x00);
        self.write_data(sy + self.dy);
        self.write_data(0x00);
        self.write_data(ey + self.dy);
    }

    pub fn set_pixel(&mut self, x: u8, y: u8, color: u16) {
        self.set_address_window(x, y, x, y);
        self.write_cmd(0x2C);

        self.cs.set_high().unwrap();
        self.dc.set_high().unwrap();
        self.cs.set_low().unwrap();
        self.spi.write(&color.to_be_bytes()).unwrap();
        self.cs.set_high().unwrap();
    }
}

impl<SPI, DC, RST, CS, BL> DrawTarget for ST7735<SPI, DC, RST, CS, BL>
where
    SPI: SpiWrite,
    DC: OutputPin,
    RST: OutputPin,
    CS: OutputPin,
    BL: OutputPin,
{
    type Error = Infallible;
    type Color = Rgb565;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Infallible>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels.into_iter() {
            // Only draw pixels that would be on screen
            if coord.x >= 0 && coord.y >= 0 && coord.x < self.width as i32 && coord.y < self.height as i32 {
                self.set_pixel(coord.x as u8, coord.y as u8, RawU16::from(color).into_inner());
            }
        }

        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Infallible>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        // Clamp area to drawable part of the display target
        let drawable_area = area.intersection(&Rectangle::new(Point::zero(), self.size()));

        if drawable_area.size != Size::zero() {
            self.set_address_window(
                drawable_area.top_left.x as u8,
                drawable_area.top_left.y as u8,
                (drawable_area.top_left.x + (drawable_area.size.width - 1) as i32) as u8,
                (drawable_area.top_left.y + (drawable_area.size.height - 1) as i32) as u8,
            );

            self.write_cmd(0x2C);

            self.cs.set_high().unwrap();
            self.dc.set_high().unwrap();
            self.cs.set_low().unwrap();

            let mut buffer = [0; 32];
            let mut index = 0;
            for color in area
                .points()
                .zip(colors)
                .filter(|(pos, _color)| drawable_area.contains(*pos))
                .map(|(_, color)| RawU16::from(color).into_inner())
            {
                let as_bytes = color.to_be_bytes();
                buffer[index] = as_bytes[0];
                buffer[index + 1] = as_bytes[1];
                index += 2;
                if index >= buffer.len() {
                    self.spi.write(&buffer).unwrap();
                    index = 0;
                }
            }
            self.spi.write(&buffer[0..index]).unwrap();

            self.cs.set_high().unwrap();
        }
        Ok(())
    }
}

impl<SPI, DC, RST, CS, BL> OriginDimensions for ST7735<SPI, DC, RST, CS, BL>
where
    SPI: SpiWrite,
    DC: OutputPin,
    RST: OutputPin,
    CS: OutputPin,
    BL: OutputPin,
{
    fn size(&self) -> embedded_graphics::prelude::Size {
        embedded_graphics::prelude::Size {
            width: self.width as u32,
            height: self.height as u32,
        }
    }
}
