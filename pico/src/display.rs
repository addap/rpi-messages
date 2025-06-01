use common::consts::{IMAGE_HEIGHT, IMAGE_WIDTH, TEXT_COLUMNS, TEXT_LINES};
use embassy_rp::{
    gpio::Output,
    spi::{Blocking, Spi},
};
use embassy_time::Delay;
use embedded_graphics::{
    draw_target::DrawTarget,
    image::{Image, ImageRaw, ImageRawBE},
    mono_font::{self, ascii::FONT_9X15, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::Rectangle,
};
use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_text::{
    alignment::{HorizontalAlignment, VerticalAlignment},
    style::{HeightMode, TextBoxStyle, TextBoxStyleBuilder, VerticalOverdraw},
    TextBox,
};

const MESSAGE_FONT: mono_font::MonoFont = FONT_9X15;
const MESSAGE_TEXT_COLOR: Rgb565 = Rgb565::BLACK;
const MESSAGE_BG_COLOR: Rgb565 = Rgb565::WHITE;
pub const PRIO_MESSAGE_BG_COLOR: Rgb565 = Rgb565::RED;
pub const MESSAGE_TEXT_STYLE: MonoTextStyle<'_, Rgb565> = MonoTextStyle::new(&MESSAGE_FONT, MESSAGE_TEXT_COLOR);

const MARGIN_LEFT: u32 = 4;
const MARGIN_RIGHT: u32 = 3;
const MARGIN_TOP: u32 = 4;
const MARGIN_BOTTOM: u32 = 4;

/// With these margins we are able to fit TEXT_LINES * TEXT_COLUMNS characters on one screen.
const _ASSERT_WIDTH_FITS: () = assert!(
    IMAGE_WIDTH
        == MARGIN_LEFT as usize + TEXT_COLUMNS * MESSAGE_FONT.character_size.width as usize + MARGIN_RIGHT as usize
);
const _ASSERT_HEIGHT_FITS: () = assert!(
    IMAGE_HEIGHT
        == MARGIN_TOP as usize + TEXT_LINES * MESSAGE_FONT.character_size.height as usize + MARGIN_BOTTOM as usize
);

pub type DisplaySPI = embassy_rp::peripherals::SPI1;
type Device = st7735_lcd::ST7735<
    ExclusiveDevice<Spi<'static, DisplaySPI, Blocking>, Output<'static>, Delay>,
    Output<'static>,
    Output<'static>,
>;

pub struct ST7735 {
    dev: Device,
    #[allow(dead_code)]
    bl: Output<'static>,
}

#[derive(Debug)]
pub struct DisplayError;

impl From<()> for DisplayError {
    fn from(_: ()) -> Self {
        DisplayError
    }
}

#[derive(Clone, Copy)]
pub enum DisplayOptions {
    PriorityMessage,
    NormalMessage,
}

impl DisplayOptions {
    fn clear_style(self) -> Rgb565 {
        match self {
            DisplayOptions::PriorityMessage => PRIO_MESSAGE_BG_COLOR,
            DisplayOptions::NormalMessage => MESSAGE_BG_COLOR,
        }
    }

    fn textbox_style(self) -> TextBoxStyle {
        match self {
            DisplayOptions::PriorityMessage => TextBoxStyleBuilder::new()
                .height_mode(HeightMode::Exact(VerticalOverdraw::Visible))
                .alignment(HorizontalAlignment::Left)
                .vertical_alignment(VerticalAlignment::Top)
                .build(),
            DisplayOptions::NormalMessage => TextBoxStyleBuilder::new()
                .height_mode(HeightMode::Exact(VerticalOverdraw::Visible))
                .alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Middle)
                .build(),
        }
    }
}

impl ST7735 {
    pub fn new(dev: Device, bl: Output<'static>) -> Self {
        Self { dev, bl }
    }

    pub fn string_formatted(&mut self, text: &str, options: DisplayOptions) -> Result<(), DisplayError> {
        // Margins are not symmetric in the 9x15 font size, so at the bottom and right side there is one pixel less space (+1 in Size::new).
        let bounds = Rectangle::new(
            Point::new(MARGIN_LEFT as i32, MARGIN_TOP as i32),
            Size::new(IMAGE_WIDTH as u32 - MARGIN_RIGHT, IMAGE_HEIGHT as u32 - MARGIN_BOTTOM),
        );

        // Create the text box and apply styling options.
        let text_box = TextBox::with_textbox_style(text, bounds, MESSAGE_TEXT_STYLE, options.textbox_style());

        // Draw the text box.
        self.dev.clear(options.clear_style())?;
        text_box.draw(&mut self.dev)?;
        Ok(())
    }

    pub fn draw_image(&mut self, data: &[u8]) -> Result<(), DisplayError> {
        let raw: ImageRawBE<Rgb565> = ImageRaw::new(data, IMAGE_WIDTH as u32);
        Image::new(&raw, Point::zero()).draw(&mut self.dev)?;
        Ok(())
    }
}
