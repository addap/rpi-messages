//! This example shows how to use SPI (Serial Peripheral Interface) in the RP2040 chip.
//!
//! Example written for a display using the ST7789 chip. Possibly the Waveshare Pico-ResTouch
//! (https://www.waveshare.com/wiki/Pico-ResTouch-LCD-2.8)

#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use core::{cmp, future::pending};

use assign_resources::assign_resources;
use common::{
    consts::{IMAGE_HEIGHT, IMAGE_WIDTH},
    protocols::pico::RequestUpdateResult,
};
use cortex_m_rt::entry;
use cyw43::JoinOptions;
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use embassy_executor::{Executor, InterruptExecutor, SendSpawner, Spawner};
use embassy_net::{self as net, StackResources};
use embassy_rp::interrupt;
use embassy_rp::{
    bind_interrupts,
    gpio::{Level, Output},
    interrupt::{InterruptExt, Priority},
    peripherals::{self, USB},
    pio,
    spi::{self, Blocking, Spi},
    usb::{self, Driver},
};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, mutex::Mutex, signal::Signal};
use embassy_time::{Delay, Duration, Instant, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use messagebuf::TextData;
/// In deploy mode we just want to reboot the device.
#[cfg(feature = "deploy")]
use panic_reset as _;
/// In development mode we want to be able to flash it again.
#[cfg(not(feature = "deploy"))]
use rp2040_panic_usb_boot as _;
use static_cell::StaticCell;

use crate::messagebuf::Messages;
use crate::static_data::device_id;
use crate::{
    display::ST7735,
    error::{handle_soft_error, Result, SoftError},
};

mod display;
mod error;
mod fetch_data;
mod messagebuf;
mod static_data;

const PRIO_MESSAGE_DISPLAY_DURATION: Duration = Duration::from_secs(3);
const MESSAGE_DISPLAY_DURATION: Duration = Duration::from_secs(5);
const MESSAGE_FETCH_INTERVAL: Duration = Duration::from_secs(60);
const SERVER_CONNECT_ERROR_WAIT: Duration = Duration::from_secs(2);

// a.d. TODO can we drop down to a Noop mutex? depends on if we access messages from difference executors.
/// Global variable to hold message data retrieved from server. No persistence across reboots.
/// We need the async mutex because we want to do an async read call inside a critical section.
static MESSAGES: Mutex<CriticalSectionRawMutex, Messages> = Mutex::new(Messages::new());
static PRIO_MESSAGE_SIGNAL: Signal<CriticalSectionRawMutex, TextData> = Signal::new();

static FW: &[u8; 230321] = include_bytes!("../cyw43-firmware/43439A0.bin");
static CLM: &[u8; 4752] = include_bytes!("../cyw43-firmware/43439A0_clm.bin");

type WifiPIO = embassy_rp::peripherals::PIO0;
type WifiDMA = embassy_rp::peripherals::DMA_CH0;

// Associate a type of interrupt that the CPU knows about with a handler (i.e. it populates the interrupt vector).
bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => pio::InterruptHandler<WifiPIO>;
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
});

//// ---- Hardware initialization functions. ----------------------------------

mod init {
    use embassy_executor::SendSpawner;
    use embassy_rp::gpio;
    use embedded_hal::delay::DelayNs;

    use super::*;
    use crate::display::DisplayOptions;

    const BOOT_MESSAGE_WAIT_MS: u32 = 1_000;
    const INIT_LOGGING_WAIT_MS: u32 = 2_000;
    const INIT_SPI_WAIT_MS: u32 = 100;
    const DISPLAY_SPI_FREQ: u32 = 10_000_000;

    /// ----- Reset button setup -----
    pub(super) fn reset(spawner: Spawner, r: ResetResources) {
        let button = gpio::Input::new(r.pin, gpio::Pull::Up);
        spawner
            .spawn(system_tasks::resetter(button))
            .expect("Spawning resetter task failed.")
    }

    /// ----- USB logging setup -----
    pub(super) fn usb(spawner: SendSpawner, r: UsbLogResources) {
        let driver = Driver::new(r.usb, Irqs);

        spawner
            .spawn(system_tasks::logger(driver))
            .expect("Spawning logger task failed.");
        // Wait until the USB host picks up our device.
        embassy_time::Delay.delay_ms(INIT_LOGGING_WAIT_MS);
    }

