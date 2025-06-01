use core::fmt::{self, Debug};
use core::str::Utf8Error;

use common::consts::TEXT_BUFFER_SIZE;
use derive_more::From;
use embassy_net::tcp::ConnectError;
use heapless::String;

use crate::messagebuf::TextData;
use crate::PRIO_MESSAGE_SIGNAL;

pub type Result<T> = core::result::Result<T, Error>;

#[allow(unused)]
#[derive(Debug, From)]
pub enum ServerMessageError {
    Encoding(Utf8Error),
    Protocol(common::protocols::pico::Error),
}

#[allow(unused)]
#[derive(Debug, From)]
pub enum Error {
    WifiConnect(cyw43::ControlError),
    WifiConfiguration,
    ServerConnect(ConnectError),
    Socket,
    ServerMessage(ServerMessageError),
    StaticDataError,
}

impl From<common::protocols::pico::Error> for Error {
    fn from(value: common::protocols::pico::Error) -> Self {
        Self::ServerMessage(ServerMessageError::Protocol(value))
    }
}

impl ServerMessageError {
    fn fmt<W: fmt::Write>(&self, f: &mut W) -> fmt::Result {
        match self {
            Self::Encoding(_) => write!(f, "UTF-8 encoding error."),
            Self::Protocol(e) => e.fmt(f),
        }
    }
}

impl Error {
    fn fmt<W: fmt::Write>(&self, f: &mut W) -> fmt::Result {
        match self {
            Error::WifiConnect(_) => write!(f, "Cannot connect to Wifi. Please check Wifi settings."),
            Error::ServerConnect(_) => write!(f, "Can't connect to server. Please check Wifi connection."),
            Error::Socket => write!(f, "Internal socket error."),
            Error::ServerMessage(e) => e.fmt(f),
            Error::StaticDataError => write!(
                f,
                "Cannot read static data from flash memory. Please re-flash static data uf2."
            ),
            Error::WifiConfiguration => write!(f, "Wifi settings are not configured yet. Please flash uf2."),
        }
    }

    pub fn to_display_string(&self) -> TextData {
        let mut text = String::new();
        let write_result = self.fmt(&mut text);

        if write_result.is_err() {
            text.clear();
            const ERROR_TOO_LONG: &str = "Error::to_display_string error too long.";
            const _: () = assert!(ERROR_TOO_LONG.len() <= TEXT_BUFFER_SIZE);
            // a.d. unwrap() cannot panic since the message is shorter than `TEXT_BUFFER_SIZE`.
            text.push_str(ERROR_TOO_LONG).unwrap();
        }

        TextData { text }
    }
}

pub fn handle_error(e: Error) {
    let msg = e.to_display_string();
    log::error!("Handling error: {}", msg.text);
    PRIO_MESSAGE_SIGNAL.signal(msg);
}
