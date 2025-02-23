//! This example shows how to use SPI (Serial Peripheral Interface) in the RP2040 chip.
//!
//! Example written for a display using the ST7789 chip. Possibly the Waveshare Pico-ResTouch
//! (https://www.waveshare.com/wiki/Pico-ResTouch-LCD-2.8)

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use core::{cmp, future::pending};

use common::{
    consts::{IMAGE_HEIGHT, IMAGE_WIDTH},
    protocol::{CheckUpdateResult, Update, UpdateKind},
};
use cyw43::JoinOptions;
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use embassy_executor::Spawner;
use embassy_net::{self as net, StackResources};
use embassy_rp::{
    bind_interrupts,
    gpio::{Level, Output, Pin},
    peripherals::USB,
    pio::{self, PioPin},
    spi::{self, Blocking, ClkPin, MosiPin, Spi},
    usb::{self, Driver},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, signal::Signal};
use embassy_time::{with_timeout, Delay, Duration, Instant, Timer};
use embedded_graphics::{
    draw_target::DrawTarget,
    image::{Image, ImageRaw, ImageRawBE},
    mono_font::{self, ascii::FONT_9X15, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    text::Text,
};
use embedded_hal_bus::spi::ExclusiveDevice;
use error::ServerMessageError;
use messagebuf::TextData;
/// In deploy mode we just want to reboot the device.
#[cfg(feature = "deploy")]
use panic_reset as _;
/// In development mode we want to be able to flash it again.
#[cfg(not(feature = "deploy"))]
use rp2040_panic_usb_boot as _;
use static_cell::StaticCell;

use crate::error::{handle_error, Error, Result};
use crate::messagebuf::{format_display_string, DisplayMessage, DisplayOptions, Messages};
use crate::static_data::device_id;

mod error;
mod fetch_protocol;
mod messagebuf;
mod static_data;

const INIT_LOGGING_WAIT: Duration = Duration::from_secs(2);
const INIT_SPI_WAIT: Duration = Duration::from_millis(100);
const DISPLAY_FREQ: u32 = 10_000_000;
const PRIO_MESSAGE_DISPLAY_DURATION: Duration = Duration::from_secs(1);
const MESSAGE_DISPLAY_DURATION: Duration = Duration::from_secs(5);
const MESSAGE_FONT: mono_font::MonoFont = FONT_9X15;
const MESSAGE_TEXT_COLOR: Rgb565 = Rgb565::BLACK;
const MESSAGE_BG_COLOR: Rgb565 = Rgb565::WHITE;
const PRIO_MESSAGE_BG_COLOR: Rgb565 = Rgb565::RED;
const MESSAGE_TEXT_STYLE: MonoTextStyle<'_, Rgb565> = MonoTextStyle::new(&MESSAGE_FONT, MESSAGE_TEXT_COLOR);
const MESSAGE_FETCH_INTERVAL: Duration = Duration::from_secs(60);
const SERVER_CONNECT_ERROR_WAIT: Duration = Duration::from_secs(2);

/// Global variable to hold message data retrieved from server. No persistence across reboots.
/// We need the async mutex because we want to do an async read call inside a critical section.
static MESSAGES: Mutex<CriticalSectionRawMutex, Messages> = Mutex::new(Messages::new());
static PRIO_MESSAGE_SIGNAL: Signal<CriticalSectionRawMutex, TextData> = Signal::new();

type DisplaySPI = embassy_rp::peripherals::SPI1;
// a.d. TODO implement DrawTarget & Deref
struct ST7735 {
    dev: st7735_lcd::ST7735<
        ExclusiveDevice<Spi<'static, DisplaySPI, Blocking>, Output<'static>, Delay>,
        Output<'static>,
        Output<'static>,
    >,
    bl: Output<'static>,
}
type WifiPIO = embassy_rp::peripherals::PIO0;
type WifiDMA = embassy_rp::peripherals::DMA_CH0;

// TODO why do we need this?
// It seems to associate a type of interrupt that the CPU knows about with a handler (so maybe populating the interrupt vector?)
bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => pio::InterruptHandler<WifiPIO>;
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
});

