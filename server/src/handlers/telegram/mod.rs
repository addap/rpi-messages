use std::{any::Any, error::Error, sync::Arc};

use anyhow::{anyhow, Context};
use authorization::{AuthReply, AuthReplyChoice, AuthRequest};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use chrono::{TimeDelta, Utc};
use common::{protocols::web::MessageMeta, types::DeviceID};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use teloxide::{
    dispatching::{
        dialogue::{self, InMemStorage},
        UpdateHandler,
    },
    dptree::{self, Type},
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, MaybeInaccessibleMessage, UpdateId, UpdateKind, User},
    utils::command::BotCommands,
    Bot,
};

use crate::user::User as DbUser;
use crate::{device::Device, error::Result};
use crate::{
    message::{InsertMessage, MessageContent, SenderID},
    message_db::Db,
};

pub mod authorization;

const ALLOWED_CALLBACK_DATA_LENGTH: usize = 64;

#[derive(Debug, Clone, Default)]
enum State {
    #[default]
    Unauthorized,
    Authorized,
    ReceiveTarget,
    ReceiveMessage {
        device: Device,
    },
}

#[derive(Debug, Clone, BotCommands)]
#[command(rename_rule = "lowercase")]
enum SimpleCommand {
    #[command(description = "Print command information")]
    Help,
    #[command(description = "Start the authorizatoin process")]
    Start,
}

#[derive(Clone, BotCommands)]
#[command(rename_rule = "lowercase")]
enum AuthorizedCommand {
    #[command(description = "Send a message to a device")]
    Send,
    #[command(description = "Cancel the current operation")]
    Cancel,
}

// a.d. TODO dependencies need to be clone-able. If this is not in the teloxide docs, add it.
#[derive(Debug, Clone)]
struct Config {
    admin_id: UserId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum CallbackData {
    Auth(AuthReply),
    Target(DeviceID),
}

impl CallbackData {
    fn serialize(&self) -> Result<String> {
        let result = postcard::to_allocvec_cobs(self).context("CallbackData serializaton failed.")?;
        let result = B64.encode(result);
        if result.len() > ALLOWED_CALLBACK_DATA_LENGTH {
            return Err(anyhow!("Telegram callback data length exceeded: {:?}", self));
        }
        Ok(result)
    }

