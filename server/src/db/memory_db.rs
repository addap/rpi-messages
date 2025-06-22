use std::{collections::HashMap, fs::File, path::Path};

use async_trait::async_trait;
use common::{
    protocols::web::MessageMeta,
    types::{DeviceID, MessageID},
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::{
    authorization::AuthRequest,
    device::Device,
    message::{image_from_bytes_mime, InsertMessage, Message, MessageContent, SenderID},
    user::{Authorized, RawUser, User},
    Db,
};
use crate::error::Result;

const MESSAGE_PATH: &str = "./messages.json";

// use type alias to switch out implementations as needed (or enum maybe)
// Db as trait has some restrictions that I don't want to deal with right now.
// 1. async functions in traits make the trair not dyn-compatible, so I need to use generics
// 2. telegram bot api does not like generics for dependencies
// 3. axum debug_handler macro is not allowed for generic functions
// pub type Db = MemoryDb;

#[derive(Debug, Serialize, Deserialize)]
struct InnerMemoryDb {
    devices: HashMap<DeviceID, Device>,
    messages: Vec<Message>,
    authorized_users: HashMap<RawUser, User<Authorized>>,
    // a.d. TODO use User instead of UserId?
    telegram_admin_id: teloxide::types::UserId,
    telegram_auth_requests: HashMap<Uuid, AuthRequest>,
}

impl InnerMemoryDb {
    pub fn dummy(telegram_admin_id: teloxide::types::UserId) -> Self {
        let test_id = DeviceID(0xcafebabe);
        let meta = MessageMeta {
            receiver_id: test_id,
            duration: chrono::Duration::hours(24),
        };
        let love_bytes = include_bytes!("../../pictures/love.png");

        let telegram_admin = User::new_telegram(telegram_admin_id).authorize();
        let test_device = Device::new(test_id, "TestDev".to_string());

        let mut authorized_users = HashMap::new();
        authorized_users.insert(telegram_admin.raw(), telegram_admin);
        let mut devices = HashMap::new();
        devices.insert(test_id, test_device);

        let telegram_auth_requests = HashMap::new();

        Self {
            devices,
            messages: vec![
                Message {
                    id: MessageID(0),
                    meta,
                    sender_id: SenderID::Web,
                    created_at: chrono::Utc::now(),
                    content: MessageContent::new_text("Dummy text").unwrap(),
                },
                Message {
                    id: MessageID(1),
                    meta,
                    sender_id: SenderID::Web,
                    created_at: chrono::Utc::now(),
                    content: MessageContent::new_image(
                        image_from_bytes_mime(love_bytes, "image/png".to_string()).unwrap(),
                    )
                    .unwrap(),
                },
                Message {
                    id: MessageID(2),
                    meta,
                    sender_id: SenderID::Web,
                    created_at: chrono::Utc::now(),
                    content: MessageContent::new_text("Another dummy text").unwrap(),
                },
            ],
            authorized_users,
            telegram_admin_id,
            telegram_auth_requests,
        }
    }

    fn load<P: AsRef<Path>>(p: &P) -> Result<Self> {
        let file = File::open(p)?;
        let messages = serde_json::from_reader(&file)?;
        Ok(messages)
    }

    pub fn store<P: AsRef<Path>>(&self, p: &P) -> Result<()> {
        let file = File::create(p)?;
        serde_json::to_writer(&file, self)?;
        Ok(())
    }
}

/// non-async implementations of Db functions
impl InnerMemoryDb {
    fn get_devices(&self) -> Vec<Device> {
        self.devices.values().cloned().collect()
    }

    fn get_device(&self, id: DeviceID) -> Option<Device> {
        self.devices.get(&id).cloned()
    }

    fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        // guard.store(&MESSAGE_PATH).ok();
    }

    fn get_next_message(&self, receiver_id: DeviceID, after_id: Option<MessageID>) -> Option<Message> {
        let after_time = after_id.and_then(|id| self.get_message(id)).map(|msg| msg.created_at);

        self.messages
            .iter()
            .filter(|message| message.meta.receiver_id == receiver_id && Some(message.created_at) > after_time)
            .min_by_key(|message| message.created_at)
            .cloned()
    }

    fn get_message(&self, id: MessageID) -> Option<Message> {
        self.messages.iter().find(|message| message.id == id).cloned()
    }

    fn next_id(&self) -> MessageID {
        MessageID(self.messages.len() as u32)
    }

    fn is_user_authorized(&self, user: RawUser) -> Option<User<Authorized>> {
        self.authorized_users.get(&user).map(|user| *user)
    }

    fn add_authorized_user(&mut self, user: User<Authorized>) {
        self.authorized_users.insert(user.raw(), user);
    }

    fn get_telegram_admin_id(&self) -> teloxide::types::UserId {
        self.telegram_admin_id
    }

    fn get_auth_request(&self, id: Uuid) -> Option<AuthRequest> {
        self.telegram_auth_requests.get(&id).cloned()
    }

    fn add_auth_request(&mut self, auth_request: AuthRequest) {
        self.telegram_auth_requests.insert(auth_request.id(), auth_request);
    }
}

