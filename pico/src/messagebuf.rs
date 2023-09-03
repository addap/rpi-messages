use core::{array, slice};

use embassy_time::{Duration, Instant};
use heapless::String;

/// With margins we are able to fit 14 * 5 characters on one screen.
/// a.d. TODO we could also do paging for longer messages, since we already need to infer linebreaks anyways.
const TEXT_MESSAGE_SIZE: usize = 70;
const TEXT_MESSAGE_NUM: usize = 10;

/// We save images raw (TODO which endian-ness) so we use the exact screen dimensions.
const IMAGE_MESSAGE_SIZE: usize = 160 * 128 * 2;
const IMAGE_MESSAGE_NUM: usize = 2;

pub trait MessageType {
    fn new() -> Self;
}

#[derive(Debug)]
pub struct Text {
    pub text: String<TEXT_MESSAGE_SIZE>,
}

#[derive(Debug)]
pub struct Image {
    pub image: [u8; IMAGE_MESSAGE_SIZE],
}

impl MessageType for Text {
    fn new() -> Self {
        Self { text: String::new() }
    }
}
impl MessageType for Image {
    fn new() -> Self {
        Image {
            image: [0; IMAGE_MESSAGE_SIZE],
        }
    }
}

pub struct Message<T>
where
    T: MessageType,
{
    pub data: T,
    pub lifetime: Duration,
    pub updated_at: Instant,
}

impl<T> Message<T>
where
    T: MessageType,
{
    fn new() -> Self {
        Self {
            data: T::new(),
            lifetime: Duration::default(),
            updated_at: Instant::now(),
        }
    }

    pub fn is_active(&self) -> bool {
        let now = Instant::now();
        now > self.updated_at + self.lifetime
    }
}

// TODO not sure we want to have this on the stack.
pub struct Messages {
    pub texts: [Message<Text>; TEXT_MESSAGE_NUM],
    pub images: [Message<Image>; IMAGE_MESSAGE_NUM],
}

impl Messages {
    pub fn new() -> Self {
        Self {
            texts: array::from_fn::<Message<Text>, TEXT_MESSAGE_NUM, _>(|_| Message::new()),
            images: array::from_fn::<Message<Image>, IMAGE_MESSAGE_NUM, _>(|_| Message::new()),
        }
    }

    pub fn next_text(&mut self) -> &mut Message<Text> {
        Messages::next_message(&mut self.texts)
    }

    pub fn next_image(&mut self) -> &mut Message<Image> {
        Messages::next_message(&mut self.images)
    }

    /// Returns a pointer to the next message that is to be overwritten.
    fn next_message<'a, T: MessageType>(messages: &'a mut [Message<T>]) -> &'a mut Message<T> {
        // We first check if there is any message that is currently not active, then we can just use that.
        // Otherwise we just return the oldest active message, to be overwritten.
        // We use indices because: TODO the uncommented version below did not work because returning the pointer
        // makes the first iter_mut live for 'a, i.e. the whole function body so we cannot call iter_mut again.
        let mut inactive_index = None;
        let mut oldest_updated_at = Instant::MAX;
        let mut oldest_index = 0;

        for (i, message) in messages.iter_mut().enumerate() {
            if !message.is_active() {
                inactive_index = Some(i);
            } else if message.updated_at < oldest_updated_at {
                oldest_index = i;
                oldest_updated_at = message.updated_at;
            }
        }

        if let Some(inactive_index) = inactive_index {
            return messages.get_mut(inactive_index).unwrap();
        } else {
            return messages.get_mut(oldest_index).unwrap();
        }

        // if let Some(message) = messages.iter_mut().find(|message| !message.is_active()) {
        //     return message;
        // } else {
        //     messages.iter_mut().min_by_key(|message| message.updated_at).unwrap()
        // }
    }
}
