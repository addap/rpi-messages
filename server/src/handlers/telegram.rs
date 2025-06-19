use std::{error::Error, str::FromStr, sync::Arc};

use common::types::DeviceID;
use teloxide::{
    dispatching::{
        dialogue::{self, InMemStorage},
        UpdateHandler,
    },
    dptree,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
    utils::command::BotCommands,
    Bot,
};
use tokio::sync::Mutex;

use crate::message::Db;

#[derive(Clone, Default)]
enum State {
    #[default]
    Start,
    Authed,
    ReceiveTarget,
    ReceiveMessage {
        device_id: DeviceID,
    },
}

#[derive(Clone, BotCommands)]
#[command(rename_rule = "lowercase")]
enum Command {
    Help,
    Start,
    Cancel,
    Send,
}

type MyDialogue = Dialogue<State, InMemStorage<State>>;
type HandlerResult = Result<(), Box<dyn Error + Send + Sync>>;

pub async fn run(db: Arc<Mutex<Db>>) {
    log::info!("Starting Telegram bot.");
    let bot = Bot::from_env();

    Dispatcher::builder(bot, schema())
        .dependencies(dptree::deps![InMemStorage::<State>::new()])
        // .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

fn schema() -> UpdateHandler<Box<dyn Error + Send + Sync + 'static>> {
    use dptree::case;

    let command_handler = teloxide::filter_command::<Command, _>()
        .branch(
            case![State::Start]
                .branch(case![Command::Help].endpoint(help))
                .branch(case![Command::Start].endpoint(start)),
        )
        .branch(
            case![State::Authed].branch(
                case![Command::Send]
                    .endpoint(send)
                    .branch(case![Command::Help].endpoint(help)),
            ),
        )
        .branch(case![Command::Cancel].endpoint(cancel));

    let message_handler = Update::filter_message()
        .branch(command_handler)
        .branch(case![State::ReceiveMessage { device_id }].endpoint(receive_message))
        .branch(dptree::endpoint(invalid_state));

    let callback_query_handler =
        Update::filter_callback_query().branch(case![State::ReceiveTarget].endpoint(receive_target));

    dialogue::enter::<Update, InMemStorage<State>, State, _>()
        .branch(message_handler)
        .branch(callback_query_handler)
}

async fn help(bot: Bot, msg: Message) -> HandlerResult {
    bot.send_message(msg.chat.id, Command::descriptions().to_string())
        .await?;
    Ok(())
}

async fn start(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    let mychatid = ChatId(0);

    if dialogue.chat_id() == mychatid {
        bot.send_message(dialogue.chat_id(), "Authentication successful.")
            .await?;
        dialogue.update(State::Authed).await?;
    }

    Ok(())
}

async fn cancel(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    bot.send_message(dialogue.chat_id(), "Chancelling dialogue.").await?;
    dialogue.exit().await?;
    Ok(())
}

async fn send(bot: Bot, dialogue: MyDialogue, msg: Message) -> HandlerResult {
    let devices = ["0xcafebabe", "0xfeedc0de"].map(|device| [InlineKeyboardButton::callback(device, device)]);
    bot.send_message(dialogue.chat_id(), "Select target device:")
        .reply_markup(InlineKeyboardMarkup::new(devices))
        .await?;
    dialogue.update(State::ReceiveTarget).await?;
    Ok(())
}

async fn receive_message(bot: Bot, dialogue: MyDialogue, device_id: DeviceID, msg: Message) -> HandlerResult {
    let text = msg.text().unwrap_or("<empty text>");
    bot.send_message(
        dialogue.chat_id(),
        format!("Sending message {text} to device {device_id}"),
    )
    .await?;
    dialogue.exit().await?;
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

async fn receive_target(bot: Bot, dialogue: MyDialogue, q: CallbackQuery) -> HandlerResult {
    if let Some(target) = &q.data {
        match DeviceID::from_str(target) {
            Ok(device_id) => {
                bot.send_message(
                    dialogue.chat_id(),
                    format!("Target {target} has been selected successfully!"),
                )
                .await?;
                dialogue.update(State::ReceiveMessage { device_id }).await?;
            }
            Err(e) => {
                bot.send_message(dialogue.chat_id(), format!("Invalid device id submitted: {e}"))
                    .await?;
                dialogue.exit().await?;
            }
        }
    }

    Ok(())
}
