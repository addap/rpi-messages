use axum::{
    extract::{Path, State},
    routing::{get, post},
    Form, Json, Router, Server,
};
use common::{protocols::web::NewMessage, types::UpdateID};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::uf2::submit_wifi_config;
use crate::message::{Message, MessageContent, Messages, SenderID};
use crate::{AppError, WebResult};

#[axum::debug_handler]
async fn new_message(
    State(messages): State<Arc<Mutex<Messages>>>,
    Form(new_message): Form<NewMessage>,
) -> WebResult<Json<()>> {
    let mut guard = messages.lock().await;
    let new_message_content = MessageContent::from(new_message);
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

#[axum::debug_handler]
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
