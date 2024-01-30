mod commands;
mod logging;

use std::{borrow::Cow, collections::HashMap};

use commands::{auto_connect, help, version};
use eyre::OptionExt;
use futures_util::AsyncWriteExt;
use rustyline_async::{Readline, ReadlineEvent, SharedWriter};
use tokio::io::{AsyncReadExt, AsyncWriteExt as TokioAsyncWrite};
use tokio_serial::SerialPortBuilderExt;
use tracing;
use winnow::Parser;

use print3rs_core::{Printer, PrinterLines};

fn connect_printer(
    mut printer_lines: PrinterLines,
    mut print_line_writer: SharedWriter,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn(async move {
        while let Ok(line) = printer_lines.recv().await {
            print_line_writer.write_all(&line).await.unwrap_or(());
        }
    })
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
    let printer_sender = printer.get_sender();
    let printer_reader = printer.subscribe_lines();
    let mut serializer = gcode_serializer::Serializer::default();
    let task = tokio::spawn(async move {
        for line in file_contents.lines() {
            printer_sender
                .send(serializer.serialize(line))
                .await
                .unwrap_or(());
            print3rs_core::search_for_sequence(serializer.sequence(), printer_reader.resubscribe())
                .await;
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
    let mut log_printer_reader = printer.subscribe_lines();
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
                log_file.write_all(&record_bytes).await.unwrap_or(());
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
    let printer_sender = printer.get_sender();
    let printer_reader = printer.subscribe_lines();
    let mut serializer = gcode_serializer::Serializer::default();
    let repeat_task = tokio::spawn(async move {
        for ref line in gcodes.iter().cycle() {
            printer_sender
                .send(serializer.serialize(line))
                .await
                .unwrap_or(());
            print3rs_core::search_for_sequence(serializer.sequence(), printer_reader.resubscribe())
                .await;
        }
    });
    Ok(repeat_task)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> eyre::Result<()> {
    let (mut readline, mut writer) = Readline::new(String::from("> "))?;
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
                    background_tasks.insert(name.to_owned(), log_task_handle);
                }
                Err(e) => {
                    writer.write_all(e.to_string().as_bytes()).await?;
                }
            },
            Repeat(name, gcodes) => {
                match start_repeat(gcodes, &printer).await {
                    Ok(repeat_task) => {
                        background_tasks.insert(name.to_owned(), repeat_task);
                    }
                    Err(e) => {
                        writer.write_all(e.to_string().as_bytes()).await?;
                    }
                };
            }
            Connect(path, baud) => {
                let _ = printer.insert(Printer::new(
                    tokio_serial::new(path, baud.unwrap_or(115200)).open_native_async()?,
                ));
            }
            AutoConnect => {
                printer = auto_connect().await;
                let msg = match printer {
                    Some(ref mut printer) => {
                        printer_reader =
                            Some(connect_printer(printer.subscribe_lines(), writer.clone()));
                        "Found printer!\n".as_bytes()
                    }
                    None => "Printer not found.\n".as_bytes(),
                };
                writer.write_all(msg).await?;
            }
            Disconnect => {
                printer.take();
                printer_reader.take();
            }
            Help => help(&mut writer).await,
            Version => version(&mut writer).await,
            Clear => {
                writer.write_all(b"Press 'Ctrl+L' to clear\n").await?;
            }
            Unrecognized => {
                writer
                    .write_all(
                        "Invalid command! use ':help' for valid commands and syntax\n".as_bytes(),
                    )
                    .await?;
            }
            Stop(label) => {
                if let Some(task_handle) = background_tasks.remove(label) {
                    task_handle.abort()
                } else {
                    writer
                        .write_all(format!("No task named {label} running\n").as_bytes())
                        .await?;
                }
            }
            Print(filename) => match start_print_file(filename, &printer).await {
                Ok(print_task) => {
                    background_tasks.insert(filename.to_owned(), print_task);
                }
                Err(e) => {
                    writer.write_all(e.to_string().as_bytes()).await?;
                }
            },
            Quit => break,
        };

        readline.add_history_entry(line);
    }

    Ok(())
}
