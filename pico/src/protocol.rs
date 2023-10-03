//! Definition of the protocol used to communicate messages between server and client.

use cyw43::Control;
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_time::Duration;
use embedded_io_async::{Read, Write};
use rpi_messages_common::{ClientCommand, MessageUpdate, UpdateResult, IMAGE_BUFFER_SIZE};

use crate::error::Error;
use crate::static_data::{device_id, server_endpoint};
use crate::Result;

const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);

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
    ) -> Result<Protocol<'a>> {
        // SAFETY - we only use RX_BUFFER here. We set it as static to keep it in the .data section. TODO might not be necessary but iirc I had problems when it was on the stack, i.e. in the future.
        let mut socket = unsafe { TcpSocket::new(stack, &mut RX_BUFFER, tx_buffer) };
        socket.set_timeout(Some(SOCKET_TIMEOUT));

        // TODO what does setting the gpio here do?
        control.gpio_set(1, false).await;
        let server_endpoint = server_endpoint();
        log::info!("Connecting to server: {}", server_endpoint);
        let connect_result = socket
            .connect(server_endpoint)
            .await
            .map_err(|e| Error::ServerConnect(e));
        control.gpio_set(0, true).await;

        connect_result.and(Ok(Self { socket }))
    }

    pub async fn check_update(&mut self) -> Result<UpdateResult> {
        let command_buf = ClientCommand::CheckUpdate(device_id())
            .serialize()
            .map_err(|e| Error::Serialize(e))?;
        self.socket.write_all(&command_buf).await.map_err(|_| Error::Socket)?;

        let mut reply_buf = [0u8; UpdateResult::SERIALIZED_LEN];
        self.socket
            .read_exact(&mut reply_buf)
            .await
            .map_err(|_| Error::Socket)?;

        UpdateResult::deserialize(&reply_buf).map_err(|e| Error::Serialize(e))
    }

    pub async fn request_update(&mut self, update: &MessageUpdate, message_buf: &mut [u8]) -> Result<()> {
        assert!(message_buf.len() >= update.kind.size());
        assert!(message_buf.len() > 0);
        assert!(update.kind.size() > 0);

        let command_buf = ClientCommand::RequestUpdate(update.uuid)
            .serialize()
            .map_err(|e| Error::Serialize(e))?;
        self.socket.write_all(&command_buf).await.map_err(|_| Error::Socket)?;

        self.socket
            .read_exact(&mut message_buf[..update.kind.size()])
            .await
            .map_err(|_| Error::Socket)?;
        Ok(())
    }
}
