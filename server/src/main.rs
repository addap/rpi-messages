use rpi_messages_common::{
    ClientCommand, MessageUpdate, MessageUpdateKind, UpdateResult, IMAGE_BUFFER_SIZE,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

mod image;
mod message;

fn main() {
    //
}