    fn deserialize(s: &str) -> Result<Self> {
        let mut bytes = B64.decode(s).context("CallbackData base64 decoding failed")?;
        let value = postcard::from_bytes_cobs(&mut bytes).context("CallbackData deseialization failed.")?;
        Ok(value)
    }
}

type MyDialogue = Dialogue<State, InMemStorage<State>>;
// a.d. TODO can we just use our anyhow result?
type HandlerResult = std::result::Result<(), Box<dyn Error + Send + Sync>>;

async fn reset_dialogue(state: State, dialogue: MyDialogue, user: User) -> HandlerResult {
    match state {
        State::Unauthorized => {
            log::warn!("Trying to reset dialogue of unauthorized user: {user:?}");
        }
        State::Authorized | State::ReceiveTarget | State::ReceiveMessage { .. } => {
            dialogue.update(State::Authorized).await?;
        }
    }
    Ok(())
}

pub async fn run(db: Arc<dyn Db>) {
    log::info!("Starting Telegram bot.");
    let bot = Bot::from_env();

    let config = Config {
        admin_id: db.get_telegram_admin_id().await,
    };

    // Type check handlers against dependencies.
    let global_deps = dptree::deps![InMemStorage::<State>::new(), db, config];
    let handler = schema(&global_deps);
    // dptree::type_check(handler.sig(), &deps, &[]);

    // a.d. TODO after a restart chats start in unauthorized state again.
    // 1. either use sqlite storage
    // 2. or some fancy middleware that sets people to authroized if they are in the list.
    Dispatcher::builder(bot, handler)
        .dependencies(global_deps)
        .default_handler(|upd| async move { log::warn!("Unhandled update: {:?}", upd) })
        .error_handler(LoggingErrorHandler::with_custom_text(
            "An error has occurred in the dispatcher.",
        ))
        // don't enable this here as this overwrites the signal handler that we set in the main function.
        // .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

fn schema(global_deps: &DependencyMap) -> UpdateHandler<Box<dyn Error + Send + Sync + 'static>> {
    use dptree::case;

    let command_handler = dptree::entry()
        // Simple command are always handled.
        .branch(
            teloxide::filter_command::<SimpleCommand, _>()
                .branch(case![SimpleCommand::Help].endpoint(help))
                .branch(case![SimpleCommand::Start].endpoint(start)),
        )
        // Authorized command handling depends on the current state of the dialogue.
        .branch(
            teloxide::filter_command::<AuthorizedCommand, _>()
                // The /send command is only handled for clients in the Authorized state.
                // For example, to prevent sending multiple messages at once.
                .branch(case![State::Authorized].branch(case![AuthorizedCommand::Send].endpoint(send)))
                // The /cancel command is only handled after authorization.
                // (Not strictly necessary but I wanted to see how to implement that.)
                .branch(
                    dptree::entry()
                        .filter(|state: State| match state {
                            State::Unauthorized => false,
                            State::Authorized | State::ReceiveTarget | State::ReceiveMessage { .. } => true,
                        })
                        .branch(case![AuthorizedCommand::Cancel].endpoint(cancel)),
                ),
        );

    let message_handler = Update::filter_message()
        .branch(command_handler)
        .branch(case![State::ReceiveMessage { device }].endpoint(receive_message))
        .branch(dptree::endpoint(invalid_state));

    let callback_query_handler = Update::filter_callback_query()
        // CallbackQueries from admin
        .branch(
            dptree::entry()
                .filter(|config: Config, q: CallbackQuery| config.admin_id == q.from.id)
                .filter_map(|q: CallbackQuery| CallbackData::deserialize(&q.data.unwrap_or_default()).ok())
                .chain(case![CallbackData::Auth(auth_reply)])
                .endpoint(handle_auth_callback),
        )
        // Other CallbackQueries
        .branch(
            case![State::ReceiveTarget]
                .filter_map(|q: CallbackQuery| CallbackData::deserialize(&q.data.unwrap_or_default()).ok())
                .chain(case![CallbackData::Target(device_id)])
                .endpoint(handle_target_callback),
        );

    let update_type = Type {
        id: Update {
            id: UpdateId(0),
            kind: UpdateKind::Error(Value::Null),
        }
        .type_id(),
        name: "update",
    };
    dptree::type_check(message_handler.sig(), global_deps, &[update_type]);

    dialogue::enter::<Update, InMemStorage<State>, State, _>()
        // Insert the `User` object representing the author of an incoming message into every successive handler function.
        .filter_map(|upd: Update| upd.from().cloned())
        .branch(message_handler)
        .branch(callback_query_handler)
}

async fn help(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, SimpleCommand::descriptions().to_string())
        .await?;
    Ok(())
}

async fn send_auth_request(bot: &Bot, db: &dyn Db, requester: User) -> HandlerResult {
    let admin_id = db.get_telegram_admin_id().await;

    let auth_request = AuthRequest::new(&requester);
    let auth_request_id = auth_request.id();
    db.add_auth_request(auth_request).await;

    let mut answers = Vec::new();
    for choice in [AuthReplyChoice::Accept, AuthReplyChoice::Deny] {
        let callback_data = CallbackData::Auth(AuthReply::new(auth_request_id, choice));
        let serialized = callback_data.serialize()?;
        answers.push(InlineKeyboardButton::callback(choice.to_string(), serialized));
    }

    bot.send_message(
        admin_id,
        format!("The user \"{}\" has requested authorization.", requester.full_name()),
    )
    .reply_markup(InlineKeyboardMarkup::new([answers]))
    .await?;
    Ok(())
}

async fn start(bot: Bot, state: State, db: Arc<dyn Db>, dialogue: MyDialogue, user: User) -> HandlerResult {
    bot.send_message(dialogue.chat_id(), format!("You are in state {state:?}"))
        .await?;
    let dbuser = DbUser::new_telegram(user.id);
    if db.is_user_authorized(dbuser.raw()).await.is_some() {
        bot.send_message(
            dialogue.chat_id(),
            "You are authorized. Use /send command to send a message to someone.",
        )
        .await?;
        dialogue.update(State::Authorized).await?;
    } else {
        send_auth_request(&bot, db.as_ref(), user).await?;
        bot.send_message(dialogue.chat_id(), "Waiting for authorization from administrator.")
            .await?;
    }

    Ok(())
}

