#![cfg_attr(target_arch = "arm", no_std)]

/// We save images raw (TODO which endian-ness) so we use the exact screen dimensions.
pub const TEXT_BUFFER_SIZE: usize = 70;
pub const IMAGE_WIDTH: usize = 160;
pub const IMAGE_HEIGHT: usize = 128;
pub const IMAGE_BYTES_PER_PIXEL: usize = 2;
pub const IMAGE_BUFFER_SIZE: usize = IMAGE_HEIGHT * IMAGE_WIDTH * IMAGE_BYTES_PER_PIXEL;

pub const MESSAGE_UPDATE_KIND_LEN: usize = 5;
pub const MESSAGE_UPDATE_LEN: usize = 13;
pub const UPDATE_RESULT_LEN: usize = 14;
pub const CLIENT_COMMAND_LEN: usize = 5;

#[derive(Clone, Copy)]
pub enum MessageUpdateKind {
    Image,
    Text(u32),
}

type UUID = u32;

pub struct MessageUpdate {
    pub lifetime_sec: u32,
    pub uuid: UUID,
    pub kind: MessageUpdateKind,
}

pub enum UpdateResult {
    NoUpdate,
    Update(MessageUpdate),
}

#[derive(Clone, Copy)]
pub enum ClientCommand {
    CheckUpdate,
    RequestUpdate(UUID),
}

impl MessageUpdateKind {
    // We want to ensure that text length is bounded by TEXT_BUFFER_SIZE.
    pub fn validate(&self) -> Option<()> {
        match self {
            MessageUpdateKind::Image => Some(()),
            MessageUpdateKind::Text(size) => {
                let size = *size as usize;
                if size <= TEXT_BUFFER_SIZE {
                    Some(())
                } else {
                    None
                }
            }
        }
    }

    pub fn size(&self) -> usize {
        match self {
            MessageUpdateKind::Image => IMAGE_BUFFER_SIZE,
            MessageUpdateKind::Text(size) => *size as usize,
        }
    }

    pub fn serialize(&self) -> Option<[u8; MESSAGE_UPDATE_KIND_LEN]> {
        self.validate()?;

        Some(match self {
            MessageUpdateKind::Image => [0u8; MESSAGE_UPDATE_KIND_LEN],
            MessageUpdateKind::Text(size) => {
                let mut output = [0u8; MESSAGE_UPDATE_KIND_LEN];
                output[0] = 1;
                output[1..5].copy_from_slice(&size.to_be_bytes());
                output
            }
        })
    }

    pub fn deserialize(bytes: &[u8; MESSAGE_UPDATE_KIND_LEN]) -> Option<Self> {
        match bytes[0] {
            0 => Some(Self::Image),
            1 => {
                let result = Self::Text(u32::from_be_bytes(bytes[1..].try_into().unwrap()));
                result.validate()?;
                Some(result)
            }

            _ => None,
        }
    }
}

impl MessageUpdate {
    pub fn serialize(&self) -> Option<[u8; MESSAGE_UPDATE_LEN]> {
        let mut output = [0u8; MESSAGE_UPDATE_LEN];
        output[0..4].copy_from_slice(&self.lifetime_sec.to_be_bytes());
        output[4..8].copy_from_slice(&self.uuid.to_be_bytes());
        output[8..13].copy_from_slice(&self.kind.serialize()?);
        Some(output)
    }

    pub fn deserialize(&bytes: &[u8; MESSAGE_UPDATE_LEN]) -> Option<Self> {
        let lifetime_sec = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
        let uuid = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
        let kind = MessageUpdateKind::deserialize(&bytes[8..13].try_into().unwrap())?;

        Some(Self {
            lifetime_sec,
            uuid,
            kind,
        })
    }
}

impl UpdateResult {
    pub fn serialize(&self) -> Option<[u8; UPDATE_RESULT_LEN]> {
        match self {
            UpdateResult::NoUpdate => Some([0u8; UPDATE_RESULT_LEN]),
            UpdateResult::Update(update) => {
                let mut output = [0u8; UPDATE_RESULT_LEN];
                output[0] = 1;
                output[1..14].copy_from_slice(&update.serialize()?);
                Some(output)
            }
        }
    }

    pub fn deserialize(bytes: &[u8; UPDATE_RESULT_LEN]) -> Option<Self> {
        match bytes[0] {
            0 => Some(Self::NoUpdate),
            1 => {
                let update = MessageUpdate::deserialize(&bytes[1..14].try_into().unwrap())?;
                Some(Self::Update(update))
            }
            _ => None,
        }
    }
}

impl ClientCommand {
    pub fn serialize(&self) -> [u8; CLIENT_COMMAND_LEN] {
        match self {
            ClientCommand::CheckUpdate => [0u8; CLIENT_COMMAND_LEN],
            ClientCommand::RequestUpdate(uuid) => {
                let mut output = [0u8; CLIENT_COMMAND_LEN];
                output[0] = 1;
                output[1..5].copy_from_slice(&uuid.to_be_bytes());
                output
            }
        }
    }

    pub fn deserialize(&bytes: &[u8; CLIENT_COMMAND_LEN]) -> Option<Self> {
        match bytes[0] {
            0 => Some(Self::CheckUpdate),
            1 => Some(Self::RequestUpdate(u32::from_be_bytes(
                bytes[1..].try_into().unwrap(),
            ))),
            _ => None,
        }
    }
}
