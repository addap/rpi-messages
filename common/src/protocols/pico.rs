use core::fmt;

use postcard;
use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};

use crate::{
    consts::{self, TEXT_BUFFER_SIZE},
    types::{DeviceID, MessageID, TextLength},
};

#[derive(Debug)]
pub enum Error {
    Length { val: usize, max: usize },
    Postcard(postcard::Error),

    Socket,
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

impl From<postcard::Error> for Error {
    fn from(value: postcard::Error) -> Self {
        Self::Postcard(value)
    }
}

impl Error {
    pub fn fmt<W: fmt::Write>(&self, f: &mut W) -> fmt::Result {
        match self {
            Error::Length { val, max } => write!(f, "Length is {val} but max is {max}."),
            Error::Postcard(error) => write!(f, "Serialization error: {}", error),
            Error::Socket => write!(f, "Socket error"),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Error::fmt(self, f)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, MaxSize)]
pub enum UpdateKind {
    Image,
    Text(TextLength),
}

impl UpdateKind {
    pub fn size(&self) -> usize {
        match *self {
            UpdateKind::Image => consts::IMAGE_BUFFER_SIZE,
            UpdateKind::Text(len) => len as usize,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, MaxSize)]
pub struct Update {
    pub lifetime_sec: u32,
    pub id: MessageID,
    pub kind: UpdateKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, MaxSize)]
pub enum RequestUpdateResult {
    NoUpdate,
    Update(Update),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, MaxSize)]
pub enum ClientCommand {
    RequestUpdate(DeviceID, Option<MessageID>),
}

impl RequestUpdateResult {
    pub fn check_valid(&self) -> Result<(), Error> {
        match self {
            RequestUpdateResult::NoUpdate => Ok(()),
            RequestUpdateResult::Update(message_update) => match message_update.kind {
                UpdateKind::Image => Ok(()),
                UpdateKind::Text(size) => {
                    let size = size as usize;
                    if size > TEXT_BUFFER_SIZE {
                        Err(Error::Length {
                            val: size,
                            max: TEXT_BUFFER_SIZE,
                        })
                    } else {
                        Ok(())
                    }
                }
            },
        }
    }
}

pub mod serialization {
    use serde::de::DeserializeOwned;

    use super::*;

    type Length = u16;

    #[allow(async_fn_in_trait)]
    trait AbstractSocket {
        async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), Error>;
        async fn write_all(&mut self, buf: &[u8]) -> Result<(), Error>;
    }

    #[cfg(all(feature = "embedded-io-async", feature = "embassy-net"))]
    impl AbstractSocket for embassy_net::tcp::TcpSocket<'_> {
        async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), Error> {
            embedded_io_async::Read::read_exact(self, buf)
                .await
                .map_err(|_| Error::Socket)
        }

        async fn write_all(&mut self, buf: &[u8]) -> Result<(), Error> {
            embedded_io_async::Write::write_all(self, buf)
                .await
                .map_err(|_| Error::Socket)
        }
    }

    #[cfg(feature = "std")]
    impl AbstractSocket for std::net::TcpStream {
        async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), Error> {
            std::io::Read::read_exact(self, buf).map_err(|_| Error::Socket)?;
            Ok(())
        }

        async fn write_all(&mut self, buf: &[u8]) -> Result<(), Error> {
            std::io::Write::write_all(self, buf).map_err(|_| Error::Socket)
        }
    }

    #[cfg(feature = "tokio")]
    impl AbstractSocket for tokio::net::TcpStream {
        async fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), Error> {
            tokio::io::AsyncReadExt::read_exact(self, buf)
                .await
                .map_err(|_| Error::Socket)?;
            Ok(())
        }

        async fn write_all(&mut self, buf: &[u8]) -> Result<(), Error> {
            tokio::io::AsyncWriteExt::write_all(self, buf)
                .await
                .map_err(|_| Error::Socket)
        }
    }

    /// Serialize values with a length prefix.
    /// +-------------+----------------+
    /// | Length: u16 | data: u8       |
    /// +-------------+----------------+
    trait SerDe: Serialize + DeserializeOwned + MaxSize {
        const DATA_START: usize = size_of::<Length>();
        const SERIALIZED_SIZE: usize = Self::DATA_START + Self::POSTCARD_MAX_SIZE;
        // Statically check that the POSTCARD_MAX_SIZE constant can be encoded in the length field of our messages.
        const _ASSERT_LENGTH_REPRESENTABLE: () = assert!(Self::POSTCARD_MAX_SIZE <= Length::MAX as usize);

        fn to_bytes<'a, 'b>(&'a self, buf: &'b mut [u8]) -> Result<&'b mut [u8], Error> {
            // We cannot use Self in the const generic of the slice type, so we check the length requirement here at runtime.
            // TODO There is an unstable option for complex generic const expressions but I'd wait until it's stabilized https://github.com/rust-lang/rust/issues/76560
            debug_assert!(buf.len() == Self::SERIALIZED_SIZE);

            let result = postcard::to_slice(self, &mut buf[Self::DATA_START..])?;
            let data_len = result.len();
            let total_len = Self::DATA_START + data_len;
            let length_bytes = (data_len as Length).to_ne_bytes();
            buf[..Self::DATA_START].copy_from_slice(&length_bytes);
            Ok(&mut buf[..total_len])
        }

        fn from_bytes(buf: &[u8]) -> Result<Self, Error> {
            let result = postcard::from_bytes(buf)?;
            Ok(result)
        }
    }

    impl SerDe for ClientCommand {}
    impl SerDe for RequestUpdateResult {}

    #[allow(async_fn_in_trait, private_bounds)]
    pub trait Transmission: SerDe {
        const BUFFER_SIZE: usize = <Self as SerDe>::SERIALIZED_SIZE;

        #[cfg(feature = "std")]
        async fn send_alloc<S: AbstractSocket>(&self, socket: &mut S) -> Result<(), Error> {
            let mut buf = vec![0u8; Self::SERIALIZED_SIZE];

            self.send(&mut buf, socket).await
        }

        async fn send<S: AbstractSocket>(&self, buf: &mut [u8], socket: &mut S) -> Result<(), Error> {
            assert!(buf.len() == Self::BUFFER_SIZE);

            let serialized_buf = self.to_bytes(buf)?;
            socket.write_all(&serialized_buf).await
        }

        #[cfg(feature = "std")]
        async fn receive_alloc<S: AbstractSocket>(socket: &mut S) -> Result<Self, Error> {
            let mut buf = vec![0u8; Self::SERIALIZED_SIZE];

            Self::receive(&mut buf, socket).await
        }

        async fn receive<S: AbstractSocket>(buf: &mut [u8], socket: &mut S) -> Result<Self, Error> {
            assert!(buf.len() == Self::BUFFER_SIZE);

            socket.read_exact(&mut buf[..Self::DATA_START]).await?;
            let data_len = Length::from_ne_bytes([buf[0], buf[1]]) as usize;
            if Self::DATA_START + data_len > Self::BUFFER_SIZE {
                return Err(Error::Length {
                    val: data_len,
                    max: Self::POSTCARD_MAX_SIZE,
                });
            }
            let data_buf = &mut buf[Self::DATA_START..(Self::DATA_START + data_len)];
            socket.read_exact(data_buf).await?;
            Self::from_bytes(&data_buf)
        }
    }

    impl Transmission for ClientCommand {}
    impl Transmission for RequestUpdateResult {}
}
