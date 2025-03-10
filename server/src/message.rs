//! We have a repository of messages.
//! Different API endpoints add to that repository.
//!  
//!

use std::{fs::File, io::Cursor, path::Path};

use anyhow::anyhow;
use common::{
    consts::{IMAGE_BYTES_PER_PIXEL, IMAGE_HEIGHT, IMAGE_WIDTH, TEXT_BUFFER_SIZE},
    protocols::{pico::UpdateKind, web::MessageMeta},
    types::{DeviceID, UpdateID},
};
use image::{codecs::png::PngEncoder, ImageReader};
use serde::{Deserialize, Serialize};

use crate::{Result, MESSAGE_PATH};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SenderID {
    Web,
    Telegram,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    Image { png: Vec<u8>, rgb565: Vec<u8> },
}

impl MessageContent {
    pub fn new_text(text: String) -> Result<Self> {
        if text.bytes().len() <= TEXT_BUFFER_SIZE {
            Ok(MessageContent::Text(text))
        } else {
            Err(anyhow!("Text message too long."))
        }
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

    pub fn new_image(img_data: Vec<u8>) -> Result<Self> {
        let img_reader = ImageReader::new(Cursor::new(img_data))
            .with_guessed_format()
            .expect("Cursor IO never fails");
        let img_orig = img_reader.decode()?;
        let img_resized = image::imageops::resize(
            &img_orig,
            IMAGE_WIDTH as u32,
            IMAGE_HEIGHT as u32,
            image::imageops::FilterType::Gaussian,
        );

        // FIXME can we determine the encoded size to use with_capacity?
        let mut png = Vec::new();
        let png_encoder = PngEncoder::new(&mut png);
        img_resized.write_with_encoder(png_encoder)?;

        let mut rgb565 = Vec::with_capacity(IMAGE_HEIGHT * IMAGE_WIDTH * IMAGE_BYTES_PER_PIXEL);
        for px in img_resized.pixels() {
            let [r, g, b, _] = px.0;

            let [c1, c2] = rgb565::Rgb565::from_srgb888_components(r, g, b).to_rgb565_be();
            rgb565.push(c1);
            rgb565.push(c2);
        }
        Ok(MessageContent::Image { png, rgb565 })
    }
}

impl From<&MessageContent> for UpdateKind {
    fn from(value: &MessageContent) -> UpdateKind {
        match value {
            MessageContent::Text(text) => UpdateKind::Text(text.len() as u32),
            MessageContent::Image { .. } => UpdateKind::Image,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: UpdateID,
    pub meta: MessageMeta,
    pub sender_id: SenderID,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub content: MessageContent,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Messages {
    inner: Vec<Message>,
}

impl Message {
    pub fn new(
        id: UpdateID,
        meta: MessageMeta,
        sender_id: SenderID,
        created_at: chrono::DateTime<chrono::Utc>,
        content: MessageContent,
    ) -> Self {
        Self {
            id,
            meta,
            sender_id,
            created_at,
            content,
        }
    }
}

impl Messages {
    pub fn dummy() -> Self {
        let meta = MessageMeta {
            receiver_id: 0xcafebabe,
            duration: chrono::Duration::hours(24),
        };

        Self {
            inner: vec![
                Message::new(
                    0,
                    meta,
                    SenderID::Web,
                    chrono::Utc::now(),
                    MessageContent::Text("Dummy text".to_string()),
                ),
                Message::new(
                    1,
                    meta,
                    SenderID::Web,
                    chrono::Utc::now(),
                    MessageContent::Image {
                        png: include_bytes!("../pictures/love.png").to_vec(),
                        rgb565: include_bytes!("../pictures/love.bin").to_vec(),
                    },
                ),
                Message::new(
                    2,
                    meta,
                    SenderID::Web,
                    chrono::Utc::now(),
                    MessageContent::Text("Another dummy text".to_string()),
                ),
            ],
        }
    }

    pub const fn new() -> Self {
        Self { inner: Vec::new() }
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
        let file = File::create(p)?;
        serde_json::to_writer(&file, self)?;
        Ok(())
    }

    pub fn add_message(&mut self, message: Message) {
        self.inner.push(message);
        self.store(&MESSAGE_PATH).ok();
    }

    pub fn get_next_message(&self, receiver_id: DeviceID, after: Option<UpdateID>) -> Option<&Message> {
        // first get the timestamp of the given id.
        let after = after.and_then(|id| self.get_message(id)).map(|msg| msg.created_at);

        self.inner
            .iter()
            .filter(|message| message.meta.receiver_id == receiver_id && Some(message.created_at) > after)
            .min_by_key(|message| message.created_at)
    }

    pub fn get_message(&self, id: UpdateID) -> Option<&Message> {
        self.inner.iter().find(|message| message.id == id)
    }

    pub fn next_id(&self) -> UpdateID {
        self.inner.len() as UpdateID
    }
}
