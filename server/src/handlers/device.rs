use common::postcard::experimental::max_size::MaxSize;
use common::{
    consts::IMAGE_BUFFER_SIZE,
    protocols::pico::{CheckUpdateResult, ClientCommand, Update, UpdateKind},
};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
// use std::net::{TcpListener, TcpStream};

use crate::message::{MessageContent, Messages};

pub async fn run(messages: Arc<Mutex<Messages>>) {
    let listener = TcpListener::bind("0.0.0.0:1338").await.unwrap();

    loop {
        println!("Listening for client connections.");
        match listener.accept().await {
            Ok((mut socket, addr)) => {
                println!("new client at {:?}", addr);

                'client: loop {
                    match parse_client_command(&mut socket).await {
                        None => {
                            eprintln!("Malformed client command.");
                            break 'client;
                        }
                        Some(ClientCommand::CheckUpdate(device_id, after)) => {
                            let guard = messages.lock().await;
                            let result = match guard.get_next_message(device_id, after) {
                                Some(message) => {
                                    let message_update = Update {
                                        lifetime_sec: message.lifetime_secs,
                                        id: message.id,
                                        kind: UpdateKind::from(&message.content),
                                    };
                                    CheckUpdateResult::Update(message_update)
                                }
                                None => CheckUpdateResult::NoUpdate,
                            };

                            let bytes = common::postcard::to_allocvec(&result).unwrap();
                            socket.write_all(&bytes).await.unwrap();
                        }
                        Some(ClientCommand::RequestUpdate(id)) => {
                            let guard = messages.lock().await;
                            let message =
                                guard.get_message(id).expect("Requested message not found.");
                            match &message.content {
                                MessageContent::Text(text) => {
                                    socket.write_all(text.as_bytes()).await.unwrap();
                                }
                                MessageContent::Image(image) => {
                                    socket.write_all(image.as_slice()).await.unwrap();
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

async fn parse_client_command(socket: &mut TcpStream) -> Option<ClientCommand> {
    let mut command_buf = [0u8; ClientCommand::POSTCARD_MAX_SIZE];
    socket.read_exact(&mut command_buf).await.ok()?;
    common::postcard::from_bytes(&command_buf).ok()
}
