use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{Form, Json, Router, Server};
use common::protocol::UpdateID;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::message::{Message, MessageContent, Messages, SenderID};
use crate::uf2::submit_wifi_config;
use crate::{AppError, WebResult};

#[derive(Debug, Clone, Deserialize)]
struct NewMessage {
    msg: String,
}

#[axum::debug_handler]
async fn new_message(
    State(messages): State<Arc<Mutex<Messages>>>,
    Form(new_message): Form<NewMessage>,
) -> WebResult<Json<()>> {
    let mut guard = messages.lock().await;
    let new_message_content = MessageContent::new_text(new_message.msg)?;
    let new_message = Message::new(
        guard.next_id(),
        0xcafebabe,
        SenderID::Web,
        chrono::Utc::now().naive_utc(),
        chrono::Duration::hours(24),
        new_message_content,
    );

    guard.add_message(new_message);
    Ok(Json(()))
}

async fn latest_message(
    State(messages): State<Arc<Mutex<Messages>>>,
    Path(after): Path<UpdateID>,
) -> WebResult<Json<String>> {
    let guard = messages.lock().await;

    match guard.get_next_message(0xcafebabe, Some(after)) {
        Some(Message {
            content: MessageContent::Text(text),
            ..
        }) => Ok(Json(text.to_owned())),
        _ => Err(AppError::not_found("")),
    }
}

pub async fn run(messages: Arc<Mutex<Messages>>) {
    let app = Router::new()
        .route("/new_message", post(new_message))
        .route("/latest/:after", get(latest_message))
        .with_state(messages)
        .route("/submit_wifi_config", post(submit_wifi_config));

    Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
