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
use panic_probe as _;
use rpi_messages_common::{MessageUpdateKind, UpdateResult, IMAGE_WIDTH};
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
            // let protocol_res = Protocol::new(stack, control, &mut tx_buffer).await;
            // let mut protocol = match protocol_res {
            //     Ok(protocol) => protocol,
            //     Err(e) => {
            //         log::warn!("Connection error: {:?}", e);
            //         Timer::after(SERVER_CONNECT_ERROR_WAIT).await;
            //         continue;
            //     }
            // };

            // loop {
            //     log::info!("Checking for updates");
            //     match protocol.check_update().await {
            //         None => {
            //             log::warn!("update parse error");
            //             break;
            //         }
            //         Some(UpdateResult::NoUpdate) => {
            //             log::info!("No updates for now. Sleeping.");
            //             break;
            //         }
            //         Some(UpdateResult::Update(update)) => {
            //             log::info!("Received an update. Acquiring mutex to change message buffer.");
            //             let mut guard = MESSAGES.lock().await;
            //             let mut messages = RefCell::borrow_mut(&mut guard);

            //             match update.kind {
            //                 MessageUpdateKind::Text(_) => {
            //                     log::info!("Requesting text update.");
            //                     let message = messages.next_available_text();
            //                     message.set_meta(&update);
            //                     unsafe {
            //                         let message_buf = message.data.text.as_bytes_mut();
            //                         protocol.request_update(&update, message_buf).await;
            //                         if core::str::from_utf8(&message_buf).is_err() {
            //                             log::warn!("Received invalid utf8 from server");
            //                             message_buf.fill(0)
            //                         }
            //                     }
            //                 }
            //                 MessageUpdateKind::Image => {
            //                     log::info!("Requesting image update.");
            //                     let message = messages.next_available_image();
            //                     message.set_meta(&update);
            //                     let message_buf = message.data.image.as_mut();
            //                     protocol.request_update(&update, message_buf).await;
            //                 }
            //             }
            //         }
            //     }
            // }
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
    display: &'static mut ST7735<
        Spi<'_, embassy_rp::peripherals::SPI1, Blocking>,
        Output<'_, embassy_rp::peripherals::PIN_8>,
        Output<'_, embassy_rp::peripherals::PIN_12>,
    >,
) {
    let mut last_message_time = Instant::MIN;
    let mut i = 0;

    loop {
        // {
        log::info!("Acquiring mutex to read message buffer.");
        //     let guard = MESSAGES.lock().await;
        //     let messages = RefCell::borrow(&guard);
        //     let next_message = messages.next_display_message_generic(last_message_time);
        //     last_message_time = next_message.updated_at();

        //     match next_message {
        //         GenericMessage::Text(text) => {
        //             log::info!("Showing a text message: {}", text.data.text.as_str());

        //             // TODO add logic to add linebreaks/margins
        //             Text::new(text.data.text.as_str(), Point::new(20, 100), MESSAGE_TEXT_STYLE)
        //                 .draw(display)
        //                 .unwrap();
        //         }
        //         GenericMessage::Image(image) => {
        //             log::info!("Showing an image message.");
        //             let raw: ImageRawBE<Rgb565> = ImageRaw::new(&image.data.image, IMAGE_WIDTH as u32);
        //             Image::new(&raw, Point::zero()).draw(display).unwrap();
        //         }
        //     }
        // }

        i += 1;

        Timer::after(MESSAGE_DISPLAY_DURATION).await;

        match i % 5 {
            0 => {
                if let Err(e) = display.clear(Rgb565::BLUE) {
                    log::warn!("Error {:?}", e);
                }
            }
            1 => {
                if let Err(e) = display.clear(Rgb565::RED) {
                    log::warn!("Error {:?}", e);
                }
            }
            2 => {
                if let Err(e) = display.clear(Rgb565::YELLOW) {
                    log::warn!("Error {:?}", e);
                }
            }
            3 => {
                if let Err(e) = display.clear(Rgb565::GREEN) {
                    log::warn!("Error {:?}", e);
                }
            }
            4 => {
                if let Err(e) = display.clear(Rgb565::MAGENTA) {
                    log::warn!("Error {:?}", e);
                }
            }
            _ => {}
        }

        Timer::after(Duration::from_secs(1)).await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    // log::info!("1");

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
        let spi: Spi<'_, _, Blocking> = Spi::new_blocking_txonly(p.SPI1, clk, mosi, display_config.clone());

        log::info!("7");
        // dcx: 0 = command, 1 = data
        let dcx = Output::new(dcx, Level::Low);
        let rst = Output::new(rst, Level::Low);
        // Not used afterwards but we initialize it because it should always be low.
        let _ = Output::new(display_cs, Level::Low);

        // Enable LCD backlight
        // TODO Use PWM to regulate
        let _ = Output::new(bl, Level::High);

        // Create display driver which takes care of sending messages to the display.
        let mut display = ST7735::new(spi, dcx, rst, true, false, 160, 128);

        log::info!("8");
        display.init(&mut Delay).unwrap();
        display.set_orientation(&Orientation::Landscape).unwrap();
        // ST7735 is a 162 * 132 controller but it's connected to a 160 * 128 LCD, so we need to set an offset.
        display.set_offset(1, 2);
        log::info!("9");

        display.clear(Rgb565::GREEN).unwrap();
        log::info!("10");

        // spawner.spawn(display_messages_task(make_static!(display))).unwrap();
    }
    log::info!("Finished configuration.");
}
