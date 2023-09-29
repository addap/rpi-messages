//! This example shows how to use SPI (Serial Peripheral Interface) in the RP2040 chip.
//!
//! Example written for a display using the ST7789 chip. Possibly the Waveshare Pico-ResTouch
//! (https://www.waveshare.com/wiki/Pico-ResTouch-LCD-2.8)

#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use core::cell::RefCell;

use cyw43::Control;
use cyw43_pio::PioSpi;
use embassy_executor::Spawner;
use embassy_net::{Config, Stack, StackResources};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIN_23, PIN_25, PIO0, USB};
use embassy_rp::pwm::Pwm;
use embassy_rp::spi::{Blocking, Spi};
use embassy_rp::usb::Driver;
use embassy_rp::{bind_interrupts, pio, spi, usb};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Delay, Duration, Instant, Timer};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::image::{Image, ImageRaw, ImageRawBE};
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::mono_font::{self, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
// use panic_probe as _;
use rp2040_panic_usb_boot as _;
use rpi_messages_common::{MessageUpdateKind, UpdateResult, IMAGE_BUFFER_SIZE, IMAGE_WIDTH};
use rpi_messages_pico::messagebuf::{GenericMessage, Messages};
use rpi_messages_pico::protocol::Protocol;
use st7735_lcd::{Orientation, ST7735};
use static_cell::make_static;

const DISPLAY_FREQ: u32 = 10_000_000;

const MESSAGE_DISPLAY_DURATION: Duration = Duration::from_secs(5);
const MESSAGE_FONT: mono_font::MonoFont = FONT_10X20;
const MESSAGE_TEXT_COLOR: Rgb565 = Rgb565::BLACK;
const MESSAGE_CLEAR_COLOR: Rgb565 = Rgb565::WHITE;
const MESSAGE_TEXT_STYLE: MonoTextStyle<'_, Rgb565> = MonoTextStyle::new(&MESSAGE_FONT, MESSAGE_TEXT_COLOR);

const WIFI_SSID: &str = env!("WIFI_SSID");
const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");
const MESSAGE_FETCH_INTERVAL: Duration = Duration::from_secs(60);
const SERVER_CONNECT_ERROR_WAIT: Duration = Duration::from_secs(10);

/// Global variable to hold message data retrieved from server. No persistence accross reboots.
/// We need the async mutex because we want to do an async read call inside a critical section.
static MESSAGES: Mutex<CriticalSectionRawMutex, RefCell<Messages>> = Mutex::new(RefCell::new(Messages::new()));

static mut fb: [u8; IMAGE_BUFFER_SIZE] = [0u8; IMAGE_BUFFER_SIZE];

static imo: &'static [u8; IMAGE_BUFFER_SIZE] = include_bytes!("../../../pictures/loveimo.bin");

// TODO why do we need this?
// It seems to associate a type of interrupt that the CPU knows about with a handler (so maybe populating the interrupt vector?)
bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
});

//// ----- Some systems tasks for managing peripherals/debug. -----

/// Interacts with the WIFI chip over some internal SPI.
#[embassy_executor::task]
async fn wifi_task(
    runner: cyw43::Runner<'static, Output<'static, PIN_23>, PioSpi<'static, PIN_25, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

/// Manages the network stack (so I guess it handles connections, creating sockets and actually sending stuff over sockets).
#[embassy_executor::task]
async fn net_task(stack: &'static Stack<cyw43::NetDriver<'static>>) -> ! {
    stack.run().await
}

/// Sets the global logger and sends log messages over USB.
#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Debug, driver);
}

//// ----- Main tasks to implement the device features. -----

