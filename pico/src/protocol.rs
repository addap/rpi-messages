//! Definition of the protocol used to communicate messages between server and client.

use common::{
    consts::IMAGE_BUFFER_SIZE,
    postcard::{self, experimental::max_size::MaxSize},
    protocol::{CheckUpdateResult, ClientCommand, Update, UpdateID},
};
use cyw43::Control;
use embassy_net::tcp::TcpSocket;
use embassy_time::Duration;
use embedded_io_async::{Read, Write};

use crate::error::{Error, ServerMessageError};
use crate::static_data::{device_id, server_endpoint};
use crate::Result;

// a.d. TODO we could treat all of the consts like in the static_data module to make it configurable.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);

// rx_buffer must be large enough to hold a whole image, or alternatively we do streaming.
static mut RX_BUFFER: [u8; IMAGE_BUFFER_SIZE] = [0; IMAGE_BUFFER_SIZE];

pub struct Protocol<'a> {
    socket: TcpSocket<'a>,
}

impl<'a> Protocol<'a> {
    pub async fn new(
        stack: embassy_net::Stack<'static>,
        control: &'a mut Control<'static>,
        tx_buffer: &'a mut [u8],
    ) -> Result<Protocol<'a>> {
        // SAFETY - we only use RX_BUFFER here. We set it as static to keep it in the .data section. TODO might not be necessary but iirc I had problems when it was on the stack, i.e. in the future.
        let mut socket = unsafe { TcpSocket::new(stack, &mut RX_BUFFER, tx_buffer) };
        socket.set_timeout(Some(SOCKET_TIMEOUT));

        // TODO what does setting the gpio here do?
        control.gpio_set(1, false).await;
        let server_endpoint = server_endpoint();
        log::info!("Connecting to server: {}", server_endpoint);
        let connected = socket
            .connect(server_endpoint)
            .await
            .map_err(|e| Error::ServerConnect(e));
        control.gpio_set(0, true).await;

        connected.and(Ok(Self { socket }))
    }

    pub async fn check_update(&mut self, after: Option<UpdateID>) -> Result<CheckUpdateResult> {
        let command = ClientCommand::CheckUpdate(device_id(), after);
        // TODO make static buffer
        let mut command_buf = [0u8; ClientCommand::POSTCARD_MAX_SIZE];
        postcard::to_slice(&command, &mut command_buf)?;

        self.socket.write(&command_buf).await.map_err(|_| Error::Socket)?;

        let mut reply_buf = [0u8; CheckUpdateResult::POSTCARD_MAX_SIZE];
        self.socket
            .read_exact(&mut reply_buf)
            .await
            .map_err(|_| Error::Socket)?;

        let result: CheckUpdateResult = postcard::from_bytes(&reply_buf)?;
        let valid = result
            .check_valid()
            .map_err(|e| Error::ServerMessage(ServerMessageError::Format(e)));

        valid.and(Ok(result))
    }

    pub async fn request_update(&mut self, update: &Update, message_buf: &mut [u8]) -> Result<()> {
        assert!(message_buf.len() >= update.kind.size());

        let command = ClientCommand::RequestUpdate(update.id);
        // a.d. TODO try to use MaybeUninit
        let mut command_buf = [0u8; ClientCommand::POSTCARD_MAX_SIZE];
        postcard::to_slice(&command, &mut command_buf)?;

        self.socket.write_all(&command_buf).await.map_err(|_| Error::Socket)?;

        self.socket
            .read_exact(&mut message_buf[..update.kind.size()])
            .await
            .map_err(|_| Error::Socket)?;
        Ok(())
    }
}
