use core::fmt;

use postcard;
use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};

use crate::{
    consts::{IMAGE_BUFFER_SIZE, TEXT_BUFFER_SIZE},
    types::{DeviceID, UpdateID},
};

#[derive(Debug)]
pub enum Error {
    Length(usize, usize),
    Postcard(postcard::Error),
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
            Error::Length(length, max) => write!(f, "Length is {length} but max is {max}."),
            Error::Postcard(error) => write!(f, "Serialization error: {}", error),
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
    Text(u32),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, MaxSize)]
pub struct Update {
    pub lifetime_sec: u32,
    pub id: UpdateID,
    pub kind: UpdateKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, MaxSize)]
pub enum CheckUpdateResult {
    NoUpdate,
    Update(Update),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, MaxSize)]
pub enum ClientCommand {
    CheckUpdate(DeviceID, Option<UpdateID>),
    RequestUpdate(UpdateID),
}

impl UpdateKind {
    pub fn size(&self) -> usize {
        match self {
            UpdateKind::Image => IMAGE_BUFFER_SIZE,
            UpdateKind::Text(size) => *size as usize,
        }
    }
}

impl CheckUpdateResult {
    pub fn check_valid(&self) -> Result<(), Error> {
        match self {
            CheckUpdateResult::NoUpdate => Ok(()),
            CheckUpdateResult::Update(message_update) => match message_update.kind {
                UpdateKind::Image => Ok(()),
                UpdateKind::Text(size) => {
                    let size = size as usize;
                    if size > TEXT_BUFFER_SIZE {
                        Err(Error::Length(size, TEXT_BUFFER_SIZE))
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

    pub trait SerDe: Serialize + DeserializeOwned + MaxSize {
        const BUFFER_SIZE: usize = Self::POSTCARD_MAX_SIZE;

        // a.d. TODO remove maybe
        #[cfg(feature = "std")]
        fn to_bytes_alloc(&self) -> Result<Vec<u8>, Error> {
            let mut buf = vec![0; Self::BUFFER_SIZE];
            self.to_bytes(buf.as_mut_slice())?;
            Ok(buf)
        }

        fn to_bytes<'a, 'b>(&'a self, buf: &'b mut [u8]) -> Result<&'b mut [u8], Error> {
            // We cannot use Self in the const generic of the slice type, so we check the length requirement here at runtime.
            // TODO There is an unstable option for complex generic const expressions but I'd wait until it's stabilized https://github.com/rust-lang/rust/issues/76560
            assert!(buf.len() == Self::BUFFER_SIZE);
            postcard::to_slice(self, buf)?;
            Ok(buf)
        }

        fn from_bytes(buf: &[u8]) -> Result<Self, Error> {
            assert!(buf.len() == Self::BUFFER_SIZE);
            let result = postcard::from_bytes(buf)?;
            Ok(result)
        }
    }
}

impl serialization::SerDe for ClientCommand {}
impl serialization::SerDe for CheckUpdateResult {}
