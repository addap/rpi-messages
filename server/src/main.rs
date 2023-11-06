use axum::{http::StatusCode, response::IntoResponse};
use core::fmt;
use message::Messages;
use std::{sync::Arc, thread};
use tokio::{sync::Mutex, task};

mod device_handler;
mod image;
mod message;
mod uf2;
mod web;

const MESSAGE_PATH: &str = "./messages.json";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Restore messages from disk.
    let messages = init_messages().await;
    let messages = Arc::new(messages);

    // Create local taskset to spawn tasks on our own thread.
    let local = task::LocalSet::new();
    // spawn task to handle TCP connections from devices
    local.spawn_local(device_handler::run(messages.clone()));
    // spawn task to handle HTTP connections from website/wechat
    local.spawn_local(web::run(messages.clone()));
    local.await;
}

async fn init_messages() -> Mutex<Messages> {
    let loaded = message::Messages::load(&MESSAGE_PATH);
    let messages = Mutex::new(loaded);
    messages
}

pub enum AppError {
    NotFound,
    Generic(anyhow::Error),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Generic(e) => f.write_str(&e.to_string()),
            AppError::NotFound => f.write_str("Not Found"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let msg = self.to_string();
        let status_code = match self {
            AppError::Generic(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::NotFound => StatusCode::NOT_FOUND,
        };

        (status_code, msg).into_response()
    }
}

type Result<T> = std::result::Result<T, anyhow::Error>;
type WebResult<T> = std::result::Result<T, AppError>;

impl From<anyhow::Error> for AppError {
    fn from(value: anyhow::Error) -> Self {
        AppError::Generic(value)
    }
}
