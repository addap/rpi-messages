//! We have a repository of messages.
//! Different API endpoints add to that repository.
//!  
//!

use std::io::Cursor;

use anyhow::anyhow;
use common::{
    consts::{IMAGE_BYTES_PER_PIXEL, IMAGE_HEIGHT, IMAGE_WIDTH, TEXT_BUFFER_SIZE},
    protocols::{pico::UpdateKind, web::MessageMeta},
    types::{MessageID, TextLength},
};
use image::{codecs::png::PngEncoder, DynamicImage, ImageFormat, ImageReader, ImageResult};
use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SenderID {
    Web,
    Telegram,
}

/// Contains textual content of a message.
/// To uphold an invariant on the string length this is a separate struct with private fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextContent {
    text: String,
}

impl TextContent {
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Contains image content of a message.
/// To uphold an invariant on the image data length this is a separate struct with private fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageContent {
    png: Vec<u8>,
    rgb565: Vec<u8>,
}

impl ImageContent {
    pub fn png(&self) -> &[u8] {
        &self.png
    }

    pub fn rgb565(&self) -> &[u8] {
        &self.rgb565
    }
}

/// Contains the content of a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    Text(TextContent),
    Image(ImageContent),
}

impl MessageContent {
    // a.d. TOOD str vs String
    pub fn new_text(text: &str) -> Result<Self> {
        if text.bytes().len() <= TEXT_BUFFER_SIZE {
            Ok(MessageContent::Text(TextContent { text: text.to_string() }))
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
            texts.push(MessageContent::Text(TextContent { text: s }));

            bytes = &bytes[TEXT_BUFFER_SIZE..]
        }
        Ok(texts)
    }

    pub fn new_image(img: DynamicImage) -> Result<Self> {
        let img_resized = image::imageops::resize(
            &img,
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
        Ok(MessageContent::Image(ImageContent { png, rgb565 }))
    }
}
impl From<&MessageContent> for UpdateKind {
    fn from(value: &MessageContent) -> UpdateKind {
        match value {
            MessageContent::Text(tc) => UpdateKind::Text(tc.text.len() as TextLength),
            MessageContent::Image { .. } => UpdateKind::Image,
        }
    }
}

pub fn image_from_bytes_mime(bytes: &[u8], mime: String) -> ImageResult<DynamicImage> {
    let mut img_reader = ImageReader::new(Cursor::new(bytes));
    match ImageFormat::from_mime_type(mime) {
        Some(format) => {
            img_reader.set_format(format);
        }
        None => {
            img_reader = img_reader.with_guessed_format().expect("Cursor io does not fail");
        }
    }
    img_reader.decode()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageID,
    // a.d. TODO why meta separate? either put other stuff also in there or remove it.
    pub meta: MessageMeta,
    pub sender_id: SenderID,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub content: MessageContent,
}

impl Message {
    pub fn from_insert(id: MessageID, message: InsertMessage) -> Self {
        Self {
            id,
            meta: message.meta,
            sender_id: message.sender_id,
            created_at: message.created_at,
            content: message.content,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct InsertMessage {
    pub meta: MessageMeta,
    pub sender_id: SenderID,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub content: MessageContent,
}

impl InsertMessage {
    pub fn new(
        meta: MessageMeta,
        sender_id: SenderID,
        created_at: chrono::DateTime<chrono::Utc>,
        content: MessageContent,
    ) -> Self {
        Self {
            meta,
            sender_id,
            created_at,
            content,
        }
    }
}
