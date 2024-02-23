//! # print3rs-console
//!  A shell to talk to 3D printers or other Gcode accepting serial devices, inspired by Pronsole
//!

mod commands;
mod logging;

use std::{collections::HashMap, fmt::Debug, time::Duration};

use futures_util::AsyncWriteExt;
use rustyline_async::{Readline, ReadlineEvent, SharedWriter};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use winnow::Parser;

use print3rs_core::{AsyncPrinterComm, SerialPrinter as Printer};

struct AppState {
    printer: Printer,
    writer: tokio::sync::mpsc::UnboundedSender<String>,
    tasks: HashMap<String, commands::BackgroundTask>,
    error_sender: tokio::sync::mpsc::Sender<AppError>,
}

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("Printer error: {0}")]
    Printer(#[from] print3rs_core::Error),
    #[error("Connection error: {0}")]
    Connection(#[from] tokio_serial::Error),
    #[error("Console error: {0}")]
    Readline(#[from] rustyline_async::ReadlineError),
    #[error("Can't write to console")]
    Writer(#[from] futures_util::io::Error),
}

impl commands::HandleCommand for AppState {
    type Error = AppError;

    fn printer(&self) -> &Printer {
        &self.printer
    }

    fn printer_mut(&mut self) -> &mut Printer {
        &mut self.printer
    }

    fn on_connect(&mut self) {}

    fn respond(&self, message: &str) {
        self.writer.send(message.to_owned()).expect("main exited")
    }

    fn error(&self, err: Self::Error) {
        self.error_sender.try_send(err).expect("too many errors");
    }

    fn add_task(&mut self, task_name: &str, task: commands::BackgroundTask) {
        self.tasks.insert(task_name.to_owned(), task);
    }

    fn remove_task(&mut self, task_name: &str) {
        if let Some(task) = self.tasks.remove(task_name) {
            task.abort_handle.abort();
        }
    }

    fn task_iter(&self) -> impl Iterator<Item = (&String, &commands::BackgroundTask)> {
        self.tasks.iter()
    }
}

fn prompt_string(printer: &Printer) -> String {
    let status = match printer {
        print3rs_core::Printer::Disconnected => "Disconnected",
        print3rs_core::Printer::Connected { .. } => "Connected",
    };
    format!("[{status}]> ")
}

fn setup_logging(writer: SharedWriter) {
    if let Ok(env_log) = tracing_subscriber::EnvFilter::builder()
        .with_env_var("PRINT3RS_LOG")
        .try_from_env()
    {
        let write_layer = tracing_subscriber::fmt::layer().with_writer(move || writer.clone());
        let format_layer = tracing_subscriber::fmt::layer().without_time().compact();
        let logger = tracing_subscriber::registry()
            .with(env_log)
            .with(write_layer)
            .with(format_layer);

        logger.init();
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), AppError> {
    let printer = Printer::default();

    let (mut readline, mut writer) = Readline::new(prompt_string(&printer))?;

    let (error_sender, mut error_receiver) = tokio::sync::mpsc::channel(8);

    writer.write_all(commands::version().as_bytes()).await?;
    writer
        .write_all(b"\ntype `:help` for a list of commands\n")
        .await?;
    setup_logging(writer.clone());

    let (response_sender, mut response_receiver) = tokio::sync::mpsc::unbounded_channel();

    let mut app = AppState {
        printer,
        writer: response_sender.clone(),
        tasks: HashMap::new(),
        error_sender,
    };
    loop {
        tokio::select! {
            Ok(response) = app.printer.read_next_line() => {
                writer.write_all(&response).await?;
            },
            Some(error) = error_receiver.recv() => {
                match error {
                    AppError::Printer(_) => {
                        app.printer.disconnect();
                        writer.write_all(b"Printer is disconnected!\n").await?;
                    },
                    AppError::Connection(_) => {
                        writer.write_all(b"Connection failed!\n").await?;
                    },
                    _ => {readline.flush()?; return Ok(());},
                }
            }
            Some(message) = response_receiver.recv() => {
                writer.write_all(message.as_bytes()).await?;
            },
            Ok(event) = readline.readline() => {
                let line = match event {
                    ReadlineEvent::Line(line) => line,
                    _ => {readline.flush()?; return Ok(());}
                };
                let command = match commands::parse_command.parse(&line) {
                    Ok(command) => command,
                    Err(_) => {
                        writer.write_all(b"invalid command!\n").await?;
                        continue;
                    }
                };
                match command {
                    commands::Command::Clear => readline.clear()?,
                    commands::Command::Quit => {readline.flush()?; return Ok(());},
                    other => {
                        commands::handle_command(&mut app, other).await;
                    }
                }
                readline.add_history_entry(line);
            },
            
        }
        readline.update_prompt(&prompt_string(&app.printer))?;
    }
}
