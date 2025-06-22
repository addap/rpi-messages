use async_trait::async_trait;
use common::types::{DeviceID, MessageID};
use uuid::Uuid;

use self::{
    authorization::AuthRequest,
    device::Device,
    message::{InsertMessage, Message},
    user::{Authorized, RawUser, User},
};

pub mod authorization;
pub mod device;
pub mod memory_db;
pub mod message;
pub mod user;

// a.d. so as far as I understand, when async functions are *declared* using the `async` keyword in traits, then the returned future loses all send & sync bounds.
// So you should still declare futures as `-> impl Future<Output = X> + Send + 'static`.
// But you can use async for the actual implementation of the trait.
#[async_trait]
pub trait Db: Send + Sync {
    async fn get_devices(&self) -> Vec<Device>;
    async fn get_device(&self, id: DeviceID) -> Option<Device>;
    async fn get_message(&self, id: MessageID) -> Option<Message>;
    async fn add_message(&self, message: InsertMessage) -> MessageID;
    async fn get_next_message(&self, receiver_id: DeviceID, after: Option<MessageID>) -> Option<Message>;
    async fn is_user_authorized(&self, user: RawUser) -> Option<User<Authorized>>;
    async fn add_authorized_user(&self, user: User<Authorized>);
    async fn get_telegram_admin_id(&self) -> teloxide::types::UserId;
    async fn get_auth_request(&self, id: Uuid) -> Option<AuthRequest>;
    async fn add_auth_request(&self, auth_request: AuthRequest);
}
