use core::fmt;
use std::sync::Arc;

use anyhow::anyhow;
use axum::{http::StatusCode, response::IntoResponse};
use dotenvy::dotenv;
use message::Db;
use teloxide::types::UserId;
use tokio::{runtime::Runtime, signal, sync::Mutex};

use crate::users::User;

mod handlers;
mod message;
mod users;

const MESSAGE_PATH: &str = "./messages.json";

fn main() -> Result<()> {
    dotenv().expect(".env file not found");
    env_logger::init();

    let body = async {
        // Restore messages from disk.
        let db = init_db().await;
        let mut join_handles = Vec::new();

        // spawn task to handle TCP connections from devices
        join_handles.push(tokio::spawn(handlers::device::run(db.clone())));
        // spawn task to handle HTTP connections from website
        join_handles.push(tokio::spawn(handlers::web::run(db.clone())));
        // spawn task to handle Telegram webhooks
        join_handles.push(tokio::spawn(handlers::telegram::run(db.clone())));

        // for (i, handle) in join_handles.into_iter().enumerate() {
        //     handle.await?;
        //     log::info!("Joined task {i}...");
        // }
        // log::info!("Joined all tasks.");
        signal::ctrl_c().await.expect("failed to listen for Ctrl-C");
        Ok(())
    };

    let rt = Runtime::new()?;
    rt.block_on(body)
}

// Messages need to be in an Arc to use axum::debug_handler.
async fn init_db() -> Arc<Mutex<Db>> {
    // let messages = message::Messages::load(&MESSAGE_PATH);
    let admin = UserId(
        std::env::var("ADMIN_CHAT_ID")
            .expect("ADMIN_CHAT_ID not set")
            .parse()
            .expect("ADMIN_CHAT_ID invalid"),
    );
    let admin = User::new_telegram(admin).authenticate();
    let messages = Db::dummy(admin);
    Arc::new(Mutex::new(messages))
}

#[derive(Debug)]
struct AppError {
    code: StatusCode,
    error: anyhow::Error,
}

impl AppError {
    pub const fn new(code: StatusCode, error: anyhow::Error) -> Self {
        Self { code, error }
    }

    pub fn not_found(item: &str) -> Self {
        Self {
            code: StatusCode::NOT_FOUND,
            error: anyhow!("{item} not found"),
        }
    }

    pub fn bad_request(msg: &str) -> Self {
        Self {
            code: StatusCode::BAD_REQUEST,
            error: anyhow!("{}", msg),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} - {}", self.code, self.error)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (self.code, format!("{}", self.error)).into_response()
    }
}

type Result<T> = anyhow::Result<T>;
type WebResult<T> = std::result::Result<T, AppError>;

impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            error,
        }
    }
}
