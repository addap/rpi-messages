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

// The different ways of declaring async functions in traits (after Rust 1.75) as far as I understand it.
//
// 1. async fn foo() -> Bar
//    - Desugars to `fn foo() -> impl Future<Output = Bar>`
//    - Supported by the compiler directly, so this is the way forward.
//    - But returned future has no additional bounds like Send or Sync,
//        so if these are required (e.g. since I'm using the multithreaded tokio executor my futures must be Send)
//        one has to write them explicitly like in 2.
//
// 2. fn foo() -> impl Future<Output = Bar> + Send
//    - The logical way to write it, says there exists some type that implementes Future and gives the correct output.
//    - The implementation of the trait can still use the `async` keyword
//    - We are able to write additional bounds if required.
//
// The disadvantage of 1. & 2. is that traits using "return position impl Trait" are not dyn-compatible, i.e. for any function
// taking a Db instant, I would also have to use `impl Db` in an argument position. And this means that I'm using generics in those functions.
// The axum and telegram libraries that are supposed to work with a Db instance both don't like generics: axum's debug_handler macro & telegram's dependency injection framework.
// So I'm either using approach 3, or no trait at all.
//
// 3. async fn foo() -> Bar and #[async_trait] at trait declaration & implementation
//   - Macro transforms declaration to `fn foo() -> Pin<Box<dyn Future<Output = Bar>>`
//   - Explanation to this approach here https://smallcultfollowing.com/babysteps/blog/2019/10/26/async-fn-in-traits-are-hard/
//   - This trait is dyn-compatible so I can use `dyn Db` for axum and telegram.
//   - One caveat, the `is_user_authorized` function used to be generic and take a User<T>, but even though this is supposed to be supported, it did not work for me.
//       (even adding a where bound like User<T>: Send).

/// Generic interface to our application state.
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
