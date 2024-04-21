use std::{fmt::Debug, sync::Arc};

use serde::Serialize;
use winnow::Parser;

mod response;

use response::response;
pub use response::Response;

use print3rs_serializer::{serialize_unsequenced, Sequenced};

use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt},
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

pub type LineStream = broadcast::Receiver<Arc<str>>;

pub async fn search_for_sequence(sequence: i32, mut responses: LineStream) -> Response {
    tracing::debug!("Started looking for Ok {sequence}");
    while let Ok(resp) = responses.recv().await {
        match response.parse(resp.as_bytes()) {
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
    pub responses: broadcast::Receiver<Arc<str>>,
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

impl Socket {
    /// Serialize a struct implementing Serialize and send the bytes to the printer
    ///
    /// Sent bytes will include a sequence number and checksum.
    /// For printers which support advanced OK messages this will allow TCP like checked communication.
    ///
    /// When called, a local task is spawned to check for a matching OK message.
    /// The handle to this task is returned after the first await on success.
    /// This allows simple synchronization of any sent command by awaiting twice.
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn send(
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
    pub fn send_unsequenced(&self, gcode: impl Serialize + Debug) -> Result<(), Error> {
        let bytes = serialize_unsequenced(gcode);
        self.sender.try_send(bytes)?;
        Ok(())
    }

    /// Send any raw sequence of bytes to the printer
    pub fn send_raw(&self, gcode: &[u8]) -> Result<(), Error> {
        self.sender.try_send(gcode.to_owned().into_boxed_slice())?;
        Ok(())
    }

    /// Read the next line from the printer
    ///
    /// May not recieve all lines, if calls to this function are spaced
    /// far apart, the buffer may overfill and the oldest messages will
    /// be dropped. In this case the oldest available message is returned.
    pub async fn read_next_line(&mut self) -> Result<Arc<str>, Error> {
        loop {
            match self.responses.recv().await {
                Ok(line) => break Ok(line),
                Err(broadcast::error::RecvError::Lagged(_)) => todo!(),
                Err(broadcast::error::RecvError::Closed) => break Err(Error::Disconnected),
            }
        }
    }

    /// Obtain a broadcast receiver returning all lines received by the printer
    pub fn subscribe_lines(&self) -> Result<LineStream, Error> {
        Ok(self.responses.resubscribe())
    }
}

/// Handle for asynchronous serial communication with a 3D printer
#[derive(Debug, Default)]
pub enum Printer {
    #[default]
    Disconnected,
    Connected {
        socket: Socket,
        com_task: tokio::task::JoinHandle<()>,
    },
}

impl Drop for Printer {
    fn drop(&mut self) {
        if let Self::Connected { com_task, .. } = self {
            com_task.abort()
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),

    #[error("Background task failed to propagate message from printer\nError message: {0}")]
    ResponseSender(#[from] broadcast::error::SendError<Arc<str>>),

    #[error("Send queue full or closed")]
    Sender(#[from] mpsc::error::TrySendError<Box<[u8]>>),

    #[error("Couldn't reserve a slot to send message")]
    SendReserve(#[from] mpsc::error::SendError<()>),

    #[error("Underlying printer connection was closed")]
    Disconnected,
}

/// Loop for handling sending/receiving in the background with possible split senders/receivers
async fn printer_com_task(
    mut transport: impl AsyncBufRead + AsyncWrite + Unpin,
    mut gcoderx: mpsc::Receiver<Box<[u8]>>,
    responsetx: broadcast::Sender<Arc<str>>,
) {
    tracing::debug!("Started background printer communications");
    let mut buf = String::new();
    loop {
        tokio::select! {
            Some(line) = gcoderx.recv() => {
                if transport.write_all(&line).await.is_err() {return;}
                if transport.flush().await.is_err() {return;}
                tracing::debug!("Sent `{}` to printer", String::from_utf8_lossy(&line).trim());
            },
            Ok(1..) = transport.read_line(&mut buf) => {
                tracing::debug!("Received `{buf}` from printer");
                if responsetx.send(Arc::from(buf.split_off(0))).is_err() {return;}
            },
            else => return,
        }
    }
}

impl Printer {
    /// Create a new printer from a SerialStream.
    ///
    /// Starts a local task to handle printer communication asynchronously
    #[tracing::instrument(level = "debug")]
    pub fn new<S>(port: S) -> Self
    where
        S: AsyncBufRead + AsyncWrite + Unpin + Send + 'static + Debug,
    {
        let (sender, gcoderx) = mpsc::channel::<Box<[u8]>>(8);
        let (response_sender, responses) = broadcast::channel(64);
        let com_task = tokio::task::spawn(printer_com_task(port, gcoderx, response_sender));
        let serializer = Sequenced::default();
        Self::Connected {
            socket: Socket {
                sender,
                serializer,
                responses,
            },
            com_task,
        }
    }

    /// Connect to a device
    pub fn connect<S>(&mut self, port: S)
    where
        S: AsyncBufRead + AsyncWrite + Unpin + Send + 'static + Debug,
    {
        *self = Printer::new(port);
    }

    /// Obtain a cloneable socket handle to talk to printer
    pub fn socket(&self) -> Option<&Socket> {
        match self {
            Self::Disconnected => None,
            Self::Connected { socket, .. } => Some(socket),
        }
    }

    /// Obtain an exclusive socket handle - needed to read
    pub fn socket_mut(&mut self) -> Option<&mut Socket> {
        match self {
            Self::Disconnected => None,
            Self::Connected { socket, .. } => Some(socket),
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

    pub fn background_task(&self) -> Option<&JoinHandle<()>> {
        match self {
            Printer::Disconnected => None,
            Printer::Connected { com_task, .. } => Some(com_task),
        }
    }

    /// Serialize a struct implementing Serialize and send the bytes to the printer
    ///
    /// Sent bytes will include a sequence number and checksum.
    /// For printers which support advanced OK messages this will allow TCP like checked communication.
    ///
    /// When called, a local task is spawned to check for a matching OK message.
    /// The handle to this task is returned after the first await on success.
    /// This allows simple synchronization of any sent command by awaiting twice.
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn send(
        &self,
        gcode: impl Serialize + Debug,
    ) -> Result<tokio::task::JoinHandle<Response>, Error> {
        self.socket().ok_or(Error::Disconnected)?.send(gcode).await
    }

    /// Serialize anything implementing Serialize and send the bytes to the printer
    ///
    /// There is no guarantee that a command is correctly recieved or serviced;
    /// any synchronization based on responses will have to be done manually.
    ///
    /// If your printer supports it, the sequenced `send` function is preferred,
    /// although this version is slightly lower overhead.
    pub fn send_unsequenced(&self, gcode: impl Serialize + Debug) -> Result<(), Error> {
        self.socket()
            .ok_or(Error::Disconnected)?
            .send_unsequenced(gcode)
    }

    /// Send any raw sequence of bytes to the printer
    pub fn send_raw(&self, gcode: &[u8]) -> Result<(), Error> {
        self.socket().ok_or(Error::Disconnected)?.send_raw(gcode)
    }

    /// Read the next line from the printer
    ///
    /// May not recieve all lines, if calls to this function are spaced
    /// far apart, the buffer may overfill and the oldest messages will
    /// be dropped. In this case the oldest available message is returned.
    pub async fn read_next_line(&mut self) -> Result<Arc<str>, Error> {
        self.socket_mut()
            .ok_or(Error::Disconnected)?
            .read_next_line()
            .await
    }

    /// Obtain a broadcast receiver returning all lines received by the printer
    pub fn subscribe_lines(&self) -> Result<LineStream, Error> {
        self.socket().ok_or(Error::Disconnected)?.subscribe_lines()
    }
}

impl From<Option<Printer>> for Printer {
    fn from(value: Option<Printer>) -> Self {
        match value {
            Some(printer) => printer,
            None => Printer::Disconnected,
        }
    }
}
