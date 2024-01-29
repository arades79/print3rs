mod commands;
mod logging;

use commands::{auto_connect, help};
use futures_util::AsyncWriteExt;
use rustyline_async::{Readline, ReadlineEvent, SharedWriter};
use tokio_serial::SerialPortBuilderExt;
use tracing;
use winnow::Parser;

use print3rs_core::{Printer, PrinterLines};

fn connect_printer(
    mut printer_lines: PrinterLines,
    mut print_line_writer: SharedWriter,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_local(async move {
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

    while let ReadlineEvent::Line(line) = readline.readline().await? {
        match commands::parse_command.parse(&line) {
            Ok(command) => match command {
                commands::Command::Gcodes(gcodes) => {
                    if let Some(ref mut printer) = printer {
                        for line in gcodes {
                            printer.send_raw(line.as_bytes()).await?;
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
                commands::Command::Log(_, _) => todo!(),
                commands::Command::Repeat(_, _) => todo!(),
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
                commands::Command::Clear => todo!(),
                commands::Command::Unrecognized => {
                    writer
                        .write(
                            "Invalid command! use ':help' for valid commands and syntax\n"
                                .as_bytes(),
                        )
                        .await?;
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
