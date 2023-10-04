use core::fmt::Write;
use core::str::Utf8Error;

use embassy_net::tcp::ConnectError;
use heapless::String;
use rpi_messages_common::TEXT_BUFFER_SIZE;

use crate::PRIO_MESSAGE_SIGNAL;

pub type Result<T> = core::result::Result<T, Error>;

pub enum Error {
    WifiConnect(cyw43::ControlError),
    ServerConnect(ConnectError),
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
            Error::ServerConnect(_) => write!(&mut s, "Can't connect to server. Please check Wifi connection."),
            Error::Socket => write!(&mut s, "Internal socket error."),
            Error::Serialize(_) => write!(&mut s, "Internal serialization error."),
            Error::ServerMessage(_) => write!(&mut s, "Malformed message from server."),
            Error::MemoryError => write!(&mut s, "Cannot read Wifi data. Please check Wifi settings."),
        };

        if write_result.is_err() {
            s.clear();
            const ERROR_TOO_LONG: &str = "Error::to_string error.";
            const _: () = assert!(ERROR_TOO_LONG.len() <= TEXT_BUFFER_SIZE);
            // a.d. unwrap() cannot panic since the message is shorter than `TEXT_BUFFER_SIZE`.
            s.push_str(ERROR_TOO_LONG).unwrap();
        }

        s
    }
}

pub fn handle_error(e: Error) {
    log::debug!("he: Enter");
    let msg = e.to_string();
    log::warn!("Handling error: {}", msg);
    PRIO_MESSAGE_SIGNAL.signal(msg);
    log::debug!("he: Exit");
}
