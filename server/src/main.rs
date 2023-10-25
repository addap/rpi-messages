use message::Messages;
use rpi_messages_common::{
    ClientCommand, MessageUpdate, MessageUpdateKind, UpdateResult, IMAGE_BUFFER_SIZE,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::thread;

mod device_handler;
mod image;
mod message;

type Result<T> = std::result::Result<T, anyhow::Error>;

static messages: Mutex<Messages> = Mutex::new(Messages::new());

fn main() {
    // spawn thread to handle TCP connections from devices
    // spawn thread to handle HTTP connections from website/wechat
    let device_thread = thread::spawn(|| device_handler::run());
}