    /// ----- Display setup -----
    pub(super) fn display(r: DisplayResources) -> display::ST7735 {
        log::info!("Display initialization start.");

        // Create SPI
        let mut display_config = spi::Config::default();
        display_config.frequency = DISPLAY_SPI_FREQ;

        // We only have one SPI device so we don't need the SPI bus/SPIDevice machinery.
        let spi: Spi<'_, _, Blocking> = Spi::new_blocking_txonly(r.spi, r.clk, r.mosi, display_config);
        embassy_time::Delay.delay_ms(INIT_SPI_WAIT_MS);
        // Timer::after(INIT_SPI_WAIT_MS).await;

        // dcx: 0 = command, 1 = data
        let dcx = Output::new(r.dcx, Level::Low);
        let rst: Output = Output::new(r.rst, Level::Low);
        let display_cs = Output::new(r.cs, Level::High);
        // Enable LCD backlight
        // a.d. TODO Use PWM to regulate
        let bl = Output::new(r.bl, Level::High);

        // Create display driver which takes care of sending messages to the display.
        let spi_dev = ExclusiveDevice::new(spi, display_cs, Delay).unwrap();
        // let mut display = display::ST7735::new(spi, dcx, rst, display_cs, bl, IMAGE_WIDTH as u8, IMAGE_HEIGHT as u8);
        let mut device =
            st7735_lcd::ST7735::new(spi_dev, dcx, rst, true, false, IMAGE_WIDTH as u32, IMAGE_HEIGHT as u32);
        device.init(&mut Delay).unwrap();
        device
            .set_orientation(&st7735_lcd::Orientation::LandscapeSwapped)
            .expect("Initial display clear failed.");

        let mut display = display::ST7735::new(device, bl);
        display
            .string_formatted("Booting...", DisplayOptions::PriorityMessage)
            .expect("Initial display draw failed.");
        embassy_time::Delay.delay_ms(BOOT_MESSAGE_WAIT_MS);
        display
    }

    /// ----- WIFI setup -----
    pub(super) async fn cyw43(
        spawner: Spawner,
        r: Cyw43Resources,
    ) -> (cyw43::NetDriver<'static>, cyw43::Control<'static>) {
        log::info!("Initialization of cyw43 WIFI chip started.");
        let pwr = Output::new(r.pwr, Level::Low);
        let cs = Output::new(r.cs, Level::High);
        let mut pio = pio::Pio::new(r.pio, Irqs);
        let spi = PioSpi::new(
            &mut pio.common,
            pio.sm0,
            DEFAULT_CLOCK_DIVIDER,
            pio.irq0,
            cs,
            r.dio,
            r.clk,
            r.dma,
        );

        static STATE: StaticCell<cyw43::State> = StaticCell::new();
        let state = STATE.init(cyw43::State::new());
        log::info!("1");
        let (device, mut control, runner) = cyw43::new(state, pwr, spi, FW).await;
        spawner
            .spawn(system_tasks::cyw43(runner))
            .expect("Spawning cyw43_task failed.");

        // The cyw43 runner must have been spawned before doing this!
        control.init(CLM).await;
        // a.d. TODO check which power management mode I want.
        control
            .set_power_management(cyw43::PowerManagementMode::PowerSave)
            .await;

        log::info!("Initialization of cyw43 WIFI chip finished.");
        (device, control)
    }

    /// Setup network stack.
    pub(super) async fn net(spawner: Spawner, net_device: cyw43::NetDriver<'static>) -> net::Stack<'static> {
        log::info!("Initializing network stack.");
        let config = net::Config::dhcpv4(Default::default());
        let seed = 0x0981_a34b_8288_01ff;

        // Init network stack
        static RESOURCES: StaticCell<StackResources<2>> = StaticCell::new();
        let (stack, runner) = net::new(net_device, config, RESOURCES.init(StackResources::new()), seed);

        spawner
            .spawn(system_tasks::net(runner))
            .expect("Spawning net_task failed.");
        stack
    }

    /// Setup WIFI connection.
    pub(super) async fn wifi(control: &mut cyw43::Control<'static>) {
        log::info!("Initializing WIFI connection.");

        let wifi_ssid = match static_data::wifi_ssid() {
            Some(wifi_ssid) => {
                if wifi_ssid.is_empty() {
                    handle_soft_error(SoftError::WifiConfiguration);
                    pending().await
                } else {
                    wifi_ssid
                }
            }
            None => {
                handle_soft_error(SoftError::StaticDataError);
                pending().await
            }
        };

        let wifi_pw = match static_data::wifi_password() {
            Some(wifi_pw) => {
                if wifi_pw.is_empty() {
                    handle_soft_error(SoftError::WifiConfiguration);
                    pending().await
                } else {
                    wifi_pw
                }
            }
            None => {
                handle_soft_error(SoftError::StaticDataError);
                pending().await
            }
        };

        log::info!("Connecting to Wifi '{}'.", wifi_ssid);
        log::info!("With password '{}'", wifi_pw);
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
                    handle_soft_error(SoftError::WifiConnect(e));
                }
            }
        }
    }

    //// ---- System tasks for managing peripherals/debug. -------------------------
    mod system_tasks {
        use embassy_rp::gpio;

        use super::*;

        /// Interacts with the WIFI chip over some internal SPI.
        #[embassy_executor::task]
        pub(super) async fn cyw43(
            runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, WifiPIO, 0, WifiDMA>>,
        ) -> ! {
            log::info!("System task cyw43 starting.");
            runner.run().await
        }

        /// Manages the network stack (so I guess it handles connections, creating sockets and actually sending stuff over sockets).
        #[embassy_executor::task]
        pub(super) async fn net(mut runner: net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
            log::info!("System task net starting.");
            runner.run().await
        }

        /// Sets the global logger and sends log messages over USB.
        #[embassy_executor::task]
        pub(super) async fn logger(driver: Driver<'static, USB>) {
            log::info!("System task logger starting.");
            let level = env!("LOG_LEVEL").parse().unwrap_or(log::LevelFilter::Info);
            embassy_usb_logger::run!(1024, level, driver);
        }

        #[embassy_executor::task]
        pub(super) async fn resetter(mut button: gpio::Input<'static>) -> ! {
            button.wait_for_low().await;
            panic!("Restarting after restart button pressed.");
        }
    }
}

