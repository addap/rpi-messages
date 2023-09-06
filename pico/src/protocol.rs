//! Definition of the protocol used to communicate messages between server and client.

use core::mem;

use cyw43::Control;
use embassy_net::tcp::{ConnectError, TcpSocket};
use embassy_net::{IpAddress, IpEndpoint, Stack};
use embassy_time::Duration;
use postcard::ser_flavors::Size;
use postcard::serialize_with_flavor;
use serde::{Deserialize, Serialize};

use crate::messagebuf::IMAGE_BUFFER_SIZE;

const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);
const SERVER_ENDPOINT: IpEndpoint = IpEndpoint::new(IpAddress::v4(192, 168, 12, 1), 1337);

const USIZE: usize = mem::size_of::<usize>();

// rx_buffer must be large enough to hold a whole image, or alternatively we do streaming.
static mut RX_BUFFER: [u8; IMAGE_BUFFER_SIZE] = [0; IMAGE_BUFFER_SIZE];

pub struct Protocol<'a> {
    socket: TcpSocket<'a>,
}

impl<'a> Protocol<'a> {
    pub async fn new(
        stack: &'a Stack<cyw43::NetDriver<'static>>,
        control: &'a mut Control<'static>,
        tx_buffer: &'a mut [u8],
    ) -> Result<Protocol<'a>, ConnectError> {
        // SAFETY - we only use RX_BUFFER here. We set it as static to keep it in the .data section. TODO might not be necessary but iirc I had problems when it was on the stack, i.e. in the future.
        let mut socket = unsafe { TcpSocket::new(stack, &mut RX_BUFFER, tx_buffer) };
        socket.set_timeout(Some(SOCKET_TIMEOUT));

        // TODO what does setting the gpio here do?
        control.gpio_set(1, false).await;
        log::info!("Connecting to server: {}", SERVER_ENDPOINT);
        let connect_result = socket.connect(SERVER_ENDPOINT).await;
        control.gpio_set(0, true).await;

        connect_result.and(Ok(Self { socket }))
    }

    pub async fn check_update(&mut self) -> Option<MessageUpdate> {
        let mut command_buf = [9u8; 256];
        let command_buf = ClientCommand::CheckUpdate.prepare_command(&mut command_buf);

        self.socket.write(&command_buf).await.unwrap();

        let mut update_size = [0u8; USIZE];
        self.socket.read(&mut update_size).await.unwrap();
        let update_size = usize::from_be_bytes(update_size);
        let mut buf = [0u8; mem::size_of::<Option<MessageUpdate>>()];
        self.socket.read(&mut buf[..update_size]).await.unwrap();

        postcard::from_bytes(&buf).unwrap()
    }

    pub async fn request_update(&mut self, update: &MessageUpdate, message_buf: &mut [u8]) {
        let mut command_buf = [9u8; 256];
        let command_buf = ClientCommand::RequestUpdate(update.uuid).prepare_command(&mut command_buf);

        self.socket.write(&command_buf).await.unwrap();
        self.socket.read(message_buf).await.unwrap();
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum MessageUpdateKind {
    Text(usize),
    Image,
}

#[derive(Serialize, Deserialize)]
pub struct MessageUpdate {
    pub lifetime_sec: u64,
    pub kind: MessageUpdateKind,
    uuid: u64,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum ClientCommand {
    CheckUpdate,
    RequestUpdate(u64),
}

impl ClientCommand {
    fn prepare_command(self, command_buf: &mut [u8]) -> &mut [u8] {
        let command_size = serialize_with_flavor(&self, Size::default()).unwrap();
        debug_assert!(command_size < mem::size_of::<ClientCommand>());

        command_buf[..USIZE].copy_from_slice(&command_size.to_be_bytes());
        postcard::to_slice(&self, command_buf).unwrap()
    }
}
