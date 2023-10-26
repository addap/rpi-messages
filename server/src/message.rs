//! We have a repository of messages.
//! Different API endpoints add to that repository.
//!  
//!

use anyhow::anyhow;
use image::{imageops::resize, EncodableLayout, ImageBuffer, RgbImage};
use rpi_messages_common::{DeviceID, MessageUpdateKind, UpdateID, TEXT_BUFFER_SIZE};
use rpi_messages_common::{IMAGE_BYTES_PER_PIXEL, IMAGE_HEIGHT, IMAGE_WIDTH};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    io::Write,
    path::Path,
    time::Instant,
};

use crate::{Result, MESSAGE_PATH};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SenderID {
    Web,
    Wechat,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    Image(Vec<u8>),
}

impl MessageContent {
    pub fn new_text(text: String) -> Result<Self> {
        if text.bytes().len() > TEXT_BUFFER_SIZE {
            return Err(anyhow!("Text message too long."));
        }
        Ok(MessageContent::Text(text))
    }
    pub fn new_texts(text: String) -> Result<Vec<Self>> {
        // TODO iterate in a way that we don't split up unicode chars.
        let mut texts = vec![];
        let mut bytes = text.as_bytes();

        while bytes.len() > TEXT_BUFFER_SIZE {
            let text = &bytes[..TEXT_BUFFER_SIZE];
            let s = std::str::from_utf8(text).unwrap().to_owned();
            texts.push(MessageContent::Text(s));

            bytes = &bytes[TEXT_BUFFER_SIZE..]
        }
        Ok(texts)
    }

    pub fn new_image(img: RgbImage) -> Result<Self> {
        let img = resize(
            &img,
            IMAGE_WIDTH as u32,
            IMAGE_HEIGHT as u32,
            image::imageops::FilterType::Gaussian,
        );

        let mut bytes = Vec::with_capacity(IMAGE_HEIGHT * IMAGE_WIDTH * IMAGE_BYTES_PER_PIXEL);
        for px in img.pixels() {
            let [r, g, b] = px.0;

            let [c1, c2] = rgb565::Rgb565::from_srgb888_components(r, g, b).to_rgb565_be();
            bytes.push(c1);
            bytes.push(c2);
        }
        Ok(MessageContent::Image(bytes))
    }
}

impl Into<MessageUpdateKind> for &MessageContent {
    fn into(self) -> MessageUpdateKind {
        match self {
            MessageContent::Text(text) => MessageUpdateKind::Text(text.len() as u32),
            MessageContent::Image(_) => MessageUpdateKind::Image,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: UpdateID,
    pub receiver_id: DeviceID,
    pub sender_id: SenderID,
    pub created_at: chrono::NaiveDateTime,
    pub lifetime_secs: u32,
    pub content: MessageContent,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Messages(Vec<Message>);

impl Message {
    pub fn new(
        id: UpdateID,
        receiver_id: DeviceID,
        sender_id: SenderID,
        created_at: chrono::NaiveDateTime,
        lifetime: chrono::Duration,
        content: MessageContent,
    ) -> Self {
        Self {
            id,
            receiver_id,
            sender_id,
            created_at,
            lifetime_secs: lifetime.num_seconds() as u32,
            content,
        }
    }
}

impl Messages {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    fn load_file<P: AsRef<Path>>(p: &P) -> Result<Self> {
        let file = File::open(p)?;
        let messages = serde_json::from_reader(&file)?;
        Ok(messages)
    }

    pub fn load<P: AsRef<Path>>(p: &P) -> Self {
        Self::load_file(p).unwrap_or(Self::new())
    }

    pub fn store<P: AsRef<Path>>(&self, p: &P) -> Result<()> {
        let file = OpenOptions::new().write(true).create(true).open(p)?;
        serde_json::to_writer(&file, self)?;
        Ok(())
    }

    pub fn add_message(&mut self, message: Message) {
        self.0.push(message);
        self.store(&MESSAGE_PATH).ok();
    }

    pub fn get_next_message(
        &self,
        receiver_id: DeviceID,
        after: Option<UpdateID>,
    ) -> Option<&Message> {
        // first get the timestamp of the given id.
        let after = after
            .and_then(|id| self.get_message(id))
            .map(|msg| msg.created_at);

        self.0
            .iter()
            .filter(|message| {
                message.receiver_id == receiver_id && Some(message.created_at) > after
            })
            .min_by_key(|message| message.created_at)
    }

    pub fn get_message(&self, id: UpdateID) -> Option<&Message> {
        self.0.iter().find(|message| message.id == id)
    }

    pub fn next_id(&self) -> UpdateID {
        self.0.len() as UpdateID
    }
}
