//! We save images raw (TODO which endianness) so we use the exact screen dimensions.
//!
use crate::types::TextLength;

pub const TEXT_LINES: usize = 8;
pub const TEXT_COLUMNS: usize = 17;
pub const TEXT_LENGTH: TextLength = TEXT_LINES as u8 * TEXT_COLUMNS as u8;
pub const TEXT_BUFFER_SIZE: usize = TEXT_LENGTH as usize;

pub const IMAGE_WIDTH: usize = 160;
pub const IMAGE_HEIGHT: usize = 128;
pub const IMAGE_BYTES_PER_PIXEL: usize = 2;
pub const IMAGE_BUFFER_SIZE: usize = IMAGE_HEIGHT * IMAGE_WIDTH * IMAGE_BYTES_PER_PIXEL;

pub const WIFI_SSID_LEN: usize = 64;
pub const WIFI_PW_LEN: usize = 64;
