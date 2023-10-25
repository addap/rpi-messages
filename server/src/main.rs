use message::Messages;
use rpi_messages_common::{
    ClientCommand, MessageUpdate, MessageUpdateKind, UpdateResult, IMAGE_BUFFER_SIZE,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::Mutex;
use std::thread;

mod device_handler;
mod image;
mod message;
mod web;

type Result<T> = std::result::Result<T, anyhow::Error>;

const MESSAGE_PATH: &str = "./messages.json";

fn main() {
    // restore messages from disk
    let messages = message::Messages::load(Path::new(MESSAGE_PATH));
    let messages = Mutex::new(messages);

    thread::scope({
        let messages = &messages;
        move |scope| {
            // spawn thread to handle TCP connections from devices
            scope.spawn(move || device_handler::run(messages));
            // spawn thread to handle HTTP connections from website/wechat
            scope.spawn(move || web::run(messages));
        }
    });
}
