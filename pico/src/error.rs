use core::fmt::{self, Write};
use core::str::Utf8Error;

use common::{consts::TEXT_BUFFER_SIZE, postcard};
use derive_more::From;
use embassy_net::tcp::ConnectError;
use heapless::String;

use crate::messagebuf::TextData;
use crate::PRIO_MESSAGE_SIGNAL;

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, From)]
pub enum ServerMessageError {
    Encoding(Utf8Error),
    Format(common::protocol::Error),
}

#[derive(Debug, From)]
pub enum Error {
    WifiConnect(cyw43::ControlError),
    WifiConfiguration,
    ServerConnect(ConnectError),
    Socket,
    Postcard(postcard::Error),
    ServerMessage(ServerMessageError),
    MemoryError,
}

impl fmt::Display for ServerMessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ServerMessageError::Encoding(e) => write!(f, "{}", e),
            ServerMessageError::Format(e) => write!(f, "{}", e),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::WifiConnect(error) => todo!(),
            Error::WifiConfiguration => todo!(),
            Error::ServerConnect(connect_error) => todo!(),
            Error::Socket => todo!(),
            Error::Postcard(error) => todo!(),
            Error::ServerMessage(server_message_error) => todo!(),
            Error::MemoryError => todo!(),
        }
    }
}

impl Error {
    pub fn to_display_string(&self) -> TextData {
        let mut text = String::new();
        let write_result = match self {
            Error::WifiConnect(_) => write!(&mut text, "Cannot connect to Wifi. Please check Wifi settings."),
            Error::ServerConnect(_) => write!(&mut text, "Can't connect to server. Please check Wifi connection."),
            Error::Socket => write!(&mut text, "Internal socket error."),
            Error::Postcard(_) => write!(&mut text, "Internal serialization error."),
            Error::ServerMessage(e) => write!(&mut text, "Malformed message from server."),
            Error::MemoryError => write!(&mut text, "Cannot read Wifi data. Please check Wifi settings."),
            Error::WifiConfiguration => write!(&mut text, "Wifi settings are not configured yet. Please flash uf2."),
        };

        if write_result.is_err() {
            text.clear();
            const ERROR_TOO_LONG: &str = "Error::to_string error.";
            const _: () = assert!(ERROR_TOO_LONG.len() <= TEXT_BUFFER_SIZE);
            // a.d. unwrap() cannot panic since the message is shorter than `TEXT_BUFFER_SIZE`.
            text.push_str(ERROR_TOO_LONG).unwrap();
        }

        TextData { text }
    }
}

pub fn handle_error(e: Error) {
    // a.d. TODO change abbreviations to actual function name.
    log::debug!("he: Enter");
    let msg = e.to_display_string();
    log::warn!("Handling error: {}", msg.text);
    PRIO_MESSAGE_SIGNAL.signal(msg);
    log::debug!("he: Exit");
}
