mod commands;
mod logging;

use std::{borrow::Cow, collections::HashMap, fs::read};

use commands::{auto_connect, help, version};
use eyre::OptionExt;
use futures_util::AsyncWriteExt;
use rustyline_async::{Readline, ReadlineEvent, SharedWriter};
use tokio::io::{AsyncReadExt, AsyncWriteExt as TokioAsyncWrite};
use tokio_serial::SerialPortBuilderExt;
use winnow::Parser;

use print3rs_core::{LineStream, Printer};

fn connect_printer(
    printer: &Printer,
    writer: &SharedWriter,
) -> eyre::Result<tokio::task::JoinHandle<()>> {
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
    printer: &Option<Printer>,
) -> eyre::Result<tokio::task::JoinHandle<()>> {
    let printer = printer.as_ref().ok_or_eyre(ERR_NO_PRINTER)?;

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
    printer: &Option<Printer>,
) -> eyre::Result<tokio::task::JoinHandle<()>> {
    let printer = printer.as_ref().ok_or_eyre(ERR_NO_PRINTER)?;

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
    printer: &Option<Printer>,
) -> eyre::Result<tokio::task::JoinHandle<()>> {
    let printer = printer.as_ref().ok_or_eyre(ERR_NO_PRINTER)?;
    let gcodes: Vec<String> = gcodes.into_iter().map(|s| s.into_owned()).collect();
    let mut socket = printer.socket();
    let repeat_task = tokio::spawn(async move {
        for ref line in gcodes.into_iter().cycle() {
            socket.send(line).await.unwrap().await.unwrap();
        }
    });
    Ok(repeat_task)
}

struct BackgroundTask {
    description: &'static str,
    abort_handle: tokio::task::AbortHandle,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> eyre::Result<()> {
    static PROMPT_DISCONNECTED: &str = "[disconnected]> ";
    static PROMPT_CONNECTED: &str = "[connected]> ";

    let (mut readline, mut writer) = Readline::new(PROMPT_DISCONNECTED.to_string())?;
    let mut printer: Option<Printer> = None;
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
                if let Some(ref mut printer) = printer {
                    for line in gcodes {
                        printer.send_unsequenced(line).await?;
                    }
                } else {
                    writer
                        .write_all(
                            "No printer connected! Use ':help' for help connecting.\n".as_bytes(),
                        )
                        .await?
                };
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
                printer = match tokio_serial::new(path, baud.unwrap_or(115200)).open_native_async()
                {
                    Ok(serial) => {
                        readline.update_prompt(PROMPT_CONNECTED.to_string())?;
                        let printer = Printer::new(serial);
                        connect_printer(&printer, &writer)?;
                        Some(printer)
                    }
                    Err(e) => {
                        writer
                            .write_all(format!("Connection failed!\nError: {e}\n").as_bytes())
                            .await?;
                        None
                    }
                };
            }
            AutoConnect => {
                writer.write_all(b"Connecting...\n").await?;
                printer = auto_connect().await;
                let msg = match printer {
                    Some(ref printer) => {
                        printer_reader = Some(connect_printer(printer, &writer)?);
                        readline.update_prompt(PROMPT_CONNECTED.to_string())?;
                        "Found printer!\n".as_bytes()
                    }
                    None => "Printer not found.\n".as_bytes(),
                };
                writer.write_all(msg).await?;
                readline.flush()?;
            }
            Disconnect => {
                printer.take().map(|printer| printer.disconnect());
                printer_reader.take().map(|handle| handle.abort());
                readline.update_prompt(PROMPT_DISCONNECTED.to_string())?;
            }
            Help(sub) => help(&mut writer, sub).await,
            Version => version(&mut writer).await,
            Clear => {
                readline.clear()?;
            }
            Unrecognized => {
                writer
                    .write_all(
                        "Invalid command! use ':help' for valid commands and syntax\n".as_bytes(),
                    )
                    .await?;
            }
            Tasks => {
                for (
                    name,
                    BackgroundTask {
                        description,
                        abort_handle: _,
                    },
                ) in background_tasks.iter()
                {
                    // TODO: add task strings into the value
                    writer
                        .write_all(format!("{name}\t{description}\n").as_bytes())
                        .await?;
                }
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
        readline.add_history_entry(line);
    }
    readline.flush()?;
    Ok(())
}
