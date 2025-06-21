use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    str::FromStr,
    sync::Arc,
};

use anyhow::{anyhow, Context};
use axum::{
    extract::{DefaultBodyLimit, Multipart, OriginalUri, Path, Query, Request, State},
    http::header,
    response::{self, IntoResponse, Response},
    routing::{get, post},
    Form, Json, Router, ServiceExt,
};
use bytes::Bytes;
use chrono::Utc;
use common::{
    protocols::web::{MessageMeta, NewMessageCreated, NewTextMessage},
    types::{DeviceID, MessageID},
};
use serde::{de, Deserialize, Deserializer};
use tower::Layer;
use tower_http::{normalize_path::NormalizePathLayer, services::ServeFile, trace::TraceLayer};

use crate::message::{image_from_bytes_mime, InsertMessage, Message, MessageContent, SenderID};
use crate::{
    error::{WebError, WebResult},
    message_db::Db,
};

mod image;

const ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 3000);
// Define maximum upload file size to be 8MB.
const UPLOAD_BODY_LIMIT: usize = 8 * 1024 * 1024;
static INDEX_PATH: &str = "webclient/index.html";
static INDEX_JS_PATH: &str = "webclient/index.js";

#[axum::debug_handler]
async fn new_text_message(
    State(messages): State<Arc<dyn Db>>,
    Form(new_message): Form<NewTextMessage>,
) -> WebResult<Json<()>> {
    let new_message_content = MessageContent::new_text(&new_message.text)?;
    let new_message = InsertMessage::new(new_message.meta, SenderID::Web, Utc::now(), new_message_content);

    messages.add_message(new_message).await;
    Ok(Json(()))
}

// #[axum::debug_handler]
// async fn new_image_message(
//     State(messages): State<Arc<Mutex<Messages>>>,
//     Form(new_message): Form<NewImageMessage>,
// ) -> WebResult<Json<()>> {
//     log::info!("urlencode handler");
//     let image = image_from_bytes_mime(&new_message.image, new_message.mime).context("parsing image failed")?;
//     let mut guard = messages.lock().await;
//     let new_message_content = MessageContent::new_image(image)?;
//     let new_message = Message::new(
//         guard.next_id(),
//         new_message.meta,
//         SenderID::Web,
//         Utc::now(),
//         new_message_content,
//     );

//     guard.add_message(new_message);
//     Ok(Json(()))
// }

#[axum::debug_handler]
async fn new_image_message(
    State(messages): State<Arc<dyn Db>>,
    mut multipart: Multipart,
) -> WebResult<Json<NewMessageCreated>> {
    log::info!("Handling new image multipart message.");
    let mut image_bytes_mime: Option<(Bytes, String)> = None;
    let mut receiver: Option<DeviceID> = None;
    let mut duration: Option<chrono::Duration> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .context("multipart field extraction failed")?
    {
        let name = field.name().context("field name extraction failed")?;
        log::info!("Extracting field '{name}'");

        match name {
            "image" => {
                let mime = field
                    .content_type()
                    .context("image field content type extraction failed")?
                    .to_owned();
                let data = field.bytes().await.context("image field bytes extraction failed")?;
                log::info!("\tis image with mime type '{mime}' containing {} bytes.", data.len());
                image_bytes_mime = Some((data.clone(), mime));
            }
            "receiver" => {
                let data = field.text().await.context("recevier field text extraction failed")?;
                let receiver_id = DeviceID::from_str(&data).context("parsing DeviceID failed")?;
                log::info!("\tis receiver id '{:#010X}'.", receiver_id);
                receiver = Some(receiver_id);
            }
            "duration" => {
                let data = field.text().await.context("duration field text extraction failed")?;
                let seconds = i64::from_str(&data).context("duration parsing failed")?;
                log::info!("\tis duration of '{seconds}' seconds.");
                duration = Some(chrono::Duration::seconds(seconds));
            }
            _ => return Err(anyhow!("malformed multipart field {name}").into()),
        }
    }

    let (bytes, mime) = image_bytes_mime.context("image missing")?;
    let image = image_from_bytes_mime(&bytes, mime).context("parsing image failed")?;
    let receiver_id = receiver.context("receiver ID missing")?;
    let duration = duration.context("duration missing")?;
    let meta = MessageMeta { receiver_id, duration };

    let new_message_content = MessageContent::new_image(image)?;
    let new_message = InsertMessage::new(meta, SenderID::Web, Utc::now(), new_message_content);
    let id = messages.add_message(new_message).await;

    Ok(Json(NewMessageCreated { id }))
}

// Note, it's important to put the parameters into a struct since the FromRequestParts impl for Query
// uses a MapDeserializer, i.e. expects to build some map value. And structs are internally represented as maps.
// If we ust just Option<i32> the deserialization always fails.
#[derive(Debug, Deserialize)]
struct LatestQueryParams {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    after: Option<MessageID>,
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
    State(messages): State<Arc<dyn Db>>,
    Path(for_device): Path<String>,
    Query(params): Query<LatestQueryParams>,
) -> WebResult<Response> {
    let receiver_id = DeviceID::from_str(&for_device).context("failed to parse receiver_id")?;

    match messages.get_next_message(receiver_id, params.after).await {
        Some(Message {
            content: MessageContent::Text(text),
            ..
        }) => Ok(([(header::CONTENT_TYPE, "text/plain")], text.text().to_owned()).into_response()),
        Some(Message {
            content: MessageContent::Image(image),
            ..
        }) => Ok(([(header::CONTENT_TYPE, "image/png")], image.png().to_owned()).into_response()),
        _ => Err(WebError::not_found(&format!(
            "Latest message for {:#010X}",
            receiver_id
        ))),
    }
}

pub async fn run(messages: Arc<dyn Db>) {
    let web_client = {
        let index_html = ServeFile::new(INDEX_PATH);
        let index_js = ServeFile::new(INDEX_JS_PATH);

        Router::new()
            .route(
                "/",
                get(|OriginalUri(original_uri): OriginalUri| async move {
                    let path = format!("{}/index.html", original_uri.path());
                    response::Redirect::temporary(&path)
                }),
            )
            // .route_service("/", Redirect::temporary(Uri::from_static(src)))
            .route_service("/index.html", index_html)
            .route_service("/index.js", index_js)
    };

    let api = {
        Router::new()
            .route("/latest/{for_device}", get(latest_message))
            .route("/new_text_message", post(new_text_message))
            // .route("/new_image_message", post(new_image_message))
            .route(
                "/new_image_message",
                post(new_image_message).layer(DefaultBodyLimit::max(UPLOAD_BODY_LIMIT)),
            )
            .with_state(messages)
    };
    let router = Router::new()
        .nest("/web", web_client)
        .nest("/api", api)
        .layer(TraceLayer::new_for_http());

    // Router layers (i.e. middleware) cannot rewrite the request. So to strip of a trailing slash we must
    // first pass through this layer before entering the router.
    let app = NormalizePathLayer::trim_trailing_slash().layer(router);
    // a.d. TODO wtf does this do?
    // from https://github.com/tokio-rs/axum/discussions/2377#discussioncomment-9847433
    let app = ServiceExt::<Request>::into_make_service(app);

    log::info!("Starting web server at {ADDRESS}.");
    let listener = tokio::net::TcpListener::bind(ADDRESS).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
