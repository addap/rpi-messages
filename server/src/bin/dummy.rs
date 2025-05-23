use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use common::types::UpdateID;
use common::{
    consts::IMAGE_BUFFER_SIZE,
    protocols::pico::{serialization::SerDe, CheckUpdateResult, ClientCommand, Update, UpdateKind},
};

static IMO: &'static [u8; IMAGE_BUFFER_SIZE] = include_bytes!("../../pictures/love.bin");
static BG: &'static [u8; IMAGE_BUFFER_SIZE] = include_bytes!("../../pictures/journey.bin");
static TEXT1: &'static str = "Happy Valentine's Day!";
static TEXT2: &'static str = "Did you drink enough water today?";

fn main() {
    let listener = TcpListener::bind("0.0.0.0:1338").unwrap();

    loop {
        println!("Listening for new connections.");
        match listener.accept() {
            Ok((mut socket, addr)) => {
                println!("new client at {:?}", addr);
                let mut stage = 0;

                while let Some(command) = parse_client_command(&mut socket) {
                    match command {
                        ClientCommand::CheckUpdate(device_id, _) => {
                            let result = match stage {
                                0 => {
                                    println!("Got check for update. Sending text 1.");
                                    // if device_id == 0 {
                                    CheckUpdateResult::Update(Update {
                                        lifetime_sec: 60 * 100,
                                        kind: UpdateKind::Text(TEXT1.len() as u32),
                                        id: UpdateID(0),
                                    })
                                    // } else {
                                }
                                1 => {
                                    println!("Got check for update. Sending image 1.");
                                    // if device_id == 0 {
                                    CheckUpdateResult::Update(Update {
                                        lifetime_sec: 60 * 100,
                                        kind: UpdateKind::Image,
                                        id: UpdateID(1),
                                    })
                                }
                                2 => {
                                    println!("Got check for update. Sending text 2.");
                                    CheckUpdateResult::Update(Update {
                                        lifetime_sec: 60 * 100,
                                        kind: UpdateKind::Text(TEXT2.len() as u32),
                                        id: UpdateID(2),
                                    })
                                }
                                // } else {
                                3 => {
                                    println!("Got check for update. Sending image 1.");
                                    CheckUpdateResult::Update(Update {
                                        lifetime_sec: 60 * 100,
                                        kind: UpdateKind::Image,
                                        id: UpdateID(3),
                                    })
                                }
                                _ => {
                                    println!("Got check for update. Sending nothing.");
                                    CheckUpdateResult::NoUpdate
                                }
                            };

                            let buf = result.to_bytes_alloc().unwrap();
                            socket.write_all(&buf).unwrap();

                            stage += 1;
                        }
                        ClientCommand::RequestUpdate(id) => match id {
                            UpdateID(0) => {
                                println!("Got request for update text.");
                                socket.write_all(TEXT1.as_bytes()).unwrap()
                            }
                            UpdateID(1) => {
                                println!("Got request for update image.");
                                socket.write_all(IMO).unwrap()
                            }
                            UpdateID(2) => {
                                println!("Got request for update text.");
                                socket.write_all(TEXT2.as_bytes()).unwrap()
                            }
                            UpdateID(3) => {
                                println!("Got request for update image.");
                                socket.write_all(BG).unwrap()
                            }
                            _ => {
                                println!("Got unknown request for update.");
                            }
                        },
                    }
                }
            }
            Err(e) => println!("couldn't get client: {e:?}"),
        }
    }
}

fn parse_client_command(socket: &mut TcpStream) -> Option<ClientCommand> {
    let mut command_buf = [0u8; ClientCommand::BUFFER_SIZE];
    socket.read_exact(&mut command_buf).ok()?;
    ClientCommand::from_bytes(&command_buf).ok()
}
