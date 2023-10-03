use core::fmt::Write;
use core::str::Utf8Error;

use embassy_net::tcp::ConnectError;
use heapless::String;
use rpi_messages_common::TEXT_BUFFER_SIZE;

pub type Result<T> = core::result::Result<T, Error>;

pub enum Error {
    WifiConnect(cyw43::ControlError),
    ServerConnect(ConnectError),
    SpiError(embassy_rp::spi::Error),
    Socket,
    Serialize(postcard::Error),
    ServerMessage(Utf8Error),
    MemoryError,
}

impl core::fmt::Debug for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::WifiConnect(e) => f.debug_tuple("WifiConnect").field(e).finish(),
            Error::ServerConnect(e) => f.debug_tuple("ServerConnect").field(e).finish(),
            Error::SpiError(e) => f.debug_tuple("SPI").field(e).finish(),
            Error::Socket => f.write_str("Socket"),
            Error::Serialize(e) => f.debug_tuple("Serialize").field(e).finish(),
            Error::ServerMessage(e) => f.debug_tuple("ServerMessage").field(e).finish(),
            Error::MemoryError => f.write_str("MemoryError"),
        }
    }
}

impl Error {
    pub fn to_string(&self) -> String<TEXT_BUFFER_SIZE> {
        let mut s = String::new();
        let write_result = match self {
            Error::WifiConnect(_) => write!(&mut s, "Cannot connect to Wifi. Please check Wifi settings."),
            Error::ServerConnect(e) => write!(&mut s, "Can't connect to server: {:?}", e),
            Error::SpiError(_) => write!(&mut s, "Internal SPI error."),
            Error::Socket | Error::Socket => write!(&mut s, "Internal socket error."),
            Error::Serialize(_) => write!(&mut s, "Internal serialization error."),
            Error::ServerMessage(_) => write!(&mut s, "Malformed message from server."),
            Error::MemoryError => write!(&mut s, "Cannot read Wifi data. Please check Wifi settings."),
        };

        if write_result.is_err() {
            s.clear();
            s.push_str("Error::to_string error.");
        }

        s
    }
}
