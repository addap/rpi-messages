use rpi_messages_common::{ClientCommand, MessageUpdate, MessageUpdateKind, UpdateResult};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

fn main() {
    let listener = TcpListener::bind("0.0.0.0:1337").unwrap();

    loop {
        println!("Listening for new connections.");
        match listener.accept() {
            Ok((mut socket, addr)) => {
                println!("new client at {:?}", addr);

                let mut i = 0;
                let text = "Hello There";
                while let Some(command) = parse_client_command(&mut socket) {
                    match command {
                        ClientCommand::CheckUpdate => {
                            let result = if i == 0 {
                                println!("Got check for update. Sending text.");
                                UpdateResult::Update(MessageUpdate {
                                    lifetime_sec: 60 * 60,
                                    kind: MessageUpdateKind::Text(text.len() as u32),
                                    uuid: 0,
                                })
                            } else {
                                println!("Got check for update. Sending nothing.");
                                UpdateResult::NoUpdate
                            };

                            let bytes = result.serialize().unwrap();
                            socket.write_all(&bytes).unwrap();

                            i += 1;
                        }
                        ClientCommand::RequestUpdate(_) => {
                            println!("Got request for update.");
                            socket.write_all(text.as_bytes()).unwrap()
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
    ClientCommand::deserialize(&command_buf)
}