//// ---- Main tasks to implement the device features. ------------------------
mod main_tasks {

    use super::*;
    use crate::display::DisplayOptions;
    use crate::error::handle_hard_error;
    use crate::messagebuf::DisplayMessageData;

    /// This task connects to the configured server and periodically fetches new messages to update the global [`MESSAGES`] object.
    ///
    /// - [`stack`]: The network stack. Used to create sockets.
    /// - [`control`]: The driver of the WIFI chip. TODO usage not clear.
    #[embassy_executor::task]
    pub(super) async fn fetch_data(
        mut state: fetch_data::Token,
        stack: net::Stack<'static>,
        mut control: cyw43::Control<'static>,
    ) {
        // We save the id of the latest message we received to send to the server for the next update check.
        let mut last_message_id = None;

        loop {
            log::info!("Creating new connection.");
            let protocol = fetch_data::Socket::new(&mut state, stack, &mut control).await;
            let mut protocol = match protocol {
                Ok(protocol) => protocol,
                Err(e) => {
                    handle_soft_error(e);
                    Timer::after(SERVER_CONNECT_ERROR_WAIT).await;
                    continue;
                }
            };

            // We loop as long as we successfully receive new message updates. Every other case exits the loop.
            // a.d. TODO move somewhere else
            let update_result = loop {
                log::info!("Checking for updates");
                match protocol.request_update(last_message_id).await {
                    Err(e) => {
                        break Err(e);
                    }
                    Ok(RequestUpdateResult::NoUpdate) => {
                        log::info!("No updates for now. Sleeping.");
                        break Ok(());
                    }
                    Ok(RequestUpdateResult::Update(update)) => match protocol.handle_update(update).await {
                        Ok(()) => {
                            last_message_id = Some(last_message_id.map_or(update.id, |last| cmp::max(last, update.id)));
                        }
                        Err(e) => break Err(e),
                    },
                }
            };

            protocol.close().await;

            if let Err(e) = update_result {
                handle_soft_error(e);
            }
            Timer::after(MESSAGE_FETCH_INTERVAL).await;
        }
    }

    #[embassy_executor::task]
    pub(super) async fn display_prio_messages(display: &'static SharedDisplay) {
        loop {
            let message = PRIO_MESSAGE_SIGNAL.wait().await;
            let mut display = display.lock().await;
            display
                .string_formatted(&message.text, DisplayOptions::PriorityMessage)
                .map_err(|e| handle_hard_error(e))
                .ok();
            drop(display);

            Timer::after(PRIO_MESSAGE_DISPLAY_DURATION).await;
        }
    }