/// This task connects to `MESSAGE_SERVER_ADDR` and fetches new messages to update the global `MESSAGES` struct.
///
/// - `stack`: the network stack. Used to create sockets.
/// - `control`: a driver for the WIFI chip. TODO usage not clear.
#[embassy_executor::task]
async fn fetch_data_task(stack: &'static Stack<cyw43::NetDriver<'static>>, control: &'static mut Control<'static>) {
    let mut tx_buffer = [0; 256];

    loop {
        // Nested block to drop protocol before we await the timeout.
        {
            log::info!("Creating new connection.");
            let protocol_res = Protocol::new(stack, control, &mut tx_buffer).await;
            let mut protocol = match protocol_res {
                Ok(protocol) => protocol,
                Err(e) => {
                    log::warn!("Connection error: {:?}", e);
                    Timer::after(SERVER_CONNECT_ERROR_WAIT).await;
                    continue;
                }
            };

            loop {
                log::info!("Checking for updates");
                match protocol.check_update().await {
                    None => {
                        log::warn!("update parse error");
                        break;
                    }
                    Some(UpdateResult::NoUpdate) => {
                        log::info!("No updates for now. Sleeping.");
                        break;
                    }
                    Some(UpdateResult::Update(update)) => {
                        log::info!("Received an update. Acquiring mutex to change message buffer.");
                        let mut guard = MESSAGES.lock().await;
                        let mut messages = RefCell::borrow_mut(&mut guard);

                        match update.kind {
                            MessageUpdateKind::Text(_) => {
                                log::info!("Requesting text update.");
                                let message = messages.next_available_text();
                                message.set_meta(&update);
                                unsafe {
                                    let message_buf = message.data.text.as_bytes_mut();
                                    protocol.request_update(&update, message_buf).await;
                                    if core::str::from_utf8(&message_buf).is_err() {
                                        log::warn!("Received invalid utf8 from server");
                                        message_buf.fill(0)
                                    }
                                }
                            }
                            MessageUpdateKind::Image => {
                                log::info!("Requesting image update.");
                                let message = messages.next_available_image();
                                message.set_meta(&update);
                                let message_buf = message.data.image.as_mut();
                                protocol.request_update(&update, message_buf).await;
                            }
                        }
                    }
                }
            }
        }

        Timer::after(MESSAGE_FETCH_INTERVAL).await;
    }
}

