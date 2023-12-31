//! This example shows how to use SPI (Serial Peripheral Interface) in the RP2040 chip.
//!
//! Example written for a display using the ST7789 chip. Possibly the Waveshare Pico-ResTouch
//! (https://www.waveshare.com/wiki/Pico-ResTouch-LCD-2.8)

#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use core::cell::RefCell;
use core::cmp;
use core::future::pending;

use cyw43::Control;
use cyw43_pio::PioSpi;
use embassy_executor::Spawner;
use embassy_net::{Config, Stack, StackResources};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{
    DMA_CH0, PIN_10, PIN_11, PIN_12, PIN_13, PIN_23, PIN_24, PIN_25, PIN_29, PIN_8, PIN_9, PIO0, SPI1, USB,
};
use embassy_rp::spi::{Blocking, Spi};
use embassy_rp::usb::Driver;
use embassy_rp::{bind_interrupts, pio, spi, usb};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::{with_timeout, Delay, Duration, Instant, Timer};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::image::{Image, ImageRaw, ImageRawBE};
use embedded_graphics::mono_font::ascii::FONT_9X15;
use embedded_graphics::mono_font::{self, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;
use heapless::String;
/// In deploy mode we just want to reboot the device.
#[cfg(feature = "deploy")]
use panic_reset as _;
/// In development mode we want to be able to flash it again.
#[cfg(not(feature = "deploy"))]
use rp2040_panic_usb_boot as _;
use rpi_messages_common::{
    MessageUpdate, MessageUpdateKind, UpdateResult, IMAGE_HEIGHT, IMAGE_WIDTH, TEXT_BUFFER_SIZE,
};
use static_cell::make_static;

use crate::error::{handle_error, Error, Result};
use crate::messagebuf::{format_display_string, DisplayMessage, DisplayOptions, Messages};
use crate::protocol::Protocol;
use crate::static_data::device_id;

mod display;
mod error;
mod messagebuf;
mod protocol;
mod static_data;

const INIT_LOGGING_WAIT: Duration = Duration::from_secs(2);
const INIT_SPI_WAIT: Duration = Duration::from_millis(100);

const DISPLAY_FREQ: u32 = 10_000_000;
const MESSAGE_DISPLAY_DURATION: Duration = Duration::from_secs(5);
const MESSAGE_FONT: mono_font::MonoFont = FONT_9X15;
const MESSAGE_TEXT_COLOR: Rgb565 = Rgb565::BLACK;
const MESSAGE_BG_COLOR: Rgb565 = Rgb565::WHITE;
const PRIO_MESSAGE_BG_COLOR: Rgb565 = Rgb565::RED;
const MESSAGE_TEXT_STYLE: MonoTextStyle<'_, Rgb565> = MonoTextStyle::new(&MESSAGE_FONT, MESSAGE_TEXT_COLOR);
const MESSAGE_FETCH_INTERVAL: Duration = Duration::from_secs(60);
const SERVER_CONNECT_ERROR_WAIT: Duration = Duration::from_secs(2);

/// Global variable to hold message data retrieved from server. No persistence accross reboots.
/// We need the async mutex because we want to do an async read call inside a critical section.
static MESSAGES: Mutex<CriticalSectionRawMutex, RefCell<Messages>> = Mutex::new(RefCell::new(Messages::new()));
static PRIO_MESSAGE_SIGNAL: Signal<CriticalSectionRawMutex, String<TEXT_BUFFER_SIZE>> = Signal::new();

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

async fn handle_update<'a>(update: MessageUpdate, protocol: &mut Protocol<'a>) -> Result<()> {
    log::info!("Received an update. Acquiring mutex to change message buffer.");
    let mut guard = MESSAGES.lock().await;
    let mut messages = RefCell::borrow_mut(&mut guard);

    match update.kind {
        MessageUpdateKind::Text(size) => {
            log::info!("Requesting text update.");
            let message = messages.next_available_text();
            message.set_meta(&update);

            // Since we cannot access the underlying memory of the string directly, we allocate a
            // new buffer here and push it into the string after verifying it is valid UTF-8.
            let mut message_buf = [0u8; TEXT_BUFFER_SIZE];
            // a.d. index cannot panic since size is checked to be less than `TEXT_BUFFER_SIZE` during deserialization.
            let message_buf = &mut message_buf[..(size as usize)];
            protocol.request_update(&update, message_buf).await?;

            match core::str::from_utf8(message_buf) {
                Ok(text) => {
                    log::info!("Received text update: {}", text);
                    // a.d. unwrap() cannot panic since text is at most `TEXT_BUFFER_SIZE` long and message.data is cleared in next_available_text.
                    message.data.text.push_str(text).unwrap();
                }
                Err(e) => {
                    return Err(Error::ServerMessage(e));
                }
            }
        }
        MessageUpdateKind::Image => {
            log::info!("Requesting image update.");
            let message = messages.next_available_image();
            message.set_meta(&update);
            let message_buf = message.data.image.as_mut();
            protocol.request_update(&update, message_buf).await?;
        }
    };

    Ok(())
}

