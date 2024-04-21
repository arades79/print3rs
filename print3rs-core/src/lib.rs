use std::{collections::BTreeMap, fmt::Debug, future::Future, sync::Arc};

use serde::Serialize;
use winnow::Parser;

mod response;

use response::response;
pub use response::Response;

use print3rs_serializer::{serialize_unsequenced, Sequenced};

use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt},
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};

pub type LineStream = broadcast::Receiver<Arc<str>>;

#[derive(Debug)]
struct SendContent(Box<[u8]>, Option<i32>, Option<oneshot::Sender<()>>);

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
    sender: mpsc::Sender<SendContent>,
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
    ) -> Result<impl Future<Output = Result<(), Error>>, Error> {
        let send_slot = self.sender.reserve().await?;
        let (sequence, bytes) = self.serializer.serialize(gcode);
        let (responder, response) = oneshot::channel();
        send_slot.send(SendContent(bytes, Some(sequence), Some(responder)));
        let response = async { response.await.map_err(|e| e.into()) };
        Ok(response)
    }

    /// Serialize anything implementing Serialize and send the bytes to the printer
    ///
    /// There is no guarantee that a command is correctly recieved or serviced;
    /// any synchronization based on responses will have to be done manually.
    ///
    /// If your printer supports it, the sequenced `send` function is preferred,
    /// although this version is slightly lower overhead.
    pub fn send_unsequenced(
        &self,
        gcode: impl Serialize + Debug,
    ) -> Result<impl Future<Output = Result<(), Error>>, Error> {
        let bytes = serialize_unsequenced(gcode);
        let (responder, response) = oneshot::channel();
        self.sender
            .try_send(SendContent(bytes, None, Some(responder)))?;
        let response = async { response.await.map_err(|e| e.into()) };
        Ok(response)
    }

    /// Send any raw sequence of bytes to the printer
    pub fn send_raw(&self, gcode: &[u8]) -> Result<(), Error> {
        self.sender
            .try_send(SendContent(gcode.to_owned().into_boxed_slice(), None, None))?;
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
    Sender(#[from] mpsc::error::TrySendError<SendContent>),

    #[error("Couldn't reserve a slot to send message")]
    SendReserve(#[from] mpsc::error::SendError<()>),

    #[error("Underlying printer connection was closed")]
    Disconnected,

    #[error("Background task stopped before command finished processing")]
    WontRespond(#[from] oneshot::error::RecvError),
}

/// Loop for handling sending/receiving in the background with possible split senders/receivers
async fn printer_com_task(
    mut transport: impl AsyncBufRead + AsyncWrite + Unpin,
    mut gcoderx: mpsc::Receiver<SendContent>,
    responsetx: broadcast::Sender<Arc<str>>,
) {
    tracing::debug!("Started background printer communications");
    let mut buf = String::new();
    let mut pending_responses = BTreeMap::new();
    loop {
        tokio::select! {
            Some(SendContent(line, sequence, responder)) = gcoderx.recv(), if pending_responses.len() < 4 => {
                if transport.write_all(&line).await.is_err() {return;}
                if transport.flush().await.is_err() {return;}
                tracing::debug!("Sent `{}` to printer", String::from_utf8_lossy(&line).trim());
                if let Some(responder) = responder {
                    pending_responses.insert(sequence, responder);
                }
            },
            Ok(1..) = transport.read_line(&mut buf) => {
                tracing::debug!("Received `{buf}` from printer");
                if let Ok(ok_res) = response.parse(buf.as_bytes()) {
                    match ok_res {
                        Response::Ok => {if let Some(responder) = pending_responses.remove(&None){ responder.send(());}},
                        Response::SequencedOk(seq) => {if let Some(responder) = pending_responses.remove(&Some(seq)){ responder.send(());}},
                        Response::Resend(_) => todo!(),
                    }
                }
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
        let (sender, gcoderx) = mpsc::channel::<SendContent>(8);
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
    ) -> Result<impl Future<Output = Result<(), Error>>, Error> {
        self.socket().ok_or(Error::Disconnected)?.send(gcode).await
    }

    /// Serialize anything implementing Serialize and send the bytes to the printer
    ///
    /// There is no guarantee that a command is correctly recieved or serviced;
    /// any synchronization based on responses will have to be done manually.
    ///
    /// If your printer supports it, the sequenced `send` function is preferred,
    /// although this version is slightly lower overhead.
    pub fn send_unsequenced(
        &self,
        gcode: impl Serialize + Debug,
    ) -> Result<impl Future<Output = Result<(), Error>>, Error> {
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