/// This task reads messages from the global `MESSAGES` struct and displays a new one every `MESSAGE_DURATION` seconds.
/// TODO add some queue for status messages (wifi problems, can't find server, etc.) which have priority over `MESSAGES`.
///
/// - `display`: a driver to interact with the display's ST7735 chip over SPI.
#[embassy_executor::task]
async fn display_messages_task(
    display: &'static mut display::ST7735<
        Spi<'_, embassy_rp::peripherals::SPI1, Blocking>,
        Output<'_, embassy_rp::peripherals::PIN_8>,
        Output<'_, embassy_rp::peripherals::PIN_12>,
        Output<'_, embassy_rp::peripherals::PIN_9>,
        Output<'_, embassy_rp::peripherals::PIN_13>,
    >,
) {
    let mut last_message_time = Instant::MIN;

    loop {
        {
            log::info!("Acquiring mutex to read message buffer.");
            let guard = MESSAGES.lock().await;
            let messages = RefCell::borrow(&guard);
            let next_message = messages.next_display_message_generic(last_message_time);
            last_message_time = next_message.updated_at();

            match next_message {
                GenericMessage::Text(text) => {
                    log::info!("Showing a text message: {}", text.data.text.as_str());
                    Text::new(text.data.text.as_str(), Point::new(20, 100), MESSAGE_TEXT_STYLE)
                        .draw(display)
                        .unwrap();
                    // TODO add logic to add linebreaks/margins
                }
                GenericMessage::Image(image) => {
                    log::info!("Showing an image message.");
                    let raw: ImageRawBE<Rgb565> = ImageRaw::new(&image.data.image, IMAGE_WIDTH as u32);
                    Image::new(&raw, Point::zero()).draw(display).unwrap();
                }
            }
        }

        Timer::after(MESSAGE_DISPLAY_DURATION).await;
        display.clear(MESSAGE_CLEAR_COLOR).unwrap();
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // ----- USB logging setup -----
    {
        let driver = Driver::new(p.USB, Irqs);
        spawner.spawn(logger_task(driver)).unwrap();

        Timer::after(Duration::from_secs(5)).await;
        log::info!("Hello World!");
    }

    log::info!("2");
    // ----- WIFI setup -----
    {
        let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
        let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");

        let pwr = Output::new(p.PIN_23, Level::Low);
        let cs = Output::new(p.PIN_25, Level::High);
        let mut pio = pio::Pio::new(p.PIO0, Irqs);
        let spi = PioSpi::new(&mut pio.common, pio.sm0, pio.irq0, cs, p.PIN_24, p.PIN_29, p.DMA_CH0);

        let state = make_static!(cyw43::State::new());
        let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
        spawner.spawn(wifi_task(runner)).unwrap();

        control.init(clm).await;
        control
            .set_power_management(cyw43::PowerManagementMode::PowerSave)
            .await;

        let config = Config::dhcpv4(Default::default());

        let seed = 0x0981_a34b_8288_01ff;

        // Init network stack
        let stack = make_static!(Stack::new(
            net_device,
            config,
            make_static!(StackResources::<2>::new()),
            seed
        ));

        log::info!("3");
        spawner.spawn(net_task(stack)).unwrap();
        log::info!("4");

        loop {
            match control.join_wpa2(WIFI_SSID, WIFI_PASSWORD).await {
                Ok(_) => {
                    log::info!("join suceeded {}", "what?");
                    break;
                }
                Err(err) => {
                    log::info!("join failed with status={}", err.status);
                }
            }
        }

        log::info!("5");
        spawner.spawn(fetch_data_task(stack, make_static!(control))).unwrap();
    }

    // ----- Display setup -----
    {
        let bl = p.PIN_13;
        let rst = p.PIN_12;
        let display_cs = p.PIN_9;
        let dcx = p.PIN_8;
        let mosi = p.PIN_11;
        let clk = p.PIN_10;

        // create SPI
        let mut display_config = spi::Config::default();
        display_config.frequency = DISPLAY_FREQ;

        log::info!("6");
        // we only have one SPI device so we don't need the SPI bus/SPIDevice machinery.
        // a.d. order does matter, it does not work if DC pin is initialized before SPI
        // maybe some implicit async thing where one of these is actually not completely done before the next python instruction executes
        let spi: Spi<'_, _, Blocking> = Spi::new_blocking_txonly(p.SPI1, clk, mosi, display_config);

        Timer::after(Duration::from_millis(100)).await;

        log::info!("7");
        // dcx: 0 = command, 1 = data
        let dcx = Output::new(dcx, Level::Low);
        let rst = Output::new(rst, Level::Low);
        let display_cs = Output::new(display_cs, Level::Low);
        // Enable LCD backlight
        // TODO Use PWM to regulate
        let bl = Output::new(bl, Level::High);

        // a.d. The library I want to use instead but somehow does not work atm even though it worked earlier.
        // let mut display = ST7735::new(spi, dcx, rst, true, false, 160, 128);
        // // display.init(&mut Delay).unwrap();
        // display.set_orientation(&Orientation::Landscape).unwrap();
        // display.set_offset(1, 2);
        // display.clear(Rgb565::RED).unwrap();

        // Create display driver which takes care of sending messages to the display.
        // SAFETY - we only borrow the framebuffer once in this setup procedure, so there will never be multiple mutable references.
        let fbr: &'static mut [u8; IMAGE_BUFFER_SIZE] = unsafe { &mut fb };
        let mut display = display::ST7735::new(spi, dcx, rst, display_cs, bl, fbr, 160, 128);

        log::info!("8");
        display.init(&mut Delay);
        // ST7735 is a 162 * 132 controller but it's connected to a 160 * 128 LCD, so we need to set an offset.
        display.set_offset(1, 2);
        log::info!("9");

        display.clear(Rgb565::RED).unwrap();
        // display.fill(imo);
        log::info!("10");
        // display.show();

        spawner.spawn(display_messages_task(make_static!(display))).unwrap();
    }
    log::info!("Finished configuration.");
}

mod display {
    use core::cell::RefCell;

    use embedded_graphics::framebuffer::buffer_size;
    use embedded_graphics::pixelcolor::raw::RawU16;
    use embedded_graphics::pixelcolor::Rgb565;
    use embedded_graphics::prelude::{DrawTarget, OriginDimensions, PixelColor, Point, RawData, RgbColor, Size};
    use embedded_graphics::primitives::{PointsIter, Rectangle};
    use embedded_graphics::Pixel;
    use embedded_hal_02::blocking::delay::DelayMs;
    use embedded_hal_02::blocking::spi;
    use embedded_hal_02::digital::v2::OutputPin;
    use rpi_messages_common::{MessageUpdateKind, UpdateResult, IMAGE_BUFFER_SIZE, IMAGE_WIDTH};

    pub struct ST7735<SPI, DC, RST, CS, BL>
    where
        SPI: spi::Write<u8>,
        DC: OutputPin,
        RST: OutputPin,
        CS: OutputPin,
        // BL: Pwm,
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
        bl: BL,

        /// Frame buffer.
        buffer: &'static mut [u8; IMAGE_BUFFER_SIZE],

        width: u8,
        height: u8,
        dx: u8,
        dy: u8,
    }

    impl<SPI, DC, RST, CS, BL> ST7735<SPI, DC, RST, CS, BL>
    where
        SPI: spi::Write<u8>,
        DC: OutputPin,
        RST: OutputPin,
        CS: OutputPin,
        BL: OutputPin,
    {
        pub fn new(
            spi: SPI,
            dc: DC,
            rst: RST,
            cs: CS,
            bl: BL,
            buffer: &'static mut [u8; IMAGE_BUFFER_SIZE],
            width: u8,
            height: u8,
        ) -> Self {
            Self {
                spi,
                dc,
                rst,
                cs,
                bl,
                buffer,
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
            self.reset();
            self.init_reg(delay);
        }

        fn write_cmd(&mut self, cmd: u8) {
            self.cs.set_high();
            self.dc.set_low();
            self.cs.set_low();
            self.spi.write(&[cmd]);
            self.cs.set_high();
        }

        fn write_data(&mut self, data: u8) {
            self.cs.set_high();
            self.dc.set_high();
            self.cs.set_low();
            self.spi.write(&[data]);
            self.cs.set_high();
        }

        fn module_init(&mut self) {
            // don't select chip in the beginning
            self.cs.set_high();

            // # configure pwm
            // pwm = PWM(self.bl)
            // pwm.freq(1000)
            // pwm.duty_u16(32768)  # max 65535
            //
        }

        fn reset(&mut self) {
            self.rst.set_high();
            self.rst.set_low();
            self.rst.set_high();
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

        pub fn fill(&mut self, new: &[u8; IMAGE_BUFFER_SIZE]) {
            for i in 0..IMAGE_BUFFER_SIZE {
                self.buffer[i] = new[i];
            }
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

            self.cs.set_high();
            self.dc.set_high();
            self.cs.set_low();
            self.spi.write(&color.to_be_bytes());
            self.cs.set_high();
        }

        pub fn show(&mut self) {
            self.set_address_window(0, 0, self.width - 1, self.height - 1);
            self.write_cmd(0x2C);

            self.cs.set_high();
            self.dc.set_high();
            self.cs.set_low();
            self.spi.write(self.buffer);
            self.cs.set_high();
        }

        pub fn show2(&mut self) {
            self.write_cmd(0x2A);
            self.write_data(0x00);
            self.write_data(self.dx);
            self.write_data(0x00);
            self.write_data(self.width - 1 + self.dx);

            self.write_cmd(0x2B);
            self.write_data(0x00);
            self.write_data(self.dy);
            self.write_data(0x00);
            self.write_data(self.height - 1 + self.dy);

            self.write_cmd(0x2C);

            self.cs.set_high();
            self.dc.set_high();
            self.cs.set_low();

            let color = [0xf8, 0x00];
            const _buffer_size: usize = 160 * 128 * 2;
            let mut _buffer = [0u8; _buffer_size];
            for j in (0.._buffer_size).step_by(2) {
                _buffer[j] = color[0];
                _buffer[j + 1] = color[1];
            }

            self.spi.write(&_buffer);
            self.cs.set_high();
        }
    }

    impl<SPI, DC, RST, CS, BL> DrawTarget for ST7735<SPI, DC, RST, CS, BL>
    where
        SPI: spi::Write<u8>,
        DC: OutputPin,
        RST: OutputPin,
        CS: OutputPin,
        BL: OutputPin,
    {
        type Error = ();
        type Color = Rgb565;

        fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
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

        fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
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

                self.cs.set_high();
                self.dc.set_high();
                self.cs.set_low();

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
                        self.spi.write(&buffer);
                        index = 0;
                    }
                }
                self.spi.write(&buffer[0..index]);

                self.cs.set_high();
            }
            Ok(())
        }
    }

    impl<SPI, DC, RST, CS, BL> OriginDimensions for ST7735<SPI, DC, RST, CS, BL>
    where
        SPI: spi::Write<u8>,
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
}