/// This task connects to `MESSAGE_SERVER_ADDR` and fetches new messages to update the global `MESSAGES` struct.
///
/// - `stack`: the network stack. Used to create sockets.
/// - `control`: a driver for the WIFI chip. TODO usage not clear.
#[embassy_executor::task]
async fn fetch_data_task(stack: &'static Stack<cyw43::NetDriver<'static>>, control: &'static mut Control<'static>) {
    let mut tx_buffer = [0; 256];

    // We save the id of the latest message we received to send to the server for the next update check.
    let mut last_message_id = None;

    loop {
        log::info!("Creating new connection.");
        let protocol = Protocol::new(stack, control, &mut tx_buffer).await;
        let mut protocol = match protocol {
            Ok(protocol) => protocol,
            Err(e) => {
                handle_error(e);
                Timer::after(SERVER_CONNECT_ERROR_WAIT).await;
                continue;
            }
        };

        let update_result = loop {
            log::info!("Checking for updates");
            match protocol.check_update(last_message_id).await {
                Err(e) => {
                    break Err(e);
                }
                Ok(UpdateResult::NoUpdate) => {
                    log::info!("No updates for now. Sleeping.");
                    break Ok(());
                }
                // If the update is handled we can update the last_message id, otherwise we exit the loop.
                Ok(UpdateResult::Update(update)) => match handle_update(update, &mut protocol).await {
                    Ok(()) => {
                        last_message_id = Some(cmp::max(last_message_id.unwrap_or(0), update.id));
                    }
                    Err(e) => break Err(e),
                },
            }
        };

        drop(protocol);

        if let Err(e) = update_result {
            handle_error(e);
        }
        Timer::after(MESSAGE_FETCH_INTERVAL).await;
    }
}

/// This task reads messages from the global `MESSAGES` struct and displays a new one every `MESSAGE_DURATION` seconds.
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
        log::info!("Check if priority message exists.");
        let prio_message = with_timeout(MESSAGE_DISPLAY_DURATION, PRIO_MESSAGE_SIGNAL.wait()).await;

        if let Ok(prio_message) = prio_message {
            format_display_string(&prio_message, DisplayOptions::PriorityMessage, display);
        } else {
            log::info!("Timeout! No priority message found.");
            log::info!("Acquiring mutex to read message buffer.");
            let guard = MESSAGES.lock().await;
            let messages = RefCell::borrow(&guard);
            if let Some(next_message) = messages.next_display_message_generic(last_message_time) {
                last_message_time = next_message.meta.updated_at;

                match next_message.data {
                    DisplayMessage::Text(data) => {
                        log::info!("Showing a text message: {}", data.text.as_str());
                        format_display_string(&data.text, DisplayOptions::NormalMessage, display);
                    }
                    DisplayMessage::Image(data) => {
                        log::info!("Showing an image message.");
                        let raw: ImageRawBE<Rgb565> = ImageRaw::new(&data.image, IMAGE_WIDTH as u32);
                        // a.d. unwrap() cannot panic since our display implementation has `Infallibe` as the error type.
                        Image::new(&raw, Point::zero()).draw(display).unwrap();
                    }
                }
            } else {
                format_display_string("No messages :(", DisplayOptions::NormalMessage, display);
            }
        }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    init_usb_logging(spawner, p.USB).await;

    log::info!("Booting device with ID: 0x{:08x}", device_id());

    init_display(
        spawner, p.PIN_8, p.PIN_9, p.SPI1, p.PIN_10, p.PIN_11, p.PIN_12, p.PIN_13,
    )
    .await;
    init_wifi(spawner, p.PIN_23, p.PIN_25, p.PIO0, p.PIN_24, p.PIN_29, p.DMA_CH0).await;

    log::info!("Finished configuration.");
}

