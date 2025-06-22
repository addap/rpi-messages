use std::sync::Arc;

use dotenvy::dotenv;
use teloxide::types::UserId;
use tokio::{runtime::Runtime, signal};

use crate::db::memory_db::MemoryDb;

mod db;
mod error;
mod handlers;

fn main() -> error::Result<()> {
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
        // join_handles.push(tokio::spawn(handlers::telegram::run(db.clone())));
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
async fn init_db() -> Arc<MemoryDb> {
    // let messages = message::Messages::load(&MESSAGE_PATH);
    let telegram_admin_id = {
        let id = std::env::var("ADMIN_CHAT_ID")
            .expect("ADMIN_CHAT_ID not set")
            .parse()
            .expect("ADMIN_CHAT_ID invalid");
        UserId(id)
    };
    let messages = MemoryDb::dummy(telegram_admin_id);
    Arc::new(messages)
}
