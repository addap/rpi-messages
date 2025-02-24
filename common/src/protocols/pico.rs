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
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Length(length, max) => write!(f, "Length is {length} but max is {max}."),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, MaxSize)]
pub enum UpdateKind {
    Image,
    Text(u32),
}

#[derive(Clone, Copy, Serialize, Deserialize, MaxSize)]
pub struct Update {
    pub lifetime_sec: u32,
    pub id: UpdateID,
    pub kind: UpdateKind,
}

#[derive(Clone, Copy, Serialize, Deserialize, MaxSize)]
pub enum CheckUpdateResult {
    NoUpdate,
    Update(Update),
}

#[derive(Clone, Copy, Serialize, Deserialize, MaxSize)]
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
