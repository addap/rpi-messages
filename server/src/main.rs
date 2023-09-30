use rpi_messages_common::{
    ClientCommand, MessageUpdate, MessageUpdateKind, UpdateResult, IMAGE_BUFFER_SIZE,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

static IMO: &'static [u8; IMAGE_BUFFER_SIZE] = include_bytes!("../../../pictures/loveimo.bin");
static TEXT: &'static str = "Hello There";

fn main() {
    let listener = TcpListener::bind("0.0.0.0:1337").unwrap();

    let mut stage = 0;
    loop {
        println!("Listening for new connections.");
        match listener.accept() {
            Ok((mut socket, addr)) => {
                println!("new client at {:?}", addr);

                while let Some(command) = parse_client_command(&mut socket) {
                    match command {
                        ClientCommand::CheckUpdate => {
                            let result = match stage {
                                0 => {
                                    println!("Got check for update. Sending text.");
                                    UpdateResult::Update(MessageUpdate {
                                        lifetime_sec: 60 * 100,
                                        kind: MessageUpdateKind::Text(TEXT.len() as u32),
                                        uuid: 0,
                                    })
                                }
                                1 => {
                                    println!("Got check for update. Sending image.");
                                    UpdateResult::Update(MessageUpdate {
                                        lifetime_sec: 60 * 100,
                                        kind: MessageUpdateKind::Image,
                                        uuid: 1,
                                    })
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
                                socket.write_all(TEXT.as_bytes()).unwrap()
                            }
                            1 => {
                                println!("Got request for update image.");
                                socket.write_all(IMO).unwrap()
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
    ClientCommand::deserialize(&command_buf)
}
