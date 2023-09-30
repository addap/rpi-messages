#![no_std]

use core::fmt::Write;

use embassy_net::tcp::ConnectError;
use heapless::String;
use rpi_messages_common::TEXT_BUFFER_SIZE;

pub mod display;
pub mod messagebuf;
pub mod protocol;

pub type Result<T> = core::result::Result<T, Error>;

pub enum Error {
    /// Things like no wifi connection, can't connect to server, malformed messages from server, so anything non-local
    Soft(SoftError),
    /// Anything that violates invariants about the phyiscal state of the device, e.g. display not found, GPIOs return error
    Hard(HardError),
}

pub enum SoftError {
    WifiConnect(cyw43::ControlError),
    ServerConnect(ConnectError),
}

pub enum HardError {
    SpiError(embassy_rp::spi::Error),
}

impl core::fmt::Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Soft(e) => f.debug_tuple("Soft").field(e).finish(),
            Self::Hard(e) => f.debug_tuple("Hard").field(e).finish(),
        }
    }
}

impl core::fmt::Debug for SoftError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SoftError::WifiConnect(e) => f.debug_tuple("WifiConnect").field(e).finish(),
            SoftError::ServerConnect(e) => f.debug_tuple("ServerConnect").field(e).finish(),
        }
    }
}

impl core::fmt::Debug for HardError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HardError::SpiError(e) => f.debug_tuple("SPI").field(e).finish(),
        }
    }
}

impl SoftError {
    pub fn to_string(&self) -> String<TEXT_BUFFER_SIZE> {
        let mut s = String::new();
        match self {
            SoftError::WifiConnect(e) => write!(&mut s, "Can't connect to Wifi: {:?}", e),
            SoftError::ServerConnect(e) => write!(&mut s, "Can't connect to server: {:?}", e),
        }
        .unwrap();
        s
    }
}
