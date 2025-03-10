use anyhow::anyhow;
use axum::{
    extract::{Multipart, Path, Query, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Form, Json, Router, ServiceExt,
};
use chrono::Utc;
use common::{
    protocols::web::{NewImageMessage, NewTextMessage},
    types::UpdateID,
};
use serde::{de, Deserialize, Deserializer};
use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path,
    str::FromStr,
    sync::Arc,
};
use tokio::sync::Mutex;
use tower::Layer;
use tower_http::{normalize_path::NormalizePathLayer, services::ServeFile, trace::TraceLayer};

use super::uf2::submit_wifi_config;
use crate::{
    message::{Message, MessageContent, Messages, SenderID},
    AppError, WebResult,
};

const ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3000);
static INDEX_PATH: &str = "webclient/index.html";

#[axum::debug_handler]
async fn new_text_message(
    State(messages): State<Arc<Mutex<Messages>>>,
    Form(new_message): Form<NewTextMessage>,
) -> WebResult<Json<()>> {
    let mut guard = messages.lock().await;
    let new_message_content = MessageContent::new_text(new_message.text)?;
    let new_message = Message::new(
        guard.next_id(),
        new_message.meta,
        SenderID::Web,
        Utc::now(),
        new_message_content,
    );

    guard.add_message(new_message);
    Ok(Json(()))
}

#[axum::debug_handler]
async fn new_image_message(
    State(messages): State<Arc<Mutex<Messages>>>,
    Form(new_message): Form<NewImageMessage>,
) -> WebResult<Json<()>> {
    log::info!("urlencode handler");
    let mut guard = messages.lock().await;
    let new_message_content = MessageContent::new_image(new_message.image)?;
    let new_message = Message::new(
        guard.next_id(),
        new_message.meta,
        SenderID::Web,
        Utc::now(),
        new_message_content,
    );

    guard.add_message(new_message);
    Ok(Json(()))
}

#[axum::debug_handler]
async fn mp_new_image_message(
    State(messages): State<Arc<Mutex<Messages>>>,
    mut multipart: Multipart,
) -> WebResult<String> {
    // let mut guard = messages.lock().await;
    // let new_message_content = MessageContent::new_image(new_message.image)?;
    // let new_message = Message::new(
    //     guard.next_id(),
    //     new_message.meta,
    //     SenderID::Web,
    //     Utc::now(),
    //     new_message_content,
    // );

    // guard.add_message(new_message);
    log::info!("Multipart handler");
    Ok("OK".to_string())
}

// Note, it's important to put the parameters into a struct since the FromRequestParts impl for Query
// uses a MapDeserializer, i.e. expects to build some map value. And structs are internally represented as maps.
// If we ust just Option<i32> the deserialization always fails.
#[derive(Debug, Deserialize)]
struct LatestQueryParams {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    after: Option<i32>,
}

/// Serde deserialization decorator to map empty Strings to None,
/// from https://github.com/tokio-rs/axum/blob/da3539cb0e5eed381361b2e688a776da77c52cd6/examples/query-params-with-empty-strings/src/main.rs#L44
fn empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: fmt::Display,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => FromStr::from_str(s).map_err(de::Error::custom).map(Some),
    }
}

#[axum::debug_handler]
async fn latest_message(
    State(messages): State<Arc<Mutex<Messages>>>,
    Path(for_device): Path<String>,
    Query(params): Query<LatestQueryParams>,
) -> WebResult<Response> {
    let receiver_id = u32::from_str_radix(&for_device, 16).map_err(|e| {
        log::error!("{}", e);
        AppError::bad_request("u32 hex")
    })?;
    let after = params.after.map(|v| v as UpdateID);

    let guard = messages.lock().await;
    match guard.get_next_message(receiver_id, after) {
        Some(Message {
            content: MessageContent::Text(text),
            ..
        }) => Ok(text.to_owned().into_response()),
        Some(Message {
            content: MessageContent::Image { png, .. },
            ..
        }) => Ok(([(header::CONTENT_TYPE, "image/png")], png.clone()).into_response()),
        _ => Err(AppError::not_found(&format!(
            "Latest message for {:#010X}",
            receiver_id
        ))),
    }
}

pub async fn run(messages: Arc<Mutex<Messages>>) {
    let router = Router::new()
        .route_service("/index.html", ServeFile::new(INDEX_PATH))
        .route("/new_text_message", post(new_text_message))
        .route("/new_image_message", post(new_image_message))
        .route("/mp_new_image_message", post(mp_new_image_message))
        .route("/latest/{for_device}", get(latest_message))
        .with_state(messages)
        .route("/submit_wifi_config", post(submit_wifi_config))
        .layer(TraceLayer::new_for_http());
    // Router layers (i.e. middleware) cannot rewrite the request. So to strip of a trailing slash we must
    // first pass through this layer before entering the router.
    let app = NormalizePathLayer::trim_trailing_slash().layer(router);
    // a.d. TODO wtf does this do?
    // from https://github.com/tokio-rs/axum/discussions/2377#discussioncomment-9847433
    let app = ServiceExt::<Request>::into_make_service(app);

    log::info!("Starting web server at {ADDRESS}");
    let listener = tokio::net::TcpListener::bind(ADDRESS).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
