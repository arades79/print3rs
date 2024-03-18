use std::{fmt::Debug, future::Future, marker::PhantomData};

use serde::Serialize;
use winnow::Parser;

mod response;

use response::response;
pub use response::Response;

use print3rs_serializer::{serialize_unsequenced, Sequenced};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::{broadcast, mpsc},
};

use sealed::sealed;

use bytes::{Bytes, BytesMut};

pub type LineStream = broadcast::Receiver<Bytes>;

trait Task: Future {
    fn cancel(&mut self);
}

trait Spawner {
    type Transform<T>;
    fn spawn<F: Future + Send + 'static>(
        &self,
        fut: F,
    ) -> impl Task<Output = Self::Transform<F::Output>>
    where
        F::Output: Send + 'static;
}

impl<T> Task for tokio::task::JoinHandle<T> {
    fn cancel(&mut self) {
        self.abort()
    }
}

impl Spawner for tokio::runtime::Handle {
    type Transform<T> = Result<T, tokio::task::JoinError>;
    fn spawn<F: Future + Send + 'static>(
        &self,
        fut: F,
    ) -> impl Task<Output = Self::Transform<F::Output>>
    where
        F::Output: Send + 'static,
    {
        self.spawn(fut)
    }
}

type GlobalSpawner = tokio::runtime::Handle;

fn default_spawner() -> GlobalSpawner
where
    GlobalSpawner: Spawner,
{
    tokio::runtime::Handle::current()
}

impl<T> Debug for Box<dyn Task<Output = T>>
where
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Box").finish()
    }
}

#[sealed]
#[allow(async_fn_in_trait)]
pub trait AsyncPrinterComm {
    /// Serialize a struct implementing Serialize and send the bytes to the printer
    ///
    /// Sent bytes will include a sequence number and checksum.
    /// For printers which support advanced OK messages this will allow TCP like checked communication.
    ///
    /// When called, a local task is spawned to check for a matching OK message.
    /// The handle to this task is returned after the first await on success.
    /// This allows simple synchronization of any sent command by awaiting twice.
    async fn send(
        &self,
        gcode: impl Serialize + Debug,
    ) -> Result<tokio::task::JoinHandle<Response>, Error>;

    /// Serialize anything implementing Serialize and send the bytes to the printer
    ///
    /// There is no guarantee that a command is correctly recieved or serviced;
    /// any synchronization based on responses will have to be done manually.
    ///
    /// If your printer supports it, the sequenced `send` function is preferred,
    /// although this version is slightly lower overhead.
    fn send_unsequenced(&self, gcode: impl Serialize + Debug) -> Result<(), Error>;

    /// Send any raw sequence of bytes to the printer
    fn send_raw(&self, gcode: &[u8]) -> Result<(), Error>;

    /// Read the next line from the printer
    ///
    /// May not recieve all lines, if calls to this function are spaced
    /// far apart, the buffer may overfill and the oldest messages will
    /// be dropped. In this case the oldest available message is returned.
    async fn read_next_line(&mut self) -> Result<Bytes, DisconnectedError>;

    /// Obtain a broadcast receiver returning all lines received by the printer
    fn subscribe_lines(&self) -> Result<LineStream, DisconnectedError>;
}

pub async fn search_for_sequence(sequence: i32, mut responses: LineStream) -> Response {
    tracing::debug!("Started looking for Ok {sequence}");
    while let Ok(resp) = responses.recv().await {
        match response.parse(&resp) {
            Ok(Response::SequencedOk(seq)) if seq == sequence => {
                tracing::info!("Got Ok for line {seq}");
                return Response::SequencedOk(seq);
            }
            Ok(Response::Resend(seq)) if seq == sequence => {
                tracing::warn!("Printer requested resend for line {seq}");
                return Response::Resend(seq);
            }
            _ => (),
        }
    }
    Response::Ok
}

#[derive(Debug)]
pub struct Socket {
    sender: mpsc::Sender<Box<[u8]>>,
    serializer: Sequenced,
    pub responses: broadcast::Receiver<Bytes>,
}

