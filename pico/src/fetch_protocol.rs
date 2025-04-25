//! Definition of the protocol used to communicate messages between server and client.

use common::{
    consts::IMAGE_BUFFER_SIZE,
    protocols::pico::{serialization::SerDe, CheckUpdateResult, ClientCommand, Update},
    types::UpdateID,
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
const TX_BUFFER_SIZE: usize = 256;

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

    pub async fn abort(mut self) {
        self.socket.close();
        self.socket.flush().await.ok();
    }

    pub async fn check_update(&mut self, after: Option<UpdateID>) -> Result<CheckUpdateResult> {
        let command = ClientCommand::CheckUpdate(device_id(), after);

        let mut command_buf = [0u8; ClientCommand::BUFFER_SIZE];
        let command_buf = command.to_bytes(&mut command_buf)?;

        log::info!("Check Update command buf {command_buf:?}");

        self.socket.write_all(&command_buf).await.map_err(|_| Error::Socket)?;

        let mut reply_buf = [0u8; CheckUpdateResult::BUFFER_SIZE];
        self.socket
            .read_exact(&mut reply_buf)
            .await
            .map_err(|_| Error::Socket)?;

        log::info!("CheckUpdateResult buf {reply_buf:?}");

        let result = CheckUpdateResult::from_bytes(&reply_buf)?;
        let valid = result
            .check_valid()
            .map_err(|e| Error::ServerMessage(ServerMessageError::Protocol(e)));

        log::info!("CheckUpdateResult {result:?}");
        valid.and(Ok(result))
    }

    pub async fn request_update(&mut self, update: &Update, message_buf: &mut [u8]) -> Result<()> {
        assert!(
            message_buf.len() == update.kind.size(),
            "Message buf length is {} <> {}, for update kind {:?}",
            message_buf.len(),
            update.kind.size(),
            update.kind
        );

        let command = ClientCommand::RequestUpdate(update.id);
        // a.d. TODO try to use MaybeUninit
        let mut command_buf = [0u8; ClientCommand::BUFFER_SIZE];
        let command_buf = command.to_bytes(&mut command_buf)?;
        log::info!("Request Update command buf {command_buf:?}");

        self.socket.write_all(&command_buf).await.map_err(|_| Error::Socket)?;
        self.socket.read_exact(message_buf).await.map_err(|_| Error::Socket)?;
        Ok(())
    }
}
