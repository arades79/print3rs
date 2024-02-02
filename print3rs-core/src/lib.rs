use std::{
    fmt::Debug,
    ops::{Deref, DerefMut},
    sync::{Arc, Weak},
};

use serde::Serialize;
use winnow::Parser;

mod response;

use response::response;
pub use response::Response;
use tokio_serial::SerialStream;

use gcode_serializer::Serializer;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{broadcast, mpsc},
};

use bytes::{Bytes, BytesMut};

pub type Serial = SerialStream;
pub type LineStream = broadcast::Receiver<Bytes>;

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

#[derive(Debug, Clone)]
pub struct Socket {
    sender: mpsc::Sender<Bytes>,
    serializer: Serializer,
    pub responses: Weak<broadcast::Sender<Bytes>>,
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
        &mut self,
        gcode: impl Serialize + Debug,
    ) -> Result<tokio::task::JoinHandle<Response>, Error> {
        let send_slot = self.sender.reserve().await?;
        let (sequence, bytes) = self.serializer.serialize(gcode);
        let sequenced_ok_watch = self.responses.subscribe();
        send_slot.send(bytes.freeze());
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
    pub async fn send_unsequenced(&self, gcode: impl Serialize + Debug) -> Result<(), Error> {
        let bytes = self.serializer.serialize_unsequenced(gcode);
        self.sender.send(bytes.freeze()).await?;
        Ok(())
    }

    /// Send any raw sequence of bytes to the printer
    pub async fn send_raw(&self, gcode: &[u8]) -> Result<(), Error> {
        self.sender.send(Bytes::copy_from_slice(gcode)).await?;
        Ok(())
    }

    /// Retrieve the next line to come in from the printer.
    ///
    /// There is no buffering of lines for this method,
    /// only a line which comes in after this call will be returned.
    ///
    /// Because of this, there's a reasonable chance of missing lines with this method,
    /// it is also high overhead due to establishing a new channel each call.
    ///
    /// If all lines should be processed, use `subscribe_lines`
    pub async fn read_next_line(&self) -> Result<Bytes, Error> {
        let line = self.subscribe_lines()?.recv().await?;
        Ok(line)
    }

    /// Obtain a broadcast receiver returning all lines received by the printer
    pub fn subscribe_lines(&self) -> Result<LineStream, Error> {
        let sender = self.responses.upgrade().ok_or(Error::Disconnected)?;
        Ok(sender.subscribe())
    }
}

/// Handle for asynchronous serial communication with a 3D printer
pub struct Printer {
    socket: Socket,
    com_task: tokio::task::JoinHandle<Result<(), Error>>,
}

impl Drop for Printer {
    fn drop(&mut self) {
        self.com_task.abort()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),

    #[error("Background task failed to propagate message from printer\nError message: {0}")]
    ResponseSender(#[from] broadcast::error::SendError<Bytes>),

    #[error("Couldn't send data to background task\nError message: {0}")]
    Sender(#[from] mpsc::error::SendError<Bytes>),

    #[error("Couldn't reserve a slot to send message\nError message: {0}")]
    SendReserve(#[from] mpsc::error::SendError<()>),

    #[error("Couldn't retreive data from background task\nError message: {0}")]
    ResponseReceiver(#[from] broadcast::error::RecvError),

    #[error("Underlying printer connection was closed")]
    Disconnected,
}

/// Loop for handling sending/receiving in the background with possible split senders/receivers
async fn printer_com_task(
    mut serial: Serial,
    mut gcoderx: mpsc::Receiver<Bytes>,
    responsetx: Arc<broadcast::Sender<Bytes>>,
) -> Result<(), Error> {
    let mut buf = BytesMut::with_capacity(1024);
    tracing::debug!("Started background printer communications");
    loop {
        tokio::select! {
            Some(line) = gcoderx.recv() => {
                serial.write_all(&line).await?;
                serial.flush().await?;
                tracing::debug!("Sent `{}` to printer", String::from_utf8_lossy(&line).trim());
            },
            Ok(_) = serial.read_buf(&mut buf) => {
                while let Some(n) = buf.iter().position(|b| *b == b'\n') {
                    let line = buf.split_to(n + 1).freeze();
                    tracing::debug!("Received `{}` from printer", String::from_utf8_lossy(&line).trim());
                    let _ = responsetx.send(line); // ignore errors and keep trying
                }
            },
            else => break Err(Error::Disconnected),
        }
    }
}

impl Printer {
    /// Create a new printer from a SerialStream.
    ///
    /// Starts a local task to handle printer communication asynchronously
    #[tracing::instrument(level = "debug")]
    pub fn new(port: Serial) -> Self {
        let (sender, gcoderx) = mpsc::channel::<Bytes>(8);
        let (response_sender, _) = broadcast::channel(64);
        let response_sender = Arc::new(response_sender);
        let responses = Arc::downgrade(&response_sender);
        let com_task = tokio::task::spawn(printer_com_task(port, gcoderx, response_sender));
        let serializer = Serializer::default();
        Self {
            socket: Socket {
                sender,
                serializer,
                responses,
            },
            com_task,
        }
    }

    /// Recreates a disconnected printer in place
    /// All previous handles will be invalid and need recreation
    pub fn connect(&mut self, port: Serial) {
        let new_printer = Printer::new(port);
        let _ = core::mem::replace(self, new_printer);
    }

    /// Obtain a socket to talk to printer
    pub fn socket(&self) -> Socket {
        self.socket.clone()
    }

    /// Disconnect the printer and shutdown background communication
    pub fn disconnect(&self) {
        self.com_task.abort();
    }

    /// Get a handle to disconnect the printer from some background task
    pub fn remote_disconnect(&self) -> tokio::task::AbortHandle {
        self.com_task.abort_handle()
    }
}

impl Deref for Printer {
    type Target = Socket;

    fn deref(&self) -> &Self::Target {
        &self.socket
    }
}

impl DerefMut for Printer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.socket
    }
}
