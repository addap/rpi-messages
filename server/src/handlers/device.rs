use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use anyhow::Context;
use common::protocols::pico::serialization::SerDe;
use common::protocols::pico::{CheckUpdateResult, ClientCommand, Update, UpdateKind};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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
    'client: loop {
        match parse_client_command(&mut socket).await {
            Err(e) => {
                log::error!("{e}");
                break 'client;
            }
            Ok(ClientCommand::CheckUpdate(device_id, after)) => {
                log::info!("CheckUpdate acquiring lock.");
                let guard = messages.lock().await;
                let result = match guard.get_next_message(device_id, after) {
                    Some(message) => {
                        let message_update = Update {
                            lifetime_sec: message.meta.duration.num_seconds() as u32,
                            id: message.id,
                            kind: UpdateKind::from(&message.content),
                        };
                        CheckUpdateResult::Update(message_update)
                    }
                    None => CheckUpdateResult::NoUpdate,
                };

                log::debug!("CheckUpdate result {result:?}");
                let buf = result.to_bytes_alloc().unwrap();
                log::debug!("CheckUpdate buf {buf:?}");
                socket.write_all(&buf).await.unwrap();
                match result {
                    CheckUpdateResult::NoUpdate => {
                        socket.flush().await.ok();
                        break 'client;
                    }
                    _ => {}
                }
            }
            Ok(ClientCommand::RequestUpdate(id)) => {
                log::info!("RequestUpdate acquiring lock.");
                let guard = messages.lock().await;
                let message = guard.get_message(id).expect("Requested message not found.");
                match &message.content {
                    MessageContent::Text(text) => {
                        socket.write_all(text.as_bytes()).await.unwrap();
                    }
                    MessageContent::Image { rgb565, .. } => {
                        socket.write_all(rgb565.as_slice()).await.unwrap();
                    }
                }
            }
        }
    }
}

async fn parse_client_command(socket: &mut TcpStream) -> Result<ClientCommand, anyhow::Error> {
    let mut command_buf = [0u8; ClientCommand::BUFFER_SIZE];
    socket.read_exact(&mut command_buf).await?;
    let result = ClientCommand::from_bytes(&command_buf);

    log::info!(
        "Received ClientCommand buf {command_buf:?}. Parsed...{}",
        match result {
            Ok(_) => "ok",
            Err(_) => "failed",
        }
    );
    result.context("Parsing failed")
}
