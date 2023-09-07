//! Definition of the protocol used to communicate messages between server and client.

use core::mem;

use cyw43::Control;
use embassy_net::tcp::{ConnectError, TcpSocket};
use embassy_net::{IpAddress, IpEndpoint, Stack};
use embassy_time::Duration;
use postcard::ser_flavors::Size;
use postcard::serialize_with_flavor;
use rpi_messages_common::{ClientCommand, MessageUpdate, IMAGE_BUFFER_SIZE};

const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);
const SERVER_ENDPOINT: IpEndpoint = IpEndpoint::new(IpAddress::v4(192, 168, 12, 1), 1337);
const USIZE: usize = mem::size_of::<usize>();
const MESSAGE_UPDATE_SIZE: usize = mem::size_of::<Option<MessageUpdate>>();
const COMMAND_BUF_SIZE: usize = 32;

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
        let mut command_buf = [0u8; COMMAND_BUF_SIZE];
        let command_buf = serialize_client_command(&ClientCommand::CheckUpdate, &mut command_buf);

        self.socket.write(&command_buf).await.unwrap();

        // a.d. TODO I wanted to abstract the parsing of a server reply out to a function with a T : Deserialize + ?Sized, but mem::size_of apparently does not work with generic types.
        let mut reply_size = [0u8; USIZE];
        self.socket.read(&mut reply_size).await.unwrap();
        let reply_size = usize::from_be_bytes(reply_size);
        debug_assert!(reply_size <= MESSAGE_UPDATE_SIZE);
        let mut buf = [0u8; MESSAGE_UPDATE_SIZE];
        self.socket.read(&mut buf[..reply_size]).await.unwrap();

        postcard::from_bytes(&buf).unwrap()
    }

    pub async fn request_update(&mut self, update: &MessageUpdate, message_buf: &mut [u8]) {
        debug_assert!(message_buf.len() >= update.kind.size());

        let mut command_buf = [0u8; COMMAND_BUF_SIZE];
        let command_buf = serialize_client_command(&ClientCommand::RequestUpdate(update.uuid), &mut command_buf);

        self.socket.write(&command_buf).await.unwrap();
        self.socket.read(message_buf).await.unwrap();
    }
}

fn serialize_client_command<'a>(command: &ClientCommand, command_buf: &'a mut [u8; COMMAND_BUF_SIZE]) -> &'a [u8] {
    let command_size = serialize_with_flavor(command, Size::default()).unwrap();
    debug_assert!(command_size < mem::size_of::<ClientCommand>());
    debug_assert!(command_size + USIZE < COMMAND_BUF_SIZE);

    command_buf[..USIZE].copy_from_slice(&command_size.to_be_bytes());
    postcard::to_slice(command, &mut command_buf[USIZE..]).unwrap()
}
