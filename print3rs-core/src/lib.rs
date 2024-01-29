use std::fmt::Debug;

use serde::Serialize;
use winnow::Parser;

mod response;

use response::{response, Response};
use tokio_serial::SerialStream;

use gcode_serializer::Serializer;

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{broadcast, mpsc},
};
use tracing;

use bytes::{Bytes, BytesMut};

pub type Serial = SerialStream;
pub type PrinterLines = broadcast::Receiver<Bytes>;
pub type PrinterSender = mpsc::Sender<Bytes>;

/// Handle for asynchronous serial communication with a 3D printer
#[derive(Debug)]
pub struct Printer {
    sender: mpsc::Sender<Bytes>,
    response_channel: broadcast::Sender<Bytes>,
    _com_task: tokio::task::JoinHandle<Result<(), Error>>,
    serializer: Serializer,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),

    #[error("Background task failed to propagate message from printer\nError message: {0}")]
    ResponseSender(#[from] broadcast::error::SendError<Bytes>),

    #[error("Couldn't send data to background task\nError message: {0}")]
    Sender(#[from] mpsc::error::SendError<Bytes>),

    #[error("Couldn't retreive data from background task\nError message: {0}")]
    ResponseReceiver(#[from] broadcast::error::RecvError),
}

/// Loop for handling sending/receiving in the background with possible split senders/receivers
//#[tracing::instrument(level = "debug")]
async fn printer_com_task(
    mut serial: Serial,
    mut gcoderx: mpsc::Receiver<Bytes>,
    responsetx: broadcast::Sender<Bytes>,
) -> Result<(), Error> {
    let mut buf = BytesMut::new();
    loop {
        tokio::select! {
            Some(line) = gcoderx.recv() => {
                serial.write_all(&line).await?;
                serial.flush().await?;
                tracing::debug!("Sent `{}` to printer", String::from_utf8_lossy(&line).trim());
            },
            Ok(_) = serial.read(&mut buf) => {
                let newline = buf.iter().position(|b| *b == b'\n');
                if let Some(n) = newline {
                    tracing::debug!("Received `{}` from printer", String::from_utf8_lossy(&buf).trim());
                    let line = buf.split_to(n + 1).freeze();
                    let _ = responsetx.send(line); // ignore errors and keep trying
                }
            },
            else => ()
        }
    }
}

impl Printer {
    /// Create a new printer from a SerialStream.
    ///
    /// Starts a local task to handle printer communication asynchronously
    #[tracing::instrument(level = "debug")]
    pub fn new(port: Serial) -> Self {
        let (gcodetx, gcoderx) = mpsc::channel::<Bytes>(8);
        let (response_channel, _) = broadcast::channel(8);
        let _com_task =
            tokio::task::spawn(printer_com_task(port, gcoderx, response_channel.clone()));
        Self {
            sender: gcodetx,
            response_channel,
            _com_task,
            serializer: Serializer::default(),
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
        &mut self,
        gcode: impl Serialize + Debug,
    ) -> Result<tokio::task::JoinHandle<Response>, Error> {
        let bytes = self.serializer.serialize(gcode);
        let mut sequenced_ok_watch = self.response_channel.subscribe();
        self.sender.send(bytes.clone()).await?;
        let sequence = self.serializer.sequence();
        let wait_for_response = tokio::task::spawn(async move {
            while let Ok(resp) = sequenced_ok_watch.recv().await {
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
            return Response::Ok;
        });
        Ok(wait_for_response)
    }

    /// Serialize anything implementing Serialize and send the bytes to the printer
    ///
    /// There is no guarantee that a command is correctly recieved or serviced;
    /// any synchronization based on responses will have to be done manually.
    ///
    /// If your printer supports it, the sequenced `send` function is preferred,
    /// although this version is slightly lower overhead.
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn send_unsequenced(&mut self, gcode: impl Serialize + Debug) -> Result<(), Error> {
        let bytes = self.serializer.serialize_unsequenced(gcode);
        self.sender.send(bytes.clone()).await?;
        Ok(())
    }

    /// Send any raw sequence of bytes to the printer
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn send_raw(&mut self, gcode: &[u8]) -> Result<(), Error> {
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
    #[tracing::instrument(level = "debug", skip(self))]
    pub async fn read_next_line(&self) -> Result<Bytes, Error> {
        let line = self.response_channel.subscribe().recv().await?;
        Ok(line)
    }

    /// Obtain a broadcast receiver returning all lines received by the printer
    pub fn subscribe_lines(&self) -> PrinterLines {
        self.response_channel.subscribe()
    }

    /// Obtain a raw bytes sender to send custom messages to the printer e.g. with some custom serializer
    pub fn get_sender(&self) -> PrinterSender {
        self.sender.clone()
    }
}