//// ---- System tasks for managing peripherals/debug. -------------------------
mod system_tasks {
    use super::*;

    /// Interacts with the WIFI chip over some internal SPI.
    #[embassy_executor::task]
    pub(super) async fn cyw43(
        runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, WifiPIO, 0, WifiDMA>>,
    ) -> ! {
        runner.run().await
    }

    /// Manages the network stack (so I guess it handles connections, creating sockets and actually sending stuff over sockets).
    #[embassy_executor::task]
    pub(super) async fn net(mut runner: net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
        runner.run().await
    }

    /// Sets the global logger and sends log messages over USB.
    #[embassy_executor::task]
    pub(super) async fn logger(driver: Driver<'static, USB>) {
        embassy_usb_logger::run!(1024, log::LevelFilter::Debug, driver);
    }
}

//// ---- Main tasks to implement the device features. ------------------------
mod main_tasks {
    use super::*;

    async fn handle_update<'a>(update: Update, protocol: &mut fetch_protocol::Socket<'_>) -> Result<()> {
        log::info!("Received an update. Acquiring mutex to change message buffer.");
        let mut guard = MESSAGES.lock().await;
        let messages: &mut Messages = &mut guard;

        match update.kind {
            UpdateKind::Text(_) => {
                log::info!("Requesting text update.");
                let message = messages.next_available_text();
                message.set_meta(&update);

                // SAFETY - We read the bytes from the network into the next available text message.
                // If that fails -- in which case the buffer could be half-filled -- or if the buffer does not contain valid UTF-8 in the end, we clear the string.
                // We are holding the message lock so no one else can access the the unsafe buffer contents while this future may be paused.
                unsafe {
                    let message_buf = message.data.text.as_mut_vec();
                    if let Err(e) = protocol.request_update(&update, message_buf).await {
                        message_buf.clear();
                        return Err(e);
                    }

                    match core::str::from_utf8(message_buf) {
                        Ok(text) => {
                            log::info!("Received text update: {}", text);
                        }
                        Err(e) => {
                            message_buf.clear();
                            return Err(Error::ServerMessage(ServerMessageError::Encoding(e)));
                        }
                    }
                }
            }
            UpdateKind::Image => {
                log::info!("Requesting image update.");
                let message = messages.next_available_image();
                message.set_meta(&update);
                let message_buf = message.data.image.as_mut();
                protocol.request_update(&update, message_buf).await?;
            }
        };

        Ok(())
    }

    /// This task connects to the configured server and periodically fetches new messages to update the global [`MESSAGES`] object.
    ///
    /// - [`stack`]: The network stack. Used to create sockets.
    /// - [`control`]: The driver of the WIFI chip. TODO usage not clear.
    #[embassy_executor::task]
    pub(super) async fn fetch_data(
        mut state: fetch_protocol::State,
        stack: net::Stack<'static>,
        mut control: cyw43::Control<'static>,
    ) {
        // We save the id of the latest message we received to send to the server for the next update check.
        let mut last_message_id = None;

        loop {
            log::info!("Creating new connection.");
            let protocol = fetch_protocol::Socket::new(&mut state, stack, &mut control).await;
            let mut protocol = match protocol {
                Ok(protocol) => protocol,
                Err(e) => {
                    handle_error(e);
                    Timer::after(SERVER_CONNECT_ERROR_WAIT).await;
                    continue;
                }
            };

            // We loop as long as we successfully receive new message updates. Every other case exits the loop.
            let update_result = loop {
                log::info!("Checking for updates");
                match protocol.check_update(last_message_id).await {
                    Err(e) => {
                        break Err(e);
                    }
                    Ok(CheckUpdateResult::NoUpdate) => {
                        log::info!("No updates for now. Sleeping.");
                        break Ok(());
                    }
                    Ok(CheckUpdateResult::Update(update)) => match handle_update(update, &mut protocol).await {
                        Ok(()) => {
                            last_message_id = Some(cmp::max(last_message_id.unwrap_or(0), update.id));
                        }
                        Err(e) => break Err(e),
                    },
                }
            };

            protocol.abort().await;

            if let Err(e) = update_result {
                handle_error(e);
            }
            Timer::after(MESSAGE_FETCH_INTERVAL).await;
        }
    }

    /// Periodically get the next messages from the global [`MESSAGES`] object and display it.
    ///
    /// - [`display`]: a driver to interact with the display's ST7735 chip.
    #[embassy_executor::task]
    pub(super) async fn display_messages(mut display: ST7735) {
        let mut last_message_time = Instant::MIN;
        let mut prio_message_opt: Option<TextData> = None;

        // Each time the loop is entered we immediately display a priority message if we are currently holding one.
        // Priority messages are shown for `PRIO_MESSAGE_DISPLAY_DURATION` or until a new priority message arrives.
        // If there is no priority message we display the next non-priority message and then wait for `MESSAGE_DISPLAY_DURATION`
        // or until a new priority message arrives.
        loop {
            log::info!("Check if priority message exists.");

            if let Some(prio_message) = prio_message_opt.take() {
                format_display_string(&prio_message.text, DisplayOptions::PriorityMessage, &mut display);

                prio_message_opt = with_timeout(PRIO_MESSAGE_DISPLAY_DURATION, PRIO_MESSAGE_SIGNAL.wait())
                    .await
                    .ok();
            } else {
                log::info!("No priority message found. Acquiring mutex to read message buffer.");
                let guard = MESSAGES.lock().await;
                let messages: &Messages = &guard;
                if let Some(next_message) = messages.next_display_message_generic(last_message_time) {
                    last_message_time = next_message.meta.updated_at;

                    match next_message.data {
                        DisplayMessage::Text(data) => {
                            log::info!("Showing a text message: {}", data.text.as_str());
                            format_display_string(&data.text, DisplayOptions::NormalMessage, &mut display);
                        }
                        DisplayMessage::Image(data) => {
                            log::info!("Showing an image message.");
                            let raw: ImageRawBE<Rgb565> = ImageRaw::new(&data.image, IMAGE_WIDTH as u32);
                            // a.d. unwrap() cannot panic since our display implementation has `Infallibe` as the error type.
                            Image::new(&raw, Point::zero()).draw(&mut display.dev).unwrap();
                        }
                    }
                } else {
                    format_display_string("No messages :(", DisplayOptions::NormalMessage, &mut display);
                }

                prio_message_opt = with_timeout(MESSAGE_DISPLAY_DURATION, PRIO_MESSAGE_SIGNAL.wait())
                    .await
                    .ok();
            }
        }
    }
}

