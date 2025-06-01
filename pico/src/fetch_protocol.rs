//! Definition of the protocol used to communicate messages between server and client.

use common::{
    consts::IMAGE_BUFFER_SIZE,
    protocols::pico::{serialization::Transmission, ClientCommand, RequestUpdateResult, Update, UpdateKind},
    types::MessageID,
};
use cyw43::Control;
use embassy_net::tcp::TcpSocket;
use embassy_time::Duration;
use embedded_io_async::Read;

use crate::{
    error::{Error, ServerMessageError},
    messagebuf::Messages,
    static_data::{device_id, server_endpoint},
    Result, MESSAGES,
};

// a.d. TODO we could treat all of the consts like in the static_data module to make it configurable.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);
const TX_BUFFER_SIZE: usize = 256;

/// a.d. TODO document
mod internal {
    pub struct State {
        _private: (),
    }

    impl State {
        unsafe fn steal() -> Self {
            Self { _private: () }
        }

        fn take_with_cs(_cs: critical_section::CriticalSection) -> Self {
            static mut _FETCH_PROTOCOL_STATE: bool = false;

            unsafe {
                if _FETCH_PROTOCOL_STATE {
                    panic!("fetch_protocol::State::init called more than once!")
                }
                _FETCH_PROTOCOL_STATE = true;
                Self::steal()
            }
        }

        pub fn take() -> Self {
            critical_section::with(Self::take_with_cs)
        }
    }
}

pub use internal::State;

pub struct Socket<'a> {
    #[allow(unused)]
    state: &'a mut State,
    socket: TcpSocket<'static>,
}

impl<'a> Socket<'a> {
    pub async fn new(
        state: &'a mut State,
        stack: embassy_net::Stack<'static>,
        control: &mut Control<'static>,
    ) -> Result<Self> {
        static mut RX_BUFFER: [u8; IMAGE_BUFFER_SIZE] = [0; IMAGE_BUFFER_SIZE];
        static mut TX_BUFFER: [u8; TX_BUFFER_SIZE] = [0; TX_BUFFER_SIZE];

        // SAFETY - TODO
        let mut socket = unsafe {
            #[allow(static_mut_refs)]
            TcpSocket::new(stack, &mut RX_BUFFER, &mut TX_BUFFER)
        };
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

        connected.and(Ok(Self { state, socket }))
    }

    pub async fn close(mut self) {
        self.socket.close();
        self.socket.flush().await.ok();
    }

    pub async fn request_update(&mut self, after: Option<MessageID>) -> Result<RequestUpdateResult> {
        let command = ClientCommand::RequestUpdate(device_id(), after);

        let mut command_buf = [0u8; ClientCommand::BUFFER_SIZE];
        command.send(&mut command_buf, &mut self.socket).await?;

        let mut reply_buf = [0u8; RequestUpdateResult::BUFFER_SIZE];
        let result = RequestUpdateResult::receive(&mut reply_buf, &mut self.socket).await?;
        let valid = result
            .check_valid()
            .map_err(|e| Error::ServerMessage(ServerMessageError::Protocol(e)));

        log::info!("CheckUpdateResult {result:?}");
        valid.and(Ok(result))
    }

    pub async fn receive_payload(&mut self, update: &Update, payload_buf: &mut [u8]) -> Result<()> {
        assert!(
            payload_buf.len() == update.kind.size(),
            "Payload buf length is {} <> {}, for update kind {:?}",
            payload_buf.len(),
            update.kind.size(),
            update.kind
        );

        self.socket.read_exact(payload_buf).await.map_err(|_| Error::Socket)?;
        Ok(())
    }

    pub async fn handle_update(&mut self, update: Update) -> Result<()> {
        log::info!("Received an update. Acquiring mutex to change message buffer.");
        let mut guard = MESSAGES.lock().await;
        let messages: &mut Messages = &mut guard;

        match update.kind {
            UpdateKind::Text(text_len) => {
                log::info!("Requesting text update.");
                let message = messages.next_available_text();
                message.update_meta(&update);

                // SAFETY - We read the bytes from the network into message.data.text.
                // If that fails (in which case the buffer could be half-filled) or if the buffer does not contain valid UTF-8 in the end, we clear the string.
                // We are holding the message lock so no one else can access the the unsafe buffer contents while this future may be paused.
                unsafe {
                    let message_buf = message.data.text.as_mut_vec();
                    // a.d. TODO cannot use this since elements were not initialized
                    // how about creating the slice directly
                    // but calling read with maybeuninit data is potentially UB so we should just initialize all strings in the beginning. Then we can also use set_len.
                    message_buf.set_len(text_len as usize);
                    if let Err(e) = self.receive_payload(&update, message_buf).await {
                        message_buf.clear();
                        return Err(e);
                    }

                    match core::str::from_utf8(message_buf) {
                        Ok(text) => {
                            log::info!("Received text update: {}", text);
                        }
                        Err(e) => {
                            message_buf.clear();
                            return Err(Error::ServerMessage(ServerMessageError::Encoding(e)));
                        }
                    }
                }
            }
            UpdateKind::Image => {
                log::info!("Requesting image update.");
                let message = messages.next_available_image();
                message.update_meta(&update);
                let payload_buf = message.data.image.as_mut();
                self.receive_payload(&update, payload_buf).await?;
            }
        };

        Ok(())
    }
}