    /// Periodically get the next messages from the global [`MESSAGES`] object and display it.
    ///
    /// - [`display`]: a driver to interact with the display's ST7735 chip.
    #[embassy_executor::task]
    pub(super) async fn display_messages(display: &'static SharedDisplay) {
        let mut last_message_time = Instant::MIN;

        // Each time the loop is entered we display the next non-priority message and then wait for `MESSAGE_DISPLAY_DURATION`
        // Note that if a priority message arrives this will be interrupted (outside of the critical section of locking the display)
        loop {
            log::info!("Acquiring mutex for message buffer and for display.");
            let messages = MESSAGES.lock().await;

            if let Some(next_message) = messages.next_display_message_generic(last_message_time) {
                last_message_time = next_message.meta.updated_at;
                match next_message.data {
                    DisplayMessageData::Text(data) => {
                        log::info!("Showing a text message: {}", data.text.as_str());
                        let mut display = display.lock().await;
                        display
                            .string_formatted(&data.text, DisplayOptions::NormalMessage)
                            .map_err(|e| handle_hard_error(e))
                            .ok();
                    }
                    DisplayMessageData::Image(data) => {
                        log::info!("Showing an image message.");
                        let mut display = display.lock().await;
                        display.draw_image(&data.image).map_err(|e| handle_hard_error(e)).ok();
                    }
                }
            } else {
                let mut display = display.lock().await;
                display
                    .string_formatted("No messages :(", DisplayOptions::NormalMessage)
                    .map_err(|e| handle_hard_error(e))
                    .ok();
            }

            // Must drop this before waiting below so that we do not hold the locks for too long.
            drop(messages);

            Timer::after(MESSAGE_DISPLAY_DURATION).await;
        }
    }
}

static EXECUTOR_HIGH: InterruptExecutor = InterruptExecutor::new();
static EXECUTOR_NORMAL: StaticCell<Executor> = StaticCell::new();

type SharedDisplay = Mutex<CriticalSectionRawMutex, ST7735>;
// TODO Either use StaticCell or just mutex containing option. Which is better?
// With StaticCell we at least don't have the error that the content of the Mutex might be None.
static DISPLAY: StaticCell<SharedDisplay> = StaticCell::new();

#[interrupt]
unsafe fn SWI_IRQ_0() {
    EXECUTOR_HIGH.on_interrupt();
}

assign_resources! {
    usb_log: UsbLogResources {
        usb: USB
    }
    display: DisplayResources {
        bl: PIN_6,
        cs: PIN_7,
        dcx: PIN_8,
        rst: PIN_9,
        spi: SPI1,
        clk: PIN_10,
        mosi: PIN_11,
    }
    reset: ResetResources {
        pin: PIN_1,
    }
    cyw43: Cyw43Resources {
        pwr: PIN_23,
        cs: PIN_25,
        pio: PIO0,
        dio: PIN_24,
        clk: PIN_29,
        dma: DMA_CH0
    }
}

fn init_priority_tasks(
    spawner: SendSpawner,
    r_usb_log: UsbLogResources,
    r_display: DisplayResources,
) -> &'static SharedDisplay {
    init::usb(spawner, r_usb_log);
    log::info!("Booting device with ID: 0x{:08x}", device_id());
    let display = init::display(r_display);
    let display = DISPLAY.init(Mutex::new(display));
    spawner
        .spawn(main_tasks::display_prio_messages(display))
        .expect("Spawning display_prio_messages task failed.");

    display
}

#[embassy_executor::task]
async fn init_normal_tasks(
    spawner: Spawner,
    protocol_token: fetch_data::Token,
    r_reset: ResetResources,
    r_cyw43: Cyw43Resources,
    display: &'static SharedDisplay,
) {
    init::reset(spawner, r_reset);

    spawner
        .spawn(main_tasks::display_messages(display))
        .expect("Spawning display_messages_task failed.");

    let (cyw43_driver, mut cyw43_control) = init::cyw43(spawner, r_cyw43).await;
    let net_stack = init::net(spawner, cyw43_driver).await;

    init::wifi(&mut cyw43_control).await;
    spawner
        .spawn(main_tasks::fetch_data(protocol_token, net_stack, cyw43_control))
        .expect("Spawning fetch_data_task failed.");

    log::info!("Finished configuration.");
}

#[entry]
fn main() -> ! {
    let p = embassy_rp::init(Default::default());
    let r = split_resources!(p);
    let protocol_token = fetch_data::Token::take();

    // spawn high priority tasks
    interrupt::SWI_IRQ_0.set_priority(Priority::P3);
    let interrupt_spawner = EXECUTOR_HIGH.start(interrupt::SWI_IRQ_0);
    let display = init_priority_tasks(interrupt_spawner, r.usb_log, r.display);

    // spawn low priority tasks
    let thread_executor = EXECUTOR_NORMAL.init_with(Executor::new);
    thread_executor.run(|spawner| {
        spawner
            .spawn(init_normal_tasks(spawner, protocol_token, r.reset, r.cyw43, display))
            .expect("Spawning init_system_tasks task failed.")
    });
}