/// ----- USB logging setup -----
async fn init_usb_logging(spawner: Spawner, usb: USB) {
    log::info!("USB Logging initialization start.");
    let driver = Driver::new(usb, Irqs);
    spawner
        .spawn(logger_task(driver))
        .expect("Spawning logger_task failed.");

    Timer::after(INIT_LOGGING_WAIT).await;
    log::info!("USB Logging initialization done.");
}

/// ----- Display setup -----
async fn init_display(
    spawner: Spawner,
    dcx: PIN_8,
    display_cs: PIN_9,
    spi: SPI1,
    clk: PIN_10,
    mosi: PIN_11,
    rst: PIN_12,
    bl: PIN_13,
) {
    log::info!("Display initialization start.");

    // create SPI
    let mut display_config = spi::Config::default();
    display_config.frequency = DISPLAY_FREQ;

    // we only have one SPI device so we don't need the SPI bus/SPIDevice machinery.
    // a.d. order does matter, it does not work if DC pin is initialized before SPI
    // maybe some implicit async thing where one of these is actually not completely done before the next python instruction executes
    let spi: Spi<'_, _, Blocking> = Spi::new_blocking_txonly(spi, clk, mosi, display_config);
    Timer::after(INIT_SPI_WAIT).await;

    // dcx: 0 = command, 1 = data
    let dcx = Output::new(dcx, Level::Low);
    let rst = Output::new(rst, Level::Low);
    let display_cs = Output::new(display_cs, Level::Low);
    // Enable LCD backlight
    // TODO Use PWM to regulate
    let bl = Output::new(bl, Level::High);

    // Create display driver which takes care of sending messages to the display.
    let mut display = display::ST7735::new(spi, dcx, rst, display_cs, bl, IMAGE_WIDTH as u8, IMAGE_HEIGHT as u8);

    display.init(&mut Delay);
    // ST7735 is a 162 * 132 controller but it's connected to a 160 * 128 LCD, so we need to set an offset.
    display.set_offset(1, 2);
    // a.d. unwrap() cannot panic since our display implementation has `Infallibe` as the error type.
    display.clear(PRIO_MESSAGE_BG_COLOR).unwrap();
    Text::new("Booting...", Point::new(10, 20), MESSAGE_TEXT_STYLE)
        .draw(&mut display)
        .unwrap();

    spawner
        .spawn(display_messages_task(make_static!(display)))
        .expect("Spawning display_messages_task failed.");
    log::info!("Display initialization end.");
}

/// ----- WIFI setup -----
async fn init_wifi(spawner: Spawner, pwr: PIN_23, cs: PIN_25, pio: PIO0, dio: PIN_24, clk: PIN_29, dma: DMA_CH0) {
    log::info!("WIFI initialization start.");
    let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
    let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");

    let pwr = Output::new(pwr, Level::Low);
    let cs = Output::new(cs, Level::High);
    let mut pio = pio::Pio::new(pio, Irqs);
    let spi = PioSpi::new(&mut pio.common, pio.sm0, pio.irq0, cs, dio, clk, dma);

    let state = make_static!(cyw43::State::new());
    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
    spawner.spawn(wifi_task(runner)).expect("Spawning wifi_task failed.");

    control.init(clm).await;
    // a.d. TODO check which power management mode I want.
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

    spawner.spawn(net_task(stack)).expect("Spawning net_task failed.");

    let wifi_ssid = match static_data::wifi_ssid() {
        Some(wifi_ssid) => {
            if wifi_ssid.is_empty() {
                handle_error(Error::WifiConfiguration);
                pending().await
            } else {
                wifi_ssid
            }
        }
        None => {
            handle_error(Error::MemoryError);
            pending().await
        }
    };

    let wifi_pw = match static_data::wifi_password() {
        Some(wifi_pw) => {
            if wifi_pw.is_empty() {
                handle_error(Error::WifiConfiguration);
                pending().await
            } else {
                wifi_pw
            }
        }
        None => {
            handle_error(Error::MemoryError);
            pending().await
        }
    };

    log::info!("Connecting to Wifi '{}' with password '{}'", wifi_ssid, wifi_pw);

    loop {
        match control.join_wpa2(wifi_ssid, wifi_pw).await {
            Ok(()) => {
                log::info!("WIFI successfully connected.");
                break;
            }
            Err(e) => {
                log::info!("WIFI connection failed with status={}", e.status);
                handle_error(Error::WifiConnect(e));
            }
        }
    }

    spawner
        .spawn(fetch_data_task(stack, make_static!(control)))
        .expect("Spawning fetch_data_task failed.");
    log::info!("WIFI initialization end.");
}
