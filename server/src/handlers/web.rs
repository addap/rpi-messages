use axum::{
    extract::{Path, Request, State},
    routing::{get, post},
    Form, Json, Router, ServiceExt,
};
use chrono::Utc;
use common::{protocols::web::NewMessage, types::UpdateID};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::sync::Mutex;
use tower::Layer;
use tower_http::normalize_path::NormalizePathLayer;

use super::uf2::submit_wifi_config;
use crate::message::{Message, MessageContent, Messages, SenderID};
use crate::{AppError, WebResult};

const ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3000);

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
        Utc::now(),
        chrono::Duration::hours(24),
        new_message_content,
    );

    guard.add_message(new_message);
    Ok(Json(()))
}

#[axum::debug_handler]
async fn latest_message(
    State(messages): State<Arc<Mutex<Messages>>>,
    after: Option<Path<UpdateID>>,
) -> WebResult<Json<String>> {
    // let guard = messages.lock().await;
    let after = after.map(|Path(after)| after);

    log::info!("web::latest_message: after={after:?}");
    Ok(Json("ok".to_owned()))

    // match guard.get_next_message(0xcafebabe, after) {
    //     Some(Message {
    //         content: MessageContent::Text(text),
    //         ..
    //     }) => Ok(Json(text.to_owned())),
    //     _ => Err(AppError::not_found("")),
    // }
}

pub async fn run(messages: Arc<Mutex<Messages>>) {
    let router = Router::new()
        .route("/new_message", post(new_message))
        .route("/latest/{after}", get(latest_message))
        .route("/latest", get(latest_message))
        .with_state(messages)
        .route("/submit_wifi_config", post(submit_wifi_config));
    let app = NormalizePathLayer::trim_trailing_slash().layer(router);
    // a.d. TODO wtf does this do?
    // from https://github.com/tokio-rs/axum/discussions/2377#discussioncomment-9847433
    let app = ServiceExt::<Request>::into_make_service(app);

    log::info!("Starting web server at {ADDRESS}");
    let listener = tokio::net::TcpListener::bind(ADDRESS).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
