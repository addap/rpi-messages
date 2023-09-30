use core::borrow::Borrow;

use embassy_time::{Duration, Instant};
use heapless::String;
use rpi_messages_common::{MessageUpdate, IMAGE_BUFFER_SIZE, TEXT_BUFFER_SIZE};

/// With margins we are able to fit 14 * 5 characters on one screen.
/// a.d. TODO we could also do paging for longer messages, since we already need to infer linebreaks anyways.
const TEXT_MESSAGE_NUM: usize = 10;
const IMAGE_MESSAGE_NUM: usize = 2;

pub trait MessageData: Borrow<[u8]> {}

#[derive(Debug)]
pub struct TextData {
    pub text: String<TEXT_BUFFER_SIZE>,
}

#[derive(Debug)]
pub struct ImageData {
    pub image: [u8; IMAGE_BUFFER_SIZE],
}

impl Borrow<[u8]> for TextData {
    fn borrow(&self) -> &[u8] {
        self.text.as_bytes()
    }
}
impl MessageData for TextData {}

impl TextData {
    const fn new() -> Self {
        // a.d. TODO taking a mutable reference to this results in an empty slice!
        Self { text: String::new() }
    }
}

impl Borrow<[u8]> for ImageData {
    fn borrow(&self) -> &[u8] {
        &self.image
    }
}
impl MessageData for ImageData {}

impl ImageData {
    const fn new() -> Self {
        ImageData {
            image: [0; IMAGE_BUFFER_SIZE],
        }
    }
}

#[derive(Clone, Copy)]
pub struct MessageMeta {
    pub lifetime: Duration,
    pub updated_at: Instant,
}

/// An earlier design used Message<T> where T: MessageType
/// but I wanted to have new be a const fn which is not allowed in traits, so we specialize the two types of messages.
pub struct Message<T>
where
    T: MessageData,
{
    pub data: T,
    pub meta: MessageMeta,
}

impl MessageMeta {
    const fn new() -> Self {
        Self {
            lifetime: Duration::MIN,
            updated_at: Instant::MIN,
        }
    }

    pub fn is_active(&self) -> bool {
        let now = Instant::now();
        now < self.updated_at + self.lifetime
    }
}

impl Message<TextData> {
    const fn new() -> Self {
        Self {
            data: TextData::new(),
            meta: MessageMeta::new(),
        }
    }
}

impl Message<ImageData> {
    const fn new() -> Self {
        Self {
            data: ImageData::new(),
            meta: MessageMeta::new(),
        }
    }
}

impl<T> Message<T>
where
    T: MessageData,
{
    pub fn set_meta(&mut self, update: &MessageUpdate) {
        self.meta.updated_at = Instant::now();
        self.meta.lifetime = Duration::from_secs(update.lifetime_sec.into());
    }
}

pub enum DisplayMessage<'a> {
    Text(&'a TextData),
    Image(&'a ImageData),
}

pub struct GenericDisplayMessage<'a> {
    pub data: DisplayMessage<'a>,
    pub meta: MessageMeta,
}

impl<'a> From<&'a Message<TextData>> for GenericDisplayMessage<'a> {
    fn from(value: &'a Message<TextData>) -> Self {
        Self {
            data: DisplayMessage::Text(&value.data),
            meta: value.meta,
        }
    }
}

impl<'a> From<&'a Message<ImageData>> for GenericDisplayMessage<'a> {
    fn from(value: &'a Message<ImageData>) -> Self {
        Self {
            data: DisplayMessage::Image(&value.data),
            meta: value.meta,
        }
    }
}

/// This structure holds all the message data in the system. It is supposed to be allocated statically in a global variable and used in different tasks.
///
/// TODO I think internal pointers would be so nice here. Then I could keep the metadata inside one array and reference the buffers from there.
/// We don't want to mix both types of messages in one array without indirection for the buffers because then all text messages would be the size of image messages.
pub struct Messages {
    pub texts: [Message<TextData>; TEXT_MESSAGE_NUM],
    pub images: [Message<ImageData>; IMAGE_MESSAGE_NUM],
}

