mod commands;
mod logging;

use std::collections::HashMap;

use commands::{auto_connect, help, version};
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
            print_line_writer.write(&line).await.unwrap_or(0);
        }
    })
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> eyre::Result<()> {
    let (mut readline, mut writer) = Readline::new(String::from("> "))?;
    let mut printer: Option<Printer> = None;
    let mut printer_reader = None;

    let mut background_tasks = HashMap::new();

    let tracing_writer = writer.clone();
    let _tracer = tracing_subscriber::FmtSubscriber::builder()
        .with_writer(move || tracing_writer.clone())
        .pretty()
        .finish();

    while let ReadlineEvent::Line(line) = readline.readline().await? {
        match commands::parse_command.parse(&line) {
            Ok(command) => match command {
                commands::Command::Gcodes(gcodes) => {
                    if let Some(ref mut printer) = printer {
                        for line in gcodes {
                            printer.send_unsequenced(line.as_bytes()).await?;
                        }
                    } else {
                        writer
                            .write_all(
                                "No printer connected! Use ':help' for help connecting.\n"
                                    .as_bytes(),
                            )
                            .await?
                    };
                }
                commands::Command::Log(name, pattern) => {
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
                    if let Some(ref temp_printer) = printer {
                        let mut log_printer_reader = temp_printer.subscribe_lines();
                        let log_task_handle = tokio::spawn(async move {
                            while let Ok(log_line) = log_printer_reader.recv().await {
                                if let Ok(parsed) = parser.parse(&log_line) {
                                    let mut record_bytes = Vec::new();
                                    for val in parsed {
                                        record_bytes.extend_from_slice(
                                            ryu::Buffer::new().format(val).as_bytes(),
                                        );
                                        record_bytes.push(b',');
                                    }
                                    record_bytes.pop(); // remove trailing ','
                                    record_bytes.push(b'\n');
                                    log_file.write_all(&record_bytes).await.unwrap_or(());
                                }
                            }
                        });
                        background_tasks.insert(name.to_owned(), log_task_handle);
                    };
                }
                commands::Command::Repeat(name, gcodes) => {
                    if printer.is_none() {
                        writer.write(b"Printer not connected!\n").await?;
                        continue;
                    }
                    let gcodes: Vec<_> = gcodes.into_iter().map(|s| s.to_owned()).collect();
                    let printer_sender = printer.as_mut().unwrap().get_sender();
                    let printer_reader = printer.as_mut().unwrap().subscribe_lines();
                    let mut serializer = gcode_serializer::Serializer::default();
                    let repeat_task = tokio::spawn(async move {
                        for line in gcodes.iter().cycle() {
                            printer_sender
                                .send(serializer.serialize(line))
                                .await
                                .unwrap_or(());
                            print3rs_core::search_for_sequence(
                                serializer.sequence(),
                                printer_reader.resubscribe(),
                            )
                            .await;
                        }
                    });
                    background_tasks.insert(name.to_owned(), repeat_task);
                }
                commands::Command::Connect(path, baud) => {
                    let _ = printer.insert(Printer::new(
                        tokio_serial::new(path, baud.unwrap_or(115200)).open_native_async()?,
                    ));
                }
                commands::Command::AutoConnect => {
                    printer = auto_connect().await;
                    let msg = match printer {
                        Some(ref mut printer) => {
                            let _ = printer_reader
                                .insert(connect_printer(printer.subscribe_lines(), writer.clone()));
                            "Found printer!\n".as_bytes()
                        }
                        None => "Printer not found.\n".as_bytes(),
                    };
                    writer.write(msg).await?;
                }
                commands::Command::Disconnect => {
                    printer.take();
                    printer_reader.take();
                }
                commands::Command::Help => help(&mut writer).await,
                commands::Command::Version => version(&mut writer).await,
                commands::Command::Clear => {
                    writer.write(b"Press 'Ctrl+L' to clear\n").await?;
                }
                commands::Command::Unrecognized => {
                    writer
                        .write(
                            "Invalid command! use ':help' for valid commands and syntax\n"
                                .as_bytes(),
                        )
                        .await?;
                }
                commands::Command::Stop(label) => {
                    if let Some(task_handle) = background_tasks.remove(label) {
                        task_handle.abort()
                    } else {
                        writer
                            .write(format!("No task named {label} running\n").as_bytes())
                            .await?;
                    }
                }
                commands::Command::Print(filename) => {
                    if printer.is_none() {
                        writer.write(b"Printer not connected!\n").await?;
                        continue;
                    }

                    if let Ok(mut file) = tokio::fs::File::open(filename).await {
                        let mut file_contents = String::new();
                        file.read_to_string(&mut file_contents).await?;
                        let printer_sender = printer.as_mut().unwrap().get_sender();
                        let printer_reader = printer.as_mut().unwrap().subscribe_lines();
                        let mut serializer = gcode_serializer::Serializer::default();
                        let print_task = tokio::spawn(async move {
                            for line in file_contents.lines() {
                                printer_sender
                                    .send(serializer.serialize(line))
                                    .await
                                    .unwrap_or(());
                                print3rs_core::search_for_sequence(
                                    serializer.sequence(),
                                    printer_reader.resubscribe(),
                                )
                                .await;
                            }
                        });
                        background_tasks.insert(filename.to_owned(), print_task);
                    } else {
                        writer.write(b"File not found!\n").await?;
                    }
                }
            },
            Err(e) => {
                writer
                    .write(format!("invalid command! Error: {e:?}\n").as_bytes())
                    .await?;
            }
        };
        readline.add_history_entry(line);
    }

    Ok(())
}