//// ---- Hardware initialization functions. ----------------------------------

mod init {
    // a.d. TODO fix imports?
    use super::*;

    /// ----- USB logging setup -----
    pub(super) async fn usb(usb: USB) -> usb::Driver<'static, USB> {
        Driver::new(usb, Irqs)
    }

    /// ----- Display setup -----
    pub(super) async fn display(
        bl: impl Pin,
        cs: impl Pin,
        dcx: impl Pin,
        rst: impl Pin,
        spi: DisplaySPI,
        clk: impl ClkPin<DisplaySPI>,
        mosi: impl MosiPin<DisplaySPI>,
    ) -> ST7735 {
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
        let rst: Output = Output::new(rst, Level::Low);
        let display_cs = Output::new(cs, Level::High);
        // Enable LCD backlight
        // TODO Use PWM to regulate
        let bl = Output::new(bl, Level::High);

        // Create display driver which takes care of sending messages to the display.
        let spi_dev = ExclusiveDevice::new(spi, display_cs, Delay).unwrap();
        // let mut display = display::ST7735::new(spi, dcx, rst, display_cs, bl, IMAGE_WIDTH as u8, IMAGE_HEIGHT as u8);
        let mut display =
            st7735_lcd::ST7735::new(spi_dev, dcx, rst, true, false, IMAGE_WIDTH as u32, IMAGE_HEIGHT as u32);

        display.init(&mut Delay).unwrap();
        // ST7735 is a 162 * 132 controller but it's connected to a 160 * 128 LCD, so we need to set an offset.
        // display.set_offset(1, 2);
        display
            .set_orientation(&st7735_lcd::Orientation::LandscapeSwapped)
            .unwrap();
        // a.d. unwrap() cannot panic since our display implementation has `Infallibe` as the error type.
        display.clear(PRIO_MESSAGE_BG_COLOR).unwrap();
        Text::new("Booting...", Point::new(10, 20), MESSAGE_TEXT_STYLE)
            .draw(&mut display)
            .unwrap();

        ST7735 { dev: display, bl }
    }

    /// ----- WIFI setup -----
    pub(super) async fn cyw43(
        pwr: impl Pin,
        cs: impl Pin,
        pio: WifiPIO,
        dio: impl PioPin,
        clk: impl PioPin,
        dma: WifiDMA,
    ) -> (
        cyw43::NetDriver<'static>,
        cyw43::Control<'static>,
        cyw43::Runner<'static, Output<'static>, PioSpi<'static, WifiPIO, 0, WifiDMA>>,
    ) {
        log::info!("WIFI initialization start.");
        let fw = include_bytes!("../cyw43-firmware/43439A0.bin");
        let clm = include_bytes!("../cyw43-firmware/43439A0_clm.bin");

        let pwr = Output::new(pwr, Level::Low);
        let cs = Output::new(cs, Level::High);
        let mut pio = pio::Pio::new(pio, Irqs);
        let spi = PioSpi::new(
            &mut pio.common,
            pio.sm0,
            DEFAULT_CLOCK_DIVIDER,
            pio.irq0,
            cs,
            dio,
            clk,
            dma,
        );

        static STATE: StaticCell<cyw43::State> = StaticCell::new();
        let state = STATE.init_with(cyw43::State::new);
        let (device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;

        control.init(clm).await;
        // a.d. TODO check which power management mode I want.
        control
            .set_power_management(cyw43::PowerManagementMode::PowerSave)
            .await;

        (device, control, runner)
    }

    /// Setup network stack.
    pub(super) async fn net(
        net_device: cyw43::NetDriver<'static>,
    ) -> (net::Stack<'static>, net::Runner<'static, cyw43::NetDriver<'static>>) {
        let config = net::Config::dhcpv4(Default::default());
        let seed = 0x0981_a34b_8288_01ff;

        // Init network stack
        static RESOURCES: StaticCell<StackResources<2>> = StaticCell::new();
        net::new(net_device, config, RESOURCES.init(StackResources::new()), seed)
    }

    /// Setup WIFI connection.
    pub(super) async fn wifi(control: &mut cyw43::Control<'static>) {
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

        log::info!("Connecting to Wifi '{}'.", wifi_ssid);
        log::info!("With password '{:?}'", wifi_pw);
        // TODO no need to parse it anymore

        loop {
            let options = JoinOptions::new(wifi_pw.as_bytes());
            match control.join(wifi_ssid, options).await {
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
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    let protocol_state = fetch_protocol::State::take();

    let usb_driver = init::usb(p.USB).await;
    spawner
        .spawn(system_tasks::logger(usb_driver))
        .expect("Spawning logger_task failed.");
    Timer::after(INIT_LOGGING_WAIT).await;

    log::info!("Booting device with ID: 0x{:08x}", device_id());

    let display = init::display(p.PIN_6, p.PIN_7, p.PIN_8, p.PIN_9, p.SPI1, p.PIN_10, p.PIN_11).await;
    spawner
        .spawn(main_tasks::display_messages(display))
        .expect("Spawning display_messages_task failed.");
    let (cyw43_driver, mut cyw43_control, cyw43_runner) =
        init::cyw43(p.PIN_23, p.PIN_25, p.PIO0, p.PIN_24, p.PIN_29, p.DMA_CH0).await;
    spawner
        .spawn(system_tasks::cyw43(cyw43_runner))
        .expect("Spawning cyw43_task failed.");
    let (net_stack, net_runner) = init::net(cyw43_driver).await;
    spawner
        .spawn(system_tasks::net(net_runner))
        .expect("Spawning net_task failed.");

    init::wifi(&mut cyw43_control).await;
    spawner
        .spawn(main_tasks::fetch_data(protocol_state, net_stack, cyw43_control))
        .expect("Spawning fetch_data_task failed.");

    log::info!("Finished configuration.");
}
