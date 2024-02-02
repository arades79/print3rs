mod commands;
mod logging;

use std::{borrow::Cow, collections::HashMap, fmt::Display};

use commands::{auto_connect, help, version};
use futures_util::AsyncWriteExt;
use rustyline_async::{Readline, ReadlineEvent, SharedWriter};
use tokio::io::{AsyncReadExt, AsyncWriteExt as TokioAsyncWrite};
use tokio_serial::SerialPortBuilderExt;
use winnow::Parser;

use print3rs_core::{Error as PrinterError, Printer};

fn connect_printer(
    printer: &Printer,
    writer: &SharedWriter,
) -> Result<tokio::task::JoinHandle<()>, PrinterError> {
    let mut printer_lines = printer.subscribe_lines()?;
    let mut print_line_writer = writer.clone();
    let abort_handle = printer.remote_disconnect();
    let background_comms = tokio::task::spawn(async move {
        while let Ok(line) = printer_lines.recv().await {
            match print_line_writer.write_all(&line).await {
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        abort_handle.abort();
    });
    Ok(background_comms)
}

const ERR_NO_PRINTER: &str = "Printer not connected!\n";

async fn start_print_file(
    filename: &str,
    printer: &Printer,
) -> Result<tokio::task::JoinHandle<()>, PrinterError> {
    let mut file = tokio::fs::File::open(filename).await?;
    let mut file_contents = String::new();
    file.read_to_string(&mut file_contents).await?;
    let mut socket = printer.socket();
    let task = tokio::spawn(async move {
        for line in file_contents.lines() {
            socket.send(line).await.unwrap().await.unwrap();
        }
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
    let mut log_printer_reader = printer.subscribe_lines()?;
    let log_task_handle = tokio::spawn(async move {
        while let Ok(log_line) = log_printer_reader.recv().await {
            if let Ok(parsed) = parser.parse(&log_line) {
                let mut record_bytes = Vec::new();
                for val in parsed {
                    record_bytes.extend_from_slice(ryu::Buffer::new().format(val).as_bytes());
                    record_bytes.push(b',');
                }
                record_bytes.pop(); // remove trailing ','
                record_bytes.push(b'\n');
                log_file.write_all(&record_bytes).await.unwrap_or_default();
            }
        }
    });
    Ok(log_task_handle)
}

async fn start_repeat(
    gcodes: Vec<Cow<'_, str>>,
    printer: &Printer,
) -> tokio::task::JoinHandle<()> {
    let gcodes: Vec<String> = gcodes.into_iter().map(|s| s.into_owned()).collect();
    let mut socket = printer.socket();
    let repeat_task = tokio::spawn(async move {
        for ref line in gcodes.into_iter().cycle() {
            let sequence_watch = match socket.send(line).await {
                Ok(handle) => handle,
                Err(_) => break,
            };
            match sequence_watch.await {
                Ok(_) => (),
                Err(_) => break,
            };
        }
    });
    repeat_task
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

#[tokio::main(flavor = "current_thread")]
async fn main() -> eyre::Result<()> {
    let status = Status::Disconnected;
    let (mut readline, mut writer) = Readline::new(prompt_string(status))?;
    let mut printer = Printer::new_disconnected();
    let mut printer_reader = None;

    let mut background_tasks = HashMap::new();

    commands::version(&mut writer).await;
    writer
        .write_all(b"type `:help` for a list of commands\n")
        .await?;

    while let ReadlineEvent::Line(line) = readline.readline().await? {
        let command = match commands::parse_command.parse(&line) {
            Ok(command) => command,
            Err(_) => {
                writer.write_all(b"invalid command!\n").await?;
                continue;
            }
        };
        use commands::Command::*;
        match command {
            Gcodes(gcodes) => {
                if status == Status::Disconnected {
                    writer
                        .write_all(
                            "No printer connected! Use ':help' for help connecting.\n".as_bytes(),
                        )
                        .await?
                } else {
                    for line in gcodes {
                        match printer.send_unsequenced(line).await {
                            Ok(_) => (),
                            Err(PrinterError::Disconnected) => {
                                status = Status::Disconnected
                            }
                            Err(e) => tracing::error!("{e}"),
                        };
                    }
                }
            }
            Log(name, pattern) => match start_logging(name, pattern, &printer).await {
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
            },
            Repeat(name, gcodes) => {
                match start_repeat(gcodes, &printer).await {
                    Ok(repeat_task) => {
                        background_tasks.insert(
                            name.to_owned(),
                            BackgroundTask {
                                description: "repeat",
                                abort_handle: repeat_task.abort_handle(),
                            },
                        );
                    }
                    Err(e) => {
                        writer.write_all(e.to_string().as_bytes()).await?;
                    }
                };
            }
            Connect(path, baud) => {
                match tokio_serial::new(path, baud.unwrap_or(115200)).open_native_async() {
                    Ok(serial) => {
                        printer.connect(serial);
                        connect_printer(&printer, &writer)?;
                        status = Status::Connected;
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
                        printer = new_printer;
                        printer_reader = Some(connect_printer(&printer, &writer)?);
                        status = Status::Connected;
                        "Found printer!\n".as_bytes()
                    }
                    None => "Printer not found.\n".as_bytes(),
                };
                writer.write_all(msg).await?;
            }
            Disconnect => {
                printer.disconnect();
                printer_reader.take().map(|handle| handle.abort());
                background_tasks.clear();
                status = Status::Disconnected;
            }
            Help(sub) => help(&mut writer, sub).await,
            Version => version(&mut writer).await,
            Clear => readline.clear()?,
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
            Print(filename) => match start_print_file(filename, &printer).await {
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
            Quit => break,
        };
        readline.update_prompt(prompt_string(status))?;
        readline.add_history_entry(line);
    }
    readline.flush()?;
    Ok(())
}
