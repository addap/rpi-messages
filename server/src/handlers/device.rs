use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use common::protocols::pico::serialization::Transmission;
use common::protocols::pico::{ClientCommand, RequestUpdateResult, Update, UpdateKind};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};

use crate::message::MessageContent;
use crate::message_db::Db;

const ADDRESS: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 1338);

pub async fn run(messages: Arc<dyn Db>) {
    log::info!("Listening for TCP connections from device at {ADDRESS}.");
    let listener = TcpListener::bind(ADDRESS).await.unwrap();

    loop {
        log::info!("Listening for client connections.");
        match listener.accept().await {
            Ok((socket, addr)) => {
                log::info!("new client at {:?}", addr);
                // a.d. TODO collect join handles and clean up?
                tokio::spawn({
                    let messages = messages.clone();
                    // note: need async move block so that messages is not dropped too early. the block will own the messages object.
                    async move { handle_client(socket, &*messages).await }
                });
            }
            Err(e) => log::error!("couldn't get client: {e:?}"),
        }
    }
}

// a.d. TODO I'm not sure I want a Sync here => read the async book
async fn handle_client(mut socket: TcpStream, messages: &dyn Db) {
    loop {
        match ClientCommand::receive_alloc(&mut socket).await {
            Err(e) => {
                log::error!("{e}");
                break;
            }
            Ok(ClientCommand::RequestUpdate(device_id, after)) => {
                log::trace!("RequestUpdate acquiring lock.");

                match messages.get_next_message(device_id, after).await {
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
