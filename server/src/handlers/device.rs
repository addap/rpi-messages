use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use common::protocols::pico::serialization::Transmission;
use common::protocols::pico::{ClientCommand, RequestUpdateResult, Update, UpdateKind};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::message::{MessageContent, Messages};

const ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 1338);

pub async fn run(messages: Arc<Mutex<Messages>>) {
    log::info!("Listening for TCP connections from device at {ADDRESS}.");
    let listener = TcpListener::bind(ADDRESS).await.unwrap();

    loop {
        println!("Listening for client connections.");
        match listener.accept().await {
            Ok((socket, addr)) => {
                println!("new client at {:?}", addr);
                handle_client(socket, &messages).await
            }
            Err(e) => println!("couldn't get client: {e:?}"),
        }
    }
}

async fn handle_client(mut socket: TcpStream, messages: &Mutex<Messages>) {
    loop {
        match ClientCommand::receive_alloc(&mut socket).await {
            Err(e) => {
                log::error!("{e}");
                break;
            }
            Ok(ClientCommand::RequestUpdate(device_id, after)) => {
                log::trace!("RequestUpdate acquiring lock.");

                let guard = messages.lock().await;
                match guard.get_next_message(device_id, after) {
                    Some(message) => {
                        let message_update = Update {
                            lifetime_sec: message.meta.duration.num_seconds() as u32,
                            id: message.id,
                            kind: UpdateKind::from(&message.content),
                        };
                        let result = RequestUpdateResult::Update(message_update);
                        result.send_alloc(&mut socket).await.unwrap();

                        match &message.content {
                            MessageContent::Text(text) => {
                                socket.write_all(text.text().as_bytes()).await.unwrap();
                            }
                            MessageContent::Image(image) => {
                                socket.write_all(image.rgb565()).await.unwrap();
                            }
                        }
                    }
                    None => {
                        let result = RequestUpdateResult::NoUpdate;
                        result.send_alloc(&mut socket).await.unwrap();
                        socket.flush().await.ok();
                        break;
                    }
                };
            }
        }
    }
}
