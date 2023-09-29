use embassy_time::{Duration, Instant};
use heapless::String;
use rpi_messages_common::{MessageUpdate, IMAGE_BUFFER_SIZE, TEXT_BUFFER_SIZE};

/// With margins we are able to fit 14 * 5 characters on one screen.
/// a.d. TODO we could also do paging for longer messages, since we already need to infer linebreaks anyways.
const TEXT_MESSAGE_NUM: usize = 10;
const IMAGE_MESSAGE_NUM: usize = 2;

pub trait MessageData {
    // fn new() -> Self;
}

#[derive(Debug)]
pub struct TextData {
    pub text: String<TEXT_BUFFER_SIZE>,
}

#[derive(Debug)]
pub struct ImageData {
    pub image: [u8; IMAGE_BUFFER_SIZE],
}

impl MessageData for TextData {}

impl TextData {
    const fn new() -> Self {
        Self { text: String::new() }
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

/// An earlier design used Message<T> where T: MessageType
/// but I wanted to have new be a const fn which is not allowed in traits, so we specialize the two types of messages.
pub struct Message<T>
where
    T: MessageData,
{
    pub data: T,
    pub lifetime: Duration,
    pub updated_at: Instant,
}

impl Message<TextData> {
    const fn new() -> Message<TextData> {
        Self {
            data: TextData::new(),
            lifetime: Duration::MIN,
            updated_at: Instant::MIN,
        }
    }
}

impl Message<ImageData> {
    const fn new() -> Message<ImageData> {
        Self {
            data: ImageData::new(),
            lifetime: Duration::MIN,
            updated_at: Instant::MIN,
        }
    }
}

impl<T> Message<T>
where
    T: MessageData,
{
    pub fn is_active(&self) -> bool {
        let now = Instant::now();
        now < self.updated_at + self.lifetime
    }

    pub fn set_meta(&mut self, update: &MessageUpdate) {
        self.updated_at = Instant::now();
        self.lifetime = Duration::from_secs(update.lifetime_sec.into());
    }
}

/// TODO this whole data structure organization does not seem optimal.
#[derive(Clone, Copy)]
pub enum GenericMessage<'a> {
    Text(&'a Message<TextData>),
    Image(&'a Message<ImageData>),
}

impl<'a> GenericMessage<'a> {
    pub fn updated_at(&self) -> Instant {
        match self {
            GenericMessage::Text(m) => m.updated_at,
            GenericMessage::Image(m) => m.updated_at,
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
    pub fn next_display_message_generic(&self, last_message_time: Instant) -> Option<GenericMessage<'_>> {
        let next_display_text = Messages::next_display_message(&self.texts, last_message_time);
        let next_display_image = Messages::next_display_message(&self.images, last_message_time);

        // a.d. not really more readable than if let Some()
        next_display_text.map_or_else(
            || next_display_image.map(|m| GenericMessage::Image(m)),
            |next_display_text| {
                next_display_image.map_or_else(
                    || None,
                    |next_display_image| {
                        if next_display_text.updated_at < next_display_image.updated_at {
                            Some(GenericMessage::Text(next_display_text))
                        } else {
                            Some(GenericMessage::Image(next_display_image))
                        }
                    },
                )
            },
        )
    }

    fn next_display_message<T: MessageData>(
        messages: &[Message<T>],
        last_message_time: Instant,
    ) -> Option<&Message<T>> {
        if let Some(message) = messages
            .iter()
            .filter(|m| m.is_active() && m.updated_at > last_message_time)
            .min_by_key(|m| m.updated_at)
        {
            Some(message)
        } else {
            messages.iter().filter(|m| m.is_active()).min_by_key(|m| m.updated_at)
        }
    }

    pub fn next_available_text(&mut self) -> &mut Message<TextData> {
        Messages::next_available_message(&mut self.texts)
    }

    pub fn next_available_image(&mut self) -> &mut Message<ImageData> {
        Messages::next_available_message(&mut self.images)
    }

    /// Returns a pointer to the next message that should be overwritten.
    fn next_available_message<'a, T: MessageData>(messages: &'a mut [Message<T>]) -> &'a mut Message<T> {
        // We first check if there is any message that is currently not active, then we can just use that.
        // Otherwise we return the oldest active message, to be overwritten.
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
