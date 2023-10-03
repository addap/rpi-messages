#![cfg_attr(target_arch = "arm", no_std)]

use core::mem;
use postcard::{self, Result};
use serde::{Deserialize, Serialize};

/// We save images raw (TODO which endian-ness) so we use the exact screen dimensions.
pub const TEXT_LINES: usize = 7;
pub const TEXT_COLUMNS: usize = 17;
pub const TEXT_BUFFER_SIZE: usize = TEXT_COLUMNS * TEXT_LINES;
pub const IMAGE_WIDTH: usize = 160;
pub const IMAGE_HEIGHT: usize = 128;
pub const IMAGE_BYTES_PER_PIXEL: usize = 2;
pub const IMAGE_BUFFER_SIZE: usize = IMAGE_HEIGHT * IMAGE_WIDTH * IMAGE_BYTES_PER_PIXEL;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum MessageUpdateKind {
    Image,
    Text(u32),
}

type DeviceID = u32;
type UpdateID = u32;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct MessageUpdate {
    pub lifetime_sec: u32,
    pub uuid: UpdateID,
    pub kind: MessageUpdateKind,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum UpdateResult {
    NoUpdate,
    Update(MessageUpdate),
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum ClientCommand {
    CheckUpdate(DeviceID),
    RequestUpdate(UpdateID),
}

impl MessageUpdateKind {
    // We want to ensure that text length is bounded by TEXT_BUFFER_SIZE.
    pub fn is_valid(&self) -> bool {
        match self {
            MessageUpdateKind::Image => true,
            MessageUpdateKind::Text(size) => {
                let size = *size as usize;
                size <= TEXT_BUFFER_SIZE
            }
        }
    }

    pub fn size(&self) -> usize {
        match self {
            MessageUpdateKind::Image => IMAGE_BUFFER_SIZE,
            MessageUpdateKind::Text(size) => *size as usize,
        }
    }
}

// a.d. would need unstable feature generic_const_exprs
// trait ProtocolSerializer
// where
//     Self: Sized,
// {
//     const LEN: usize = mem::size_of::<Self>();

//     fn serialize(&self) -> postcard::Result<(usize, [u8; Self::LEN])>;
// }

impl MessageUpdate {
    /// a.d. TODO postcard does not guarantee that serialized(value).len() <= mem::size_of::<T>() for value : T.
    /// So I just double the buffer and hope it works.
    pub const SERIALIZED_LEN: usize = 2 * mem::size_of::<Self>();

    pub fn serialize(&self) -> Result<[u8; Self::SERIALIZED_LEN]> {
        let mut output = [0u8; Self::SERIALIZED_LEN];
        postcard::to_slice(self, &mut output)?;

        Ok(output)
    }

    pub fn deserialize(&bytes: &[u8; Self::SERIALIZED_LEN]) -> Result<Self> {
        postcard::from_bytes(&bytes)
    }
}

impl UpdateResult {
    pub const SERIALIZED_LEN: usize = 2 * mem::size_of::<Self>();

    pub fn serialize(&self) -> Result<[u8; Self::SERIALIZED_LEN]> {
        let mut output = [0u8; Self::SERIALIZED_LEN];
        postcard::to_slice(self, &mut output)?;

        Ok(output)
    }

    pub fn deserialize(&bytes: &[u8; Self::SERIALIZED_LEN]) -> Result<Self> {
        postcard::from_bytes(&bytes)
    }
}

impl ClientCommand {
    pub const SERIALIZED_LEN: usize = 2 * mem::size_of::<Self>();

    pub fn serialize(&self) -> Result<[u8; Self::SERIALIZED_LEN]> {
        let mut output = [0u8; Self::SERIALIZED_LEN];
        postcard::to_slice(self, &mut output)?;

        Ok(output)
    }

    pub fn deserialize(&bytes: &[u8; Self::SERIALIZED_LEN]) -> Result<Self> {
        postcard::from_bytes(&bytes)
    }
}
