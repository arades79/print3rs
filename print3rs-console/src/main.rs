//! # print3rs-console
//!  A shell to talk to 3D printers or other Gcode accepting serial devices, inspired by Pronsole
//!

use {
    print3rs_commands::commands::{start_repeat, BackgroundTask},
    print3rs_core::{AsyncPrinterComm, Printer, SerialPrinter},
    std::{collections::HashMap, fmt::Debug},
    tokio_serial::SerialPortBuilderExt,
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

fn prompt_string(printer: &SerialPrinter) -> String {
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
    let mut printer = Printer::default();

    let (mut readline, mut writer) = Readline::new(prompt_string(&printer))?;

    writer.write_all(commands::version().as_bytes()).await?;
    writer
        .write_all(b"\ntype `:help` for a list of commands\n")
        .await?;
    setup_logging(writer.clone());

    let mut tasks = HashMap::new();
    let mut macros: HashMap<String, Vec<String>> = HashMap::new();

    loop {
        tokio::select! {
            Ok(response) = printer.read_next_line() => {
                writer.write_all(&response).await?;
            },
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
                const DISCONNECTED_ERROR: &[u8] = b"No printer connected!\n";
                 match command {
                        commands::Command::Clear => {readline.clear()?;},
                        commands::Command::Quit => {
                            readline.flush()?;
                            return Ok(());
                        }
                        commands::Command::Gcodes(codes) => {
                            if let Err(_e) = commands::send_gcodes(&printer, &codes, Some(&macros)) {
                            writer.write_all(DISCONNECTED_ERROR).await?;
                        }},
                        commands::Command::Print(filename) => {
                            if let Ok(print) = commands::start_print_file(filename, &printer) {
                            tasks.insert(filename.to_string(), print);
                            } else {
                                writer.write_all(DISCONNECTED_ERROR).await?;
                            }
                        }
                        commands::Command::Log(name, pattern) => {
                            if let Ok(log) = commands::start_logging(name, pattern, &printer) {
                            tasks.insert(name.to_string(), log);
                            } else {
                                writer.write_all(DISCONNECTED_ERROR).await?;
                            }
                        }
                        commands::Command::Repeat(name, gcodes) => {
                            if let Ok(socket) = printer.socket() {
                                let repeat = start_repeat(gcodes, socket.clone());
                                tasks.insert(name.to_string(), repeat);}
                            else {
                                writer.write_all(DISCONNECTED_ERROR).await?;
                            }
                        }
                        commands::Command::Tasks => {
                            for (
                                name,
                                BackgroundTask {
                                    description,
                                    abort_handle: _,
                                },
                            ) in tasks.iter()
                            {
                                writer
                                    .write_all(format!("{name}\t{description}\n").as_bytes())
                                    .await?;
                            }
                        }
                        commands::Command::Stop(name) => {
                            tasks.remove(name);
                        }
                        commands::Command::Macro(name, commands) => {
                            let commands = commands.into_iter().map(|s| s.to_string()).collect();
                            macros.insert(name.to_owned(), commands);
                        },
                        commands::Command::Macros => {
                            for (name, steps) in macros.iter() {
                                writer.write_all(name.as_bytes()).await?;
                                writer.write_all(b"\t").await?;
                                for step in steps {
                                    writer.write_all(step.as_bytes()).await?;
                                    writer.write_all(b";").await?;
                                }
                                writer.write_all(b"\n").await?;
                            }
                        }
                        commands::Command::DeleteMacro(name) => {
                            macros.remove(name);
                        }
                        commands::Command::Connect(path, baud) => {
                            if let Ok(port) = tokio_serial::new(path, baud.unwrap_or(115200)).open_native_async() {
                            printer.connect(port);
                            } else {
                                writer.write_all(b"Connection failed.\n").await?;
                            }
                        }
                        commands::Command::AutoConnect => {
                            writer.write_all(b"Connecting...\n").await?;
                            printer = commands::auto_connect().await;
                            writer.write_all(if printer.is_connected() {b"Found printer!\n"} else {b"No printer found.\n"}).await?;
                        }
                        commands::Command::Disconnect => printer.disconnect(),
                        commands::Command::Help(subcommand) => {
                            writer
                                .write_all(commands::help(subcommand).as_bytes())
                                .await?
                        }
                        commands::Command::Version => writer.write_all(commands::version().as_bytes()).await?,
                        _ => {
                            writer
                                .write_all(b"Unsupported command!\n")
                                .await?
                        }
                    };

                readline.add_history_entry(line);
            },
        }
        readline.update_prompt(&prompt_string(&printer))?;
    }
}
