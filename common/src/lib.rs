#![cfg_attr(target_arch = "arm", no_std)]

use serde::{Deserialize, Serialize};

/// We save images raw (TODO which endian-ness) so we use the exact screen dimensions.
pub const IMAGE_WIDTH: usize = 160;
pub const IMAGE_HEIGHT: usize = 128;
pub const IMAGE_BYTES_PER_PIXEL: usize = 2;
pub const IMAGE_BUFFER_SIZE: usize = IMAGE_HEIGHT * IMAGE_WIDTH * IMAGE_BYTES_PER_PIXEL;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum MessageUpdateKind {
    Text(usize),
    Image,
}

impl MessageUpdateKind {
    pub fn size(self) -> usize {
        match self {
            MessageUpdateKind::Text(size) => size,
            MessageUpdateKind::Image => IMAGE_BUFFER_SIZE,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct MessageUpdate {
    pub lifetime_sec: u64,
    pub kind: MessageUpdateKind,
    pub uuid: u64,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum ClientCommand {
    CheckUpdate,
    RequestUpdate(u64),
}
