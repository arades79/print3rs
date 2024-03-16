//! # print3rs-console
//!  A shell to talk to 3D printers or other Gcode accepting serial devices, inspired by Pronsole
//!

use {
    print3rs_core::Printer,
    std::{fmt::Debug, sync::Arc},
    tokio_serial::SerialStream,
};

use futures_util::AsyncWriteExt;
use rustyline_async::{Readline, ReadlineEvent, SharedWriter};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use winnow::Parser;

use print3rs_commands::commands;

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

fn prompt_string(printer: &Printer<SerialStream>) -> String {
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
    let mut commander = commands::Commander::new();

    let (mut readline, mut writer) = Readline::new(prompt_string(commander.printer()))?;

    writer.write_all(commands::version().as_bytes()).await?;
    writer
        .write_all(b"\ntype `:help` for a list of commands\n")
        .await?;
    setup_logging(writer.clone());

    let mut responses = commander.subscribe_responses();

    loop {
        tokio::select! {
            Ok(response) = responses.recv() => {
                match response {
                    commands::Response::Output(s) => {
                        writer.write_all(s.as_bytes()).await?;
                    },
                    commands::Response::Error(e) => {
                        writer.write_all(format!("Error: {}", e.0).as_bytes()).await?;
                    },
                    commands::Response::AutoConnect(a_printer) => {
                        commander.set_printer(Arc::into_inner(a_printer).unwrap_or_default().into_inner().unwrap_or_default());
                    },
                    commands::Response::Clear => {
                        readline.clear()?;
                    },
                    commands::Response::Quit => {
                        readline.flush()?;
                        return Ok(());
                    },
                }
            }
            Ok(event) = readline.readline() => {
                let line = match event {
                    ReadlineEvent::Line(line) => line,
                    _ => {readline.flush()?; return Ok(());}
                };
                let command = match commands::parse_command.parse(&line) {
                    Ok(command) => command,
                    Err(_e) => {
                        writer.write_all(b"invalid command!\n").await?;
                        continue;
                    }
                };
                let _ = commander.dispatch(command);
                readline.add_history_entry(line);
            },
        }
        readline.update_prompt(&prompt_string(commander.printer()))?;
    }
}