impl Clone for Socket {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            serializer: self.serializer.clone(),
            responses: self.responses.resubscribe(),
        }
    }
}

#[sealed]
impl AsyncPrinterComm for Socket {
    /// Serialize a struct implementing Serialize and send the bytes to the printer
    ///
    /// Sent bytes will include a sequence number and checksum.
    /// For printers which support advanced OK messages this will allow TCP like checked communication.
    ///
    /// When called, a local task is spawned to check for a matching OK message.
    /// The handle to this task is returned after the first await on success.
    /// This allows simple synchronization of any sent command by awaiting twice.
    #[tracing::instrument(level = "debug", skip(self))]
    async fn send(
        &self,
        gcode: impl Serialize + Debug,
    ) -> Result<tokio::task::JoinHandle<Response>, Error> {
        let send_slot = self.sender.reserve().await?;
        let (sequence, bytes) = self.serializer.serialize(gcode);
        let sequenced_ok_watch = self.subscribe_lines().expect("Socket is always connected");
        send_slot.send(bytes);
        let wait_for_response =
            tokio::task::spawn(search_for_sequence(sequence, sequenced_ok_watch));
        Ok(wait_for_response)
    }

    /// Serialize anything implementing Serialize and send the bytes to the printer
    ///
    /// There is no guarantee that a command is correctly recieved or serviced;
    /// any synchronization based on responses will have to be done manually.
    ///
    /// If your printer supports it, the sequenced `send` function is preferred,
    /// although this version is slightly lower overhead.
    fn send_unsequenced(&self, gcode: impl Serialize + Debug) -> Result<(), Error> {
        let bytes = serialize_unsequenced(gcode);
        self.sender.try_send(bytes)?;
        Ok(())
    }

    /// Send any raw sequence of bytes to the printer
    fn send_raw(&self, gcode: &[u8]) -> Result<(), Error> {
        self.sender.try_send(gcode.to_owned().into_boxed_slice())?;
        Ok(())
    }

    /// Read the next line from the printer
    ///
    /// May not recieve all lines, if calls to this function are spaced
    /// far apart, the buffer may overfill and the oldest messages will
    /// be dropped. In this case the oldest available message is returned.
    async fn read_next_line(&mut self) -> Result<Bytes, DisconnectedError> {
        loop {
            match self.responses.recv().await {
                Ok(line) => break Ok(line),
                Err(broadcast::error::RecvError::Lagged(_)) => todo!(),
                Err(broadcast::error::RecvError::Closed) => {
                    break Err(DisconnectedError::Disconnected)
                }
            }
        }
    }

    /// Obtain a broadcast receiver returning all lines received by the printer
    fn subscribe_lines(&self) -> Result<LineStream, DisconnectedError> {
        Ok(self.responses.resubscribe())
    }
}

type PrinterTask = dyn Task<Output = <GlobalSpawner as Spawner>::Transform<()>>;

/// Handle for asynchronous serial communication with a 3D printer
#[derive(Debug, Default)]
pub enum Printer<Transport> {
    #[default]
    Disconnected,
    Connected {
        socket: Socket,
        com_task: Box<PrinterTask>,
        _transport: PhantomData<Transport>,
    },
}

