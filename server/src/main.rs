use postcard::Result;
use rpi_messages_common::{
    ClientCommand, MessageUpdate, MessageUpdateKind, UpdateResult, IMAGE_BUFFER_SIZE,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

static IMO: &'static [u8; IMAGE_BUFFER_SIZE] = include_bytes!("../../../pictures/loveimo.bin");
static BG: &'static [u8; IMAGE_BUFFER_SIZE] = include_bytes!("../../../pictures/baldurs_gate.bin");
static TEXT1: &'static str = "Hope you have a good day today.";
static TEXT2: &'static str = "Take care of yourself. Drink enough water.";

fn main() {
    let listener = TcpListener::bind("0.0.0.0:1337").unwrap();

    loop {
        println!("Listening for new connections.");
        match listener.accept() {
            Ok((mut socket, addr)) => {
                println!("new client at {:?}", addr);
                let mut stage = 0;

                while let Some(command) = parse_client_command(&mut socket) {
                    match command {
                        ClientCommand::CheckUpdate(device_id) => {
                            let result = match stage {
                                0 => {
                                    println!("Got check for update. Sending text.");
                                    if device_id == 0 {
                                        UpdateResult::Update(MessageUpdate {
                                            lifetime_sec: 60 * 100,
                                            kind: MessageUpdateKind::Text(TEXT1.len() as u32),
                                            uuid: 0,
                                        })
                                    } else {
                                        UpdateResult::Update(MessageUpdate {
                                            lifetime_sec: 60 * 100,
                                            kind: MessageUpdateKind::Text(TEXT2.len() as u32),
                                            uuid: 2,
                                        })
                                    }
                                }
                                1 => {
                                    println!("Got check for update. Sending image.");
                                    if device_id == 0 {
                                        UpdateResult::Update(MessageUpdate {
                                            lifetime_sec: 60 * 100,
                                            kind: MessageUpdateKind::Image,
                                            uuid: 1,
                                        })
                                    } else {
                                        UpdateResult::Update(MessageUpdate {
                                            lifetime_sec: 60 * 100,
                                            kind: MessageUpdateKind::Image,
                                            uuid: 3,
                                        })
                                    }
                                }
                                _ => {
                                    println!("Got check for update. Sending nothing.");
                                    UpdateResult::NoUpdate
                                }
                            };

                            let bytes = result.serialize().unwrap();
                            socket.write_all(&bytes).unwrap();

                            stage += 1;
                        }
                        ClientCommand::RequestUpdate(uuid) => match uuid {
                            0 => {
                                println!("Got request for update text.");
                                socket.write_all(TEXT1.as_bytes()).unwrap()
                            }
                            1 => {
                                println!("Got request for update image.");
                                socket.write_all(IMO).unwrap()
                            }
                            2 => {
                                println!("Got request for update text.");
                                socket.write_all(TEXT2.as_bytes()).unwrap()
                            }
                            3 => {
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
    let mut command_buf = [0u8; ClientCommand::SERIALIZED_LEN];
    socket.read_exact(&mut command_buf).ok()?;
    ClientCommand::deserialize(&command_buf).ok()
}
