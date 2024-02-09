mod commands;
mod logging;

use std::{
    borrow::Cow,
    collections::HashMap,
    fmt::{Debug, Display},
};

use commands::{auto_connect, help, version};
use futures_util::AsyncWriteExt;
use rustyline_async::{Readline, ReadlineEvent, SharedWriter};
use tokio::io::{AsyncReadExt, AsyncWriteExt as TokioAsyncWrite};
use tokio_serial::SerialPortBuilderExt;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use winnow::Parser;

use print3rs_core::{Error as PrinterError, Printer};

fn connect_printer(
    printer: &Printer,
    writer: &SharedWriter,
    disconnect_notify: &mut Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<tokio::task::JoinHandle<()>, PrinterError> {
    let mut printer_lines = printer.subscribe_lines();
    let mut print_line_writer = writer.clone();
    let abort_handle = printer.remote_disconnect();
    let (disconnecttx, disconnectrx) = tokio::sync::oneshot::channel();
    *disconnect_notify = Some(disconnectrx);
    let background_comms = tokio::task::spawn(async move {
        while let Ok(line) = printer_lines.recv().await {
            match print_line_writer.write_all(&line).await {
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        abort_handle.abort();
        disconnecttx.send(()).unwrap_or_default();
    });
    Ok(background_comms)
}

async fn start_print_file(
    filename: &str,
    printer: &Printer,
) -> Result<tokio::task::JoinHandle<eyre::Result<()>>, PrinterError> {
    let mut file = tokio::fs::File::open(filename).await?;
    let mut file_contents = String::new();
    file.read_to_string(&mut file_contents).await?;
    let mut socket = printer.socket();
    let task = tokio::spawn(async move {
        for line in file_contents.lines() {
            socket.send(line).await?.await?;
        }
        Ok(())
    });
    Ok(task)
}

async fn start_logging(
    name: &str,
    pattern: Vec<logging::parsing::Segment<'_>>,
    printer: &Printer,
) -> Result<tokio::task::JoinHandle<()>, PrinterError> {
    let mut log_file = tokio::fs::File::create(format!(
        "{name}_{timestamp}.csv",
        timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    ))
    .await?;

    log_file
        .write_all(logging::parsing::get_headers(&pattern).as_bytes())
        .await?;

    let mut parser = logging::parsing::make_parser(pattern);
    let mut log_printer_reader = printer.subscribe_lines();
    let log_task_handle = tokio::spawn(async move {
        while let Ok(log_line) = log_printer_reader.recv().await {
            if let Ok(parsed) = parser.parse(&log_line) {
                let mut record_bytes = String::new();
                for val in parsed {
                    record_bytes.push_str(&val.to_string());
                    record_bytes.push(',');
                }
                record_bytes.pop(); // remove trailing ','
                record_bytes.push('\n');
                log_file
                    .write_all(record_bytes.as_bytes())
                    .await
                    .unwrap_or_default();
            }
        }
    });
    Ok(log_task_handle)
}

async fn start_repeat(
    gcodes: Vec<Cow<'_, str>>,
    printer: &Printer,
) -> tokio::task::JoinHandle<eyre::Result<()>> {
    let gcodes: Vec<String> = gcodes.into_iter().map(|s| s.into_owned()).collect();
    let mut socket = printer.socket();

    tokio::spawn(async move {
        for ref line in gcodes.into_iter().cycle() {
            socket.send(line).await?.await?;
        }
        Ok(())
    })
}

struct BackgroundTask {
    description: &'static str,
    abort_handle: tokio::task::AbortHandle,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Status {
    Disconnected,
    Connected,
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Status::Disconnected => "disconnected",
            Status::Connected => "connected",
        })
    }
}

fn prompt_string(status: Status) -> String {
    format!("[{status}]> ")
}

fn disconnect(
    printer: &mut Printer,
    printer_reader: &mut Option<tokio::task::JoinHandle<()>>,
    background_tasks: &mut HashMap<String, BackgroundTask>,
    status: &mut Status,
) {
    printer.disconnect();
    if let Some(handle) = printer_reader.take() {
        handle.abort()
    }
    background_tasks.clear();
    *status = Status::Disconnected;
}

async fn not_connected(writer: &mut SharedWriter) -> Result<(), rustyline_async::ReadlineError> {
    const ERR_NO_PRINTER: &[u8] = b"Printer not connected! Use ':help' for help connecting.\n";
    writer.write_all(ERR_NO_PRINTER).await?;
    Ok(())
}

async fn handle_command(
    command: commands::Command<'_>,
    printer: &mut Printer,
    writer: &mut SharedWriter,
    status: &mut Status,
    background_tasks: &mut HashMap<String, BackgroundTask>,
    printer_reader: &mut Option<tokio::task::JoinHandle<()>>,
    disconnect_notify: &mut Option<tokio::sync::oneshot::Receiver<()>>,
) -> eyre::Result<()> {
    use commands::Command::*;
    match command {
        Gcodes(gcodes) => {
            if *status == Status::Disconnected {
                not_connected(writer).await?;
                return Ok(());
            } else {
                for line in gcodes {
                    match printer.send_unsequenced(line).await {
                        Ok(_) => (),
                        Err(e) => {
                            tracing::error!("{e}");
                            disconnect(printer, printer_reader, background_tasks, status)
                        }
                    };
                }
            }
        }
        Log(name, pattern) => {
            if *status == Status::Disconnected {
                not_connected(writer).await?;
                return Ok(());
            }
            match start_logging(name, pattern, printer).await {
                Ok(log_task_handle) => {
                    background_tasks.insert(
                        name.to_owned(),
                        BackgroundTask {
                            description: "log",
                            abort_handle: log_task_handle.abort_handle(),
                        },
                    );
                }
                Err(e) => {
                    writer.write_all(e.to_string().as_bytes()).await?;
                }
            };
        }
        Repeat(name, gcodes) => {
            if *status == Status::Disconnected {
                not_connected(writer).await?;
                return Ok(());
            }
            let repeat_task = start_repeat(gcodes, printer).await;

            background_tasks.insert(
                name.to_owned(),
                BackgroundTask {
                    description: "repeat",
                    abort_handle: repeat_task.abort_handle(),
                },
            );
        }
        Connect(path, baud) => {
            match tokio_serial::new(path, baud.unwrap_or(115200)).open_native_async() {
                Ok(serial) => {
                    printer.connect(serial);
                    *printer_reader = Some(connect_printer(printer, writer, disconnect_notify)?);
                    *status = Status::Connected;
                }
                Err(e) => {
                    writer
                        .write_all(format!("Connection failed!\nError: {e}\n").as_bytes())
                        .await?;
                }
            };
        }
        AutoConnect => {
            writer.write_all(b"Connecting...\n").await?;
            let msg = match auto_connect().await {
                Some(new_printer) => {
                    *printer = new_printer;
                    *printer_reader = Some(connect_printer(printer, writer, disconnect_notify)?);
                    *status = Status::Connected;
                    "Found printer!\n".as_bytes()
                }
                None => "Printer not found.\n".as_bytes(),
            };
            writer.write_all(msg).await?;
        }
        Disconnect => {
            disconnect(printer, printer_reader, background_tasks, status);
        }
        Help(sub) => help(writer, sub).await,
        Version => version(writer).await,
        Unrecognized => {
            writer
                .write_all(
                    "Invalid command! use ':help' for valid commands and syntax\n".as_bytes(),
                )
                .await?;
        }
        Tasks => {
            if background_tasks.is_empty() {
                writer.write_all(b"No active tasks.\n").await?;
            } else {
                for (
                    name,
                    BackgroundTask {
                        description,
                        abort_handle: _,
                    },
                ) in background_tasks.iter()
                {
                    writer
                        .write_all(format!("{name}\t{description}\n").as_bytes())
                        .await?;
                }
            };
        }
        Stop(label) => {
            if let Some(task_handle) = background_tasks.remove(label) {
                task_handle.abort_handle.abort();
            } else {
                writer
                    .write_all(format!("No task named {label} running\n").as_bytes())
                    .await?;
            }
        }
        Print(filename) => match start_print_file(filename, printer).await {
            Ok(print_task) => {
                background_tasks.insert(
                    filename.to_owned(),
                    BackgroundTask {
                        description: "print",
                        abort_handle: print_task.abort_handle(),
                    },
                );
            }
            Err(e) => {
                writer.write_all(e.to_string().as_bytes()).await?;
            }
        },
        Clear => (), // needs external handling
        Quit => (),  // needs external handling
    };
    Ok(())
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
async fn main() -> eyre::Result<()> {
    let mut status = Status::Disconnected;
    let (mut readline, mut writer) = Readline::new(prompt_string(status))?;

    let mut printer = Printer::new_disconnected();
    let mut printer_reader = None;
    let mut disconnect_notify = None;

    let mut background_tasks = HashMap::new();
    commands::version(&mut writer).await;
    writer
        .write_all(b"type `:help` for a list of commands\n")
        .await?;

    setup_logging(writer.clone());

    loop {
        tokio::select! { Ok(ReadlineEvent::Line(line)) = readline.readline() => {
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
                        handle_command(
                            other,
                            &mut printer,
                            &mut writer,
                            &mut status,
                            &mut background_tasks,
                            &mut printer_reader,
                            &mut disconnect_notify,
                        )
                        .await?
                    }
                }
                readline.add_history_entry(line);
            },
            Ok(_) = &mut disconnect_notify.take().unwrap_or_else(|| {let (_tx, rx) = tokio::sync::oneshot::channel(); rx}), if disconnect_notify.is_some() => {
                disconnect(&mut printer, &mut printer_reader, &mut background_tasks, &mut status);
            },
            else => {readline.flush()?; return Ok(());}
        }
        readline.update_prompt(&prompt_string(status))?;
    }
}