// a.d. TODO also put the Arc here?
pub struct MemoryDb {
    inner: Mutex<InnerMemoryDb>,
}

impl MemoryDb {
    pub fn dummy(telegram_admin_id: teloxide::types::UserId) -> Self {
        Self {
            inner: Mutex::new(InnerMemoryDb::dummy(telegram_admin_id)),
        }
    }

    fn new(inner: InnerMemoryDb) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }

    fn load<P: AsRef<Path>>(p: &P) -> Result<Self> {
        let file = File::open(p)?;
        let inner = serde_json::from_reader(&file)?;
        Ok(Self::new(inner))
    }

    pub fn store<P: AsRef<Path>>(&mut self, p: &P) -> Result<()> {
        let file = File::create(p)?;
        let inner = self.inner.get_mut();
        serde_json::to_writer(&file, inner)?;
        Ok(())
    }
}

// a.d. TODO according to the Tokio docs, since we don't do async operations on in-memory data (i.e. we don't need to hold the mutex across an await)
// We know that we don't hold the mutex across an await because we never lock it outside of this impl (2nd TODO, wrap the mutex in a struct so that it's private)
#[async_trait]
impl Db for MemoryDb {
    async fn get_devices(&self) -> Vec<Device> {
        let guard = self.inner.lock().await;
        InnerMemoryDb::get_devices(&guard)
    }

    async fn get_device(&self, id: DeviceID) -> Option<Device> {
        let guard = self.inner.lock().await;
        InnerMemoryDb::get_device(&guard, id)
    }

    async fn add_message(&self, message: InsertMessage) -> MessageID {
        let mut guard = self.inner.lock().await;
        let next_id = InnerMemoryDb::next_id(&guard);
        let message = Message::from_insert(next_id, message);
        InnerMemoryDb::add_message(&mut guard, message);
        next_id
    }

    async fn get_next_message(&self, receiver_id: DeviceID, after_id: Option<MessageID>) -> Option<Message> {
        let guard = self.inner.lock().await;
        InnerMemoryDb::get_next_message(&guard, receiver_id, after_id)
    }

    async fn get_message(&self, id: MessageID) -> Option<Message> {
        let guard = self.inner.lock().await;
        InnerMemoryDb::get_message(&guard, id)
    }

    async fn is_user_authorized(&self, user: RawUser) -> Option<User<Authorized>> {
        let guard = self.inner.lock().await;
        InnerMemoryDb::is_user_authorized(&guard, user)
    }

    async fn add_authorized_user(&self, user: User<Authorized>) {
        let mut guard = self.inner.lock().await;
        InnerMemoryDb::add_authorized_user(&mut guard, user);
    }

    async fn get_telegram_admin_id(&self) -> teloxide::types::UserId {
        let guard = self.inner.lock().await;
        InnerMemoryDb::get_telegram_admin_id(&guard)
    }

    async fn get_auth_request(&self, id: Uuid) -> Option<AuthRequest> {
        let guard = self.inner.lock().await;
        InnerMemoryDb::get_auth_request(&guard, id)
    }

    async fn add_auth_request(&self, auth_request: AuthRequest) {
        let mut guard = self.inner.lock().await;
        InnerMemoryDb::add_auth_request(&mut guard, auth_request)
    }
}