impl<S> Drop for Printer<S> {
    fn drop(&mut self) {
        if let Self::Connected { com_task, .. } = self {
            com_task.cancel()
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),

    #[error("Background task failed to propagate message from printer\nError message: {0}")]
    ResponseSender(#[from] broadcast::error::SendError<Bytes>),

    #[error("Send queue full or closed")]
    Sender(#[from] tokio::sync::mpsc::error::TrySendError<std::boxed::Box<[u8]>>),

    #[error("Couldn't reserve a slot to send message")]
    SendReserve(#[from] mpsc::error::SendError<()>),

    #[error("Underlying printer connection was closed")]
    Disconnected(#[from] DisconnectedError),
}

#[derive(Debug, thiserror::Error)]
pub enum DisconnectedError {
    #[error("Printer disconnected")]
    Disconnected,
}

/// Loop for handling sending/receiving in the background with possible split senders/receivers
async fn printer_com_task(
    mut transport: impl AsyncRead + AsyncWrite + Unpin,
    mut gcoderx: mpsc::Receiver<Box<[u8]>>,
    responsetx: broadcast::Sender<Bytes>,
) {
    let mut buf = BytesMut::with_capacity(1024);
    tracing::debug!("Started background printer communications");
    loop {
        tokio::select! {
            Some(line) = gcoderx.recv() => {
                if transport.write_all(&line).await.is_err() {return;}
                if transport.flush().await.is_err() {return;}
                tracing::debug!("Sent `{}` to printer", String::from_utf8_lossy(&line).trim());
            },
            Ok(1..) = transport.read_buf(&mut buf) => {
                while let Some(n) = buf.iter().position(|b| *b == b'\n') {
                    let line = buf.split_to(n + 1).freeze();
                    tracing::debug!("Received `{}` from printer", String::from_utf8_lossy(&line).trim());
                    if responsetx.send(line).is_err() {return;}
                }
            },
            else => return,
        }
    }
}

impl<S> Printer<S> {
    /// Create a new printer from a SerialStream.
    ///
    /// Starts a local task to handle printer communication asynchronously
    #[tracing::instrument(level = "debug")]
    pub fn new(port: S) -> Self
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static + Debug,
    {
        let (sender, gcoderx) = mpsc::channel::<Box<[u8]>>(8);
        let (response_sender, responses) = broadcast::channel(64);
        let com_task = default_spawner().spawn(printer_com_task(port, gcoderx, response_sender));
        let com_task = Box::new(com_task);
        let serializer = Sequenced::default();
        Self::Connected {
            socket: Socket {
                sender,
                serializer,
                responses,
            },
            com_task,
            _transport: Default::default(),
        }
    }

    /// Connect to a device
    pub fn connect(&mut self, port: S)
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static + Debug,
    {
        *self = Printer::new(port);
    }

    /// Obtain a cloneable socket handle to talk to printer
    pub fn socket(&self) -> Result<&Socket, DisconnectedError> {
        match self {
            Self::Disconnected => Err(DisconnectedError::Disconnected),
            Self::Connected { socket, .. } => Ok(socket),
        }
    }

    /// Obtain an exclusive socket handle - needed to read
    pub fn socket_mut(&mut self) -> Result<&mut Socket, DisconnectedError> {
        match self {
            Self::Disconnected => Err(DisconnectedError::Disconnected),
            Self::Connected { socket, .. } => Ok(socket),
        }
    }

    /// Disconnect the printer and shutdown background communication
    pub fn disconnect(&mut self) {
        core::mem::take(self);
    }

    pub fn is_connected(&self) -> bool {
        match self {
            Printer::Disconnected => false,
            Printer::Connected { .. } => true,
        }
    }

    pub fn background_task(&self) -> Option<&PrinterTask> {
        match self {
            Printer::Disconnected => None,
            Printer::Connected { com_task, .. } => Some(com_task.as_ref()),
        }
    }
}

#[sealed]
impl<S> AsyncPrinterComm for Printer<S> {
    async fn send(
        &self,
        gcode: impl Serialize + Debug,
    ) -> Result<tokio::task::JoinHandle<Response>, Error> {
        let socket = self.socket()?;
        socket.send(gcode).await
    }

    fn send_unsequenced(&self, gcode: impl Serialize + Debug) -> Result<(), Error> {
        let socket = self.socket()?;
        socket.send_unsequenced(gcode)
    }

    fn send_raw(&self, gcode: &[u8]) -> Result<(), Error> {
        let socket = self.socket()?;
        socket.send_raw(gcode)
    }

    async fn read_next_line(&mut self) -> Result<Bytes, DisconnectedError> {
        let socket = self.socket_mut()?;
        socket
            .read_next_line()
            .await
            .inspect_err(|_| self.disconnect())
    }

    fn subscribe_lines(&self) -> Result<LineStream, DisconnectedError> {
        let socket = self.socket()?;
        socket.subscribe_lines()
    }
}
