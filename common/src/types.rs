use core::{fmt, num::ParseIntError, str::FromStr};

#[cfg(feature = "postcard")]
use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};

use crate::consts;

pub type TextLength = u8;
const _: () = assert!(consts::TEXT_BUFFER_SIZE <= TextLength::MAX as usize);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "postcard", derive(MaxSize))]
#[serde(transparent)]
#[repr(transparent)]
pub struct DeviceID(pub u32);
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "postcard", derive(MaxSize))]
#[serde(transparent)]
#[repr(transparent)]

pub struct MessageID(pub u32);

impl FromStr for DeviceID {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = if s.starts_with("0x") { &s[2..] } else { s };
        let id = u32::from_str_radix(s, 16)?;
        Ok(Self(id))
    }
}

impl FromStr for MessageID {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u32::from_str(s).map(|id| MessageID(id))
    }
}

impl fmt::Display for DeviceID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08x}", self)
    }
}

impl fmt::UpperHex for DeviceID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::UpperHex::fmt(&self.0, f)
    }
}

impl fmt::LowerHex for DeviceID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.0, f)
    }
}