impl Messages {
    pub const fn new() -> Self {
        // TODO can we use macros to make this easier if we have more messages?
        Self {
            texts: [
                Message::<TextData>::new(),
                Message::<TextData>::new(),
                Message::<TextData>::new(),
                Message::<TextData>::new(),
                Message::<TextData>::new(),
                Message::<TextData>::new(),
                Message::<TextData>::new(),
                Message::<TextData>::new(),
                Message::<TextData>::new(),
                Message::<TextData>::new(),
            ],
            images: [Message::<ImageData>::new(), Message::<ImageData>::new()],
        }
    }

    /// Get a pointer to either a `Message<TextData>` or `Message<ImageData>` to display.
    ///
    /// - `last_message`: the last message that was displayed. If `None`, it this function returns the oldest active message.
    ///   If `Some(m)` it returns the oldest active message newer than `m`.
    pub fn next_display_message_generic(&self, last_message_time: Instant) -> Option<GenericDisplayMessage<'_>> {
        let messages = self
            .texts
            .iter()
            .map(|text| GenericDisplayMessage::from(text))
            .chain(self.images.iter().map(|image| GenericDisplayMessage::from(image)));

        let latest_message = messages
            .clone()
            .filter(|m| m.meta.is_active() && m.meta.updated_at > last_message_time)
            .min_by_key(|m| m.meta.updated_at);

        if latest_message.is_some() {
            log::debug!("ndm: Found active message that is newer than last_message_time.");
            latest_message
        } else {
            log::debug!("ndm: Found no new active messages, wrapping around to oldest.");
            let earliest_message = messages
                .filter(|m| m.meta.is_active())
                .min_by_key(|m| m.meta.updated_at);

            if earliest_message.is_some() {
                log::debug!("ndm: Found oldest active message.");
                earliest_message
            } else {
                log::debug!("ndm: Found no active messages at all.");
                None
            }
        }
    }

    pub fn next_available_text(&mut self) -> &mut Message<TextData> {
        log::debug!("nat: Retrieve next available text.");
        let message = Messages::next_available_message(&mut self.texts);
        message.data.text.clear();
        message
    }

    pub fn next_available_image(&mut self) -> &mut Message<ImageData> {
        log::debug!("nai: Retrieve next available image.");
        let message = Messages::next_available_message(&mut self.images);
        message.data.image.fill(0);
        message
    }

    /// Returns a pointer to the next message that should be overwritten.
    fn next_available_message<'a, T: MessageData>(messages: &'a mut [Message<T>]) -> &'a mut Message<T> {
        // We first check if there is any message that is currently not active, then we can just use that.
        // Otherwise we return the oldest active message, to be overwritten.
        let mut inactive_index = None;
        let mut oldest_updated_at = Instant::MAX;
        let mut oldest_index = 0;

        for (i, message) in messages.iter_mut().enumerate() {
            if !message.meta.is_active() {
                inactive_index = Some(i);
            } else if message.meta.updated_at < oldest_updated_at {
                oldest_index = i;
                oldest_updated_at = message.meta.updated_at;
            }
        }

        if let Some(inactive_index) = inactive_index {
            return messages.get_mut(inactive_index).unwrap();
        } else {
            return messages.get_mut(oldest_index).unwrap();
        }

        // TODO we use indices since the uncommented version below did not work because returning the pointer
        // makes the first iter_mut live for 'a, i.e. the whole function body so we cannot call iter_mut again.
        // I would have thought with NLL this already works but maybe it needs the more fine grained analysis of Polonius.
        // if let Some(message) = messages.iter_mut().find(|message| !message.is_active()) {
        //     return message;
        // } else {
        //     messages.iter_mut().min_by_key(|message| message.updated_at).unwrap()
        // }
    }
}
