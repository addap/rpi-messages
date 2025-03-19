use serde::{Deserialize, Serialize};

use crate::types::{DeviceID, UpdateID};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MessageMeta {
    pub receiver_id: DeviceID,
    pub duration: chrono::Duration,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewTextMessage {
    pub meta: MessageMeta,
    pub text: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewImageMessage {
    pub meta: MessageMeta,
    pub image: Vec<u8>,
    pub mime: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewMessageCreated {
    pub id: UpdateID,
}
