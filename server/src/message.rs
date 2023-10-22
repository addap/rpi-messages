//! We have a repository of messages.
//! Different API endpoints add to that repository.
//!  
//!

use std::{
    fs::{File, OpenOptions},
    io::Read,
    io::Write,
    path::Path,
    time::Instant,
};

use image::{imageops::resize, EncodableLayout, ImageBuffer, RgbImage};
use rpi_messages_common::{DeviceID, UpdateID, TEXT_BUFFER_SIZE};
use rpi_messages_common::{IMAGE_BYTES_PER_PIXEL, IMAGE_HEIGHT, IMAGE_WIDTH};
use serde::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, anyhow::Error>;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum SenderID {
    Web,
    Wechat,
}

#[derive(Debug, Serialize, Deserialize)]
enum MessageContent {
    Text(String),
    Image(Vec<u8>),
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    uuid: UpdateID,
    receiver_id: DeviceID,
    sender_id: SenderID,
    delivered: bool,
    created_at: chrono::NaiveDateTime,
    lifetime_secs: u32,
    content: MessageContent,
}

#[derive(Debug, Serialize, Deserialize)]
struct Messages(Vec<Message>);

impl MessageContent {
    fn new_text(text: String) -> Result<Vec<Self>> {
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

    fn new_image(img: RgbImage) -> Result<Self> {
        let bytes = convert_image(img);
        Ok(MessageContent::Image(bytes))
    }
}

impl Message {
    fn new(
        uuid: UpdateID,
        receiver_id: DeviceID,
        sender_id: SenderID,
        created_at: chrono::NaiveDateTime,
        lifetime: chrono::Duration,
        content: MessageContent,
    ) -> Self {
        Self {
            uuid,
            receiver_id,
            sender_id,
            delivered: false,
            created_at,
            lifetime_secs: lifetime.num_seconds() as u32,
            content,
        }
    }
}

impl Messages {
    fn load(p: &Path) -> Result<Self> {
        let file = File::open(p)?;
        let messages = serde_json::from_reader(&file)?;
        Ok(messages)
    }

    fn store(&self, p: &Path) -> Result<()> {
        let file = OpenOptions::new().write(true).create(true).open(p)?;
        serde_json::to_writer(&file, self)?;
        Ok(())
    }

    fn add_message(&mut self, message: Message) {
        self.0.push(message)
    }

    fn find_next_message(&mut self, receiver_id: DeviceID) -> Option<&mut Message> {
        self.0
            .iter_mut()
            .find(|msg| !msg.delivered && msg.receiver_id == receiver_id)
    }
}

fn convert_image(img: RgbImage) -> Vec<u8> {
    //
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
    bytes
}
