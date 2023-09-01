//! This example shows how to use SPI (Serial Peripheral Interface) in the RP2040 chip.
//!
//! Example written for a display using the ST7789 chip. Possibly the Waveshare Pico-ResTouch
//! (https://www.waveshare.com/wiki/Pico-ResTouch-LCD-2.8)

#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use core::cell::RefCell;
use core::str::from_utf8;

use cortex_m::asm::wfe;
use cyw43::Control;
use cyw43_pio::PioSpi;
use embassy_embedded_hal::shared_bus::blocking::spi::{SpiDevice, SpiDeviceWithConfig};
use embassy_executor::Spawner;
use embassy_futures::yield_now;
use embassy_net::tcp::TcpSocket;
use embassy_net::{Config, Stack, StackResources};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIN_23, PIN_25, PIO0, USB};
use embassy_rp::spi::{Blocking, Spi};
use embassy_rp::usb::Driver;
use embassy_rp::{bind_interrupts, pio, spi, usb, Peripherals};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
// use embassy_sync::mutex::Mutex;
use embassy_sync::blocking_mutex::{CriticalSectionMutex, Mutex};
use embassy_time::{Delay, Duration, Timer};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::image::{Image, ImageRawLE};
use embedded_graphics::mono_font::ascii::FONT_10X20;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::Text;
use heapless::String;
use st7735_lcd::{Orientation, ST7735};
use static_cell::make_static;
use {defmt_rtt as _, panic_probe as _};

const DISPLAY_FREQ: u32 = 10_000_000;
const WIFI_NETWORK: &str = "Buffalo-G-1337";
const WIFI_PASSWORD: &str = "hahagetfucked";

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => pio::InterruptHandler<PIO0>;
    USBCTRL_IRQ => usb::InterruptHandler<USB>;
});

#[embassy_executor::task]
async fn wifi_task(
    runner: cyw43::Runner<'static, Output<'static, PIN_23>, PioSpi<'static, PIN_25, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<cyw43::NetDriver<'static>>) -> ! {
    stack.run().await
}

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USB>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

fn min(a: usize, b: usize) -> usize {
    if a < b {
        a
    } else {
        b
    }
}

#[embassy_executor::task]
async fn get_data_task(
    stack: &'static Stack<cyw43::NetDriver<'static>>,
    control: &'static mut Control<'static>,
    text: &'static Mutex<NoopRawMutex, RefCell<String<32>>>,
) {
    // And now we can use it!

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    let mut buf = [0; 4096];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(10)));

        control.gpio_set(1, false).await;
        log::info!("Listening on TCP:1234...");
        if let Err(e) = socket.accept(1234).await {
            log::warn!("accept error: {:?}", e);
            continue;
        }

        log::info!("Received connection from {:?}", socket.remote_endpoint());
        control.gpio_set(0, true).await;

        loop {
            let n = match socket.read(&mut buf).await {
                Ok(0) => {
                    log::warn!("read EOF");
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    log::warn!("read error: {:?}", e);
                    break;
                }
            };

            log::info!("rxd {}", from_utf8(&buf[..n]).unwrap());
            yield_now().await;

            text.lock(|rc| {
                let mut x = rc.borrow_mut();
                x.clear();
                for &c in buf[..n].iter().take(32) {
                    x.push(c as char).unwrap();
                }
            })
        }
    }
}

#[embassy_executor::task]
async fn display_messages_task(
    display: &'static mut ST7735<
        Spi<'_, embassy_rp::peripherals::SPI1, Blocking>,
        Output<'_, embassy_rp::peripherals::PIN_8>,
        Output<'_, embassy_rp::peripherals::PIN_12>,
    >,
    text: &'static Mutex<NoopRawMutex, RefCell<String<32>>>,
) {
    let style = MonoTextStyle::new(&FONT_10X20, Rgb565::GREEN);

    loop {
        text.lock(|rc| {
            let x = rc.borrow();
            Text::new(x.as_str(), Point::new(20, 100), style).draw(display).unwrap();
        });
        Timer::after(Duration::from_secs(1)).await;
        display.clear(Rgb565::RED).unwrap();
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // USB logging
    let driver = Driver::new(p.USB, Irqs);
    spawner.spawn(logger_task(driver)).unwrap();
    log::info!("Hello World!");

    let text: &Mutex<NoopRawMutex, _> = make_static!(Mutex::new(RefCell::new(String::new())));

    //////////////////  WIFI
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
        //let config = embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
        //    address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 69, 2), 24),
        //    dns_servers: Vec::new(),
        //    gateway: Some(Ipv4Address::new(192, 168, 69, 1)),
        //});

        // Generate random seed
        let seed = 0x0123_4567_89ab_cdef; // chosen by fair dice roll. guarenteed to be random.

        // Init network stack
        let stack = &*make_static!(Stack::new(
            net_device,
            config,
            make_static!(StackResources::<2>::new()),
            seed
        ));

        spawner.spawn(net_task(stack)).unwrap();

        loop {
            //control.join_open(WIFI_NETWORK).await;
            match control.join_wpa2(WIFI_NETWORK, WIFI_PASSWORD).await {
                Ok(_) => break,
                Err(err) => {
                    log::info!("join failed with status={}", err.status);
                }
            }
        }

        let scontrol = make_static!(control);
        spawner.spawn(get_data_task(stack, scontrol, text)).unwrap();
    }
    ///////////////////////// WIFI

    let bl = p.PIN_13;
    let rst = p.PIN_12;
    let display_cs = p.PIN_9;
    let dcx = p.PIN_8;
    let mosi = p.PIN_11;
    let clk = p.PIN_10;

    // let pinv = [dcx, mosi];

    // create SPI
    let mut display_config = spi::Config::default();
    display_config.frequency = DISPLAY_FREQ;

    let spi: Spi<'_, _, Blocking> = Spi::new_blocking_txonly(p.SPI1, clk, mosi, display_config.clone());
    // let spi_bus: Mutex<NoopRawMutex, _> = Mutex::new(RefCell::new(spi));

    // let display_spi = SpiDevice::new(&spi_bus, Output::new(display_cs, Level::High));
    // let touch_spi = SpiDeviceWithConfig::new(&spi_bus, Output::new(touch_cs, Level::High), touch_config);

    // let mut touch = Touch::new(touch_spi);

    // dcx: 0 = command, 1 = data
    let dcx = Output::new(dcx, Level::Low);
    let rst = Output::new(rst, Level::Low);
    // should always be low
    let display_cs = Output::new(display_cs, Level::Low);

    // Enable LCD backlight
    // Use PWN to regulate
    let _bl = Output::new(bl, Level::High);

    // display interface abstraction from SPI and DC
    // let di = SPIDeviceInterface::new(display_spi, dcx);

    // create driver
    let mut display = ST7735::new(spi, dcx, rst, true, false, 160, 128);

    // initialize
    display.init(&mut Delay).unwrap();

    // set default orientation
    display.set_orientation(&Orientation::Landscape).unwrap();
    display.set_offset(1, 2);

    display.clear(Rgb565::RED).unwrap();

    spawner
        .spawn(display_messages_task(make_static!(display), text))
        .unwrap();
}

async fn init_wifi(spawner: &Spawner, p: Peripherals) {}
