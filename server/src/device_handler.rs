use rpi_messages_common::{
    ClientCommand, MessageUpdate, MessageUpdateKind, UpdateResult, IMAGE_BUFFER_SIZE,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;

use crate::message::{MessageContent, Messages};
use crate::MESSAGES;

pub fn run() {
    let listener = TcpListener::bind("0.0.0.0:1337").unwrap();

    loop {
        println!("Listening for new connections.");
        match listener.accept() {
            Ok((mut socket, addr)) => {
                println!("new client at {:?}", addr);

                'client: loop {
                    match parse_client_command(&mut socket) {
                        None => {
                            eprintln!("Malformed client command.");
                            break 'client;
                        }
                        Some(ClientCommand::CheckUpdate(device_id, after)) => {
                            let guard = MESSAGES.lock().unwrap();
                            let result = match guard.get_next_message(device_id, after) {
                                Some(message) => {
                                    let message_update = MessageUpdate {
                                        lifetime_sec: message.lifetime_secs,
                                        id: message.id,
                                        // FIXME strange that auto-referencing does not work here.
                                        kind: (&message.content).into(),
                                    };
                                    UpdateResult::Update(message_update)
                                }
                                None => UpdateResult::NoUpdate,
                            };

                            let bytes = result.serialize().unwrap();
                            socket.write_all(&bytes).unwrap();
                        }
                        Some(ClientCommand::RequestUpdate(id)) => {
                            let guard = MESSAGES.lock().unwrap();
                            let message =
                                guard.get_message(id).expect("Requested message not found.");
                            match &message.content {
                                MessageContent::Text(text) => {
                                    socket.write_all(text.as_bytes()).unwrap();
                                }
                                MessageContent::Image(image) => {
                                    socket.write_all(image.as_slice()).unwrap()
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => println!("couldn't get client: {e:?}"),
        }
    }
}

fn parse_client_command(socket: &mut TcpStream) -> Option<ClientCommand> {
    let mut command_buf = [0u8; ClientCommand::SERIALIZED_LEN];
    socket.read_exact(&mut command_buf).ok()?;
    ClientCommand::deserialize(&command_buf).ok()
}