async fn send(bot: Bot, db: Arc<dyn Db>, dialogue: MyDialogue) -> HandlerResult {
    let mut devices = Vec::new();
    for device in db.get_devices().await {
        let callback_data = CallbackData::Target(device.id());
        let serialized = callback_data.serialize()?;
        devices.push([InlineKeyboardButton::callback(device.to_string(), serialized)]);
    }
    bot.send_message(dialogue.chat_id(), "Select target device:")
        .reply_markup(InlineKeyboardMarkup::new(devices))
        .await?;
    dialogue.update(State::ReceiveTarget).await?;
    Ok(())
}

async fn handle_target_callback(
    bot: Bot,
    db: Arc<dyn Db>,
    state: State,
    dialogue: MyDialogue,
    target_id: DeviceID,
    user: User,
    q: CallbackQuery,
) -> HandlerResult {
    bot.answer_callback_query(q.id).await?;

    if let Some(device) = db.get_device(target_id).await {
        if let Some(MaybeInaccessibleMessage::Regular(message)) = q.message {
            bot.edit_message_text(
                dialogue.chat_id(),
                message.id,
                format!("Target {device} has been selected successfully!"),
            )
            .await?;
            dialogue.update(State::ReceiveMessage { device }).await?;
        } else {
            log::warn!("Source message of callback not available. User {:?}", user);
            bot.send_message(dialogue.chat_id(), "Internal error. Resetting.")
                .await?;
            reset_dialogue(state, dialogue, user).await?;
        }
    } else {
        bot.send_message(dialogue.chat_id(), format!("Target with id {target_id} not found."))
            .await?;
        reset_dialogue(state, dialogue, user).await?;
    }

    Ok(())
}

async fn receive_message(
    bot: Bot,
    db: Arc<dyn Db>,
    state: State,
    dialogue: MyDialogue,
    device: Device,
    user: User,
    msg: Message,
) -> HandlerResult {
    if let Some(text) = msg.text() {
        bot.send_message(dialogue.chat_id(), format!("Sending message")).await?;

        let meta = MessageMeta {
            receiver_id: device.id(),
            duration: TimeDelta::days(1),
        };
        let content = MessageContent::new_text(text)?;
        let insert_message = InsertMessage::new(meta, SenderID::Telegram, Utc::now(), content);
        db.add_message(insert_message).await;
    } else {
        bot.send_message(dialogue.chat_id(), "Cannot send empty text.").await?;
    }
    reset_dialogue(state, dialogue, user).await?;
    Ok(())
}

async fn cancel(bot: Bot, state: State, dialogue: MyDialogue, user: User) -> HandlerResult {
    bot.send_message(dialogue.chat_id(), "Cancelling dialogue.").await?;
    reset_dialogue(state, dialogue, user).await?;
    Ok(())
}

async fn invalid_state(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(
        msg.chat.id,
        "Unable to handle the message. Type /help to see the usage.",
    )
    .await?;
    Ok(())
}

async fn handle_auth_callback(bot: Bot, db: Arc<dyn Db>, auth_reply: AuthReply, q: CallbackQuery) -> HandlerResult {
    bot.answer_callback_query(q.id).await?;

    if let Some(auth_request) = db.get_auth_request(auth_reply.auth_request_id).await {
        match auth_reply.choice {
            AuthReplyChoice::Accept => {
                let dbuser = DbUser::new_telegram(auth_request.user_id()).authorize();
                db.add_authorized_user(dbuser).await;
                bot.send_message(
                    auth_request.user_id(),
                    "Congratulations, you were authorized by the admin. Use the /send command to send messages.",
                )
                .await?;
                match q.message {
                    Some(MaybeInaccessibleMessage::Regular(message)) => {
                        bot.edit_message_text(q.from.id, message.id, "User was authorized.")
                            .await?;
                    }
                    _ => {}
                }
            }
            AuthReplyChoice::Deny => {
                bot.send_message(
                    auth_request.user_id(),
                    "Sorry, your authorization request was denied. Go away please.",
                )
                .await?;
                match q.message {
                    Some(MaybeInaccessibleMessage::Regular(message)) => {
                        bot.edit_message_text(q.from.id, message.id, "User was denied.").await?;
                    }
                    _ => {}
                }
            }
        }
    } else {
        bot.send_message(q.from.id, "Authorization request not found.").await?;
    }

    Ok(())
}
