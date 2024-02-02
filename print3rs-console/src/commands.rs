use std::{borrow::Cow, time::Duration};

use futures_util::AsyncWriteExt;
use winnow::{
    ascii::{alpha1, alphanumeric1, dec_uint, space0, space1},
    combinator::{alt, dispatch, empty, fail, opt, preceded, rest, separated},
    prelude::*,
    token::take_till,
};

use tokio::time::timeout;

use print3rs_core::Printer;
use tokio_serial::{available_ports, SerialPort, SerialPortBuilderExt, SerialPortInfo};

async fn check_port(port: SerialPortInfo) -> Option<Printer> {
    tracing::debug!("checking port {}...", port.port_name);
    let mut printer_port = tokio_serial::new(port.port_name, 115200)
        .timeout(std::time::Duration::from_secs(10))
        .open_native_async()
        .ok()?;
    printer_port.write_data_terminal_ready(true).ok()?;
    let mut printer = Printer::new(printer_port);
    let mut reader = printer.subscribe_lines();
    let look_for_ok = tokio::spawn(async move {
        while let Ok(line) = reader.recv().await {
            let sline = String::from_utf8_lossy(&line);
            if sline.to_ascii_lowercase().contains("ok") {
                return;
            }
        }
    });
    printer.send_raw(b"M115\n").await.ok()?;
    timeout(Duration::from_secs(10), look_for_ok)
        .await
        .ok()?
        .ok()?;
    Some(printer)
}

pub async fn auto_connect() -> Option<Printer> {
    let ports = available_ports().ok()?;
    tracing::info!("found available ports: {ports:?}");
    for port in ports {
        if let Some(printer) = check_port(port).await {
            return Some(printer);
        }
    }
    None
}

pub async fn version(writer: &mut rustyline_async::SharedWriter) {
    const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");

    writer
        .write_all(
            format!(
                "print3rs-console version {ver}\n",
                ver = VERSION.unwrap_or("???")
            )
            .as_bytes(),
        )
        .await
        .unwrap_or(());
}

static FULL_HELP: &[u8] = b"    
commands can be explicitly invoked with ':', e.g. ':log timelogger millis:{{millis}}'
if ':' is not used, an unrecognized command is uppercased and sent to the connected printer.

example: `> g28` will send `G28` to the printer. 

Some commands, including Gcodes, cannot be ran until a printer is connected.
Some printers support 'autoconnect', otherwise you will need to connect using the serial port name.

Multiple Gcodes can be sent on the same line by separating with ';'.

Arguments with ? are optional.

Available commands:
help    <command?>       display this message or details for specified command
version                  display version
print   <file>           send gcodes from file to printer
log     <name> <pattern> begin logging parsed output from printer
repeat  <name> <gcodes>  run the given gcodes in a loop until stop
stop    <name>           stop an active print, log, or repeat
send    <gcodes>         explicitly send commands (split by ;) to printer exactly as typed
connect <path> <baud?>   connect to a specified serial device at baud (default: 115200)
autoconnect              attempt to find and connect to a printer
disconnect               disconnect from printer
quit                     exit program
\n";

pub async fn help(writer: &mut rustyline_async::SharedWriter, command: &str) {
    let command = command.trim().strip_prefix(":").unwrap_or(command.trim());
    let msg: &[u8] = match command {
        "send" => b"send    <gcodes>         explicitly send commands (split by ;) to printer exactly as typed\n",
        "print" => b"print\n",
        "log" => b"log\n",
        "repeat" => b"repeat\n",
        "stop" => b"stop\n",
        "connect" => b"connect\n",
        "autoconnect" => b"autoconnect\n",
        "disconnect" => b"disconnect\n",
        _ => FULL_HELP,
    };
    writer.write_all(msg).await.unwrap_or_default();
}

use crate::logging::parsing::{parse_logger, Segment};

#[derive(Debug)]
pub enum Command<'a> {
    Gcodes(Vec<Cow<'a, str>>),
    Print(&'a str),
    Log(&'a str, Vec<Segment<'a>>),
    Repeat(&'a str, Vec<Cow<'a, str>>),
    Tasks,
    Stop(&'a str),
    Connect(&'a str, Option<u32>),
    AutoConnect,
    Disconnect,
    Help(&'a str),
    Version,
    Clear,
    Quit,
    Unrecognized,
}

fn parse_gcodes<'a>(input: &mut &'a str) -> PResult<Vec<Cow<'a, str>>> {
    separated(0.., take_till(1.., ';').map(Cow::Borrowed), ';').parse_next(input)
}

fn parse_repeater<'a>(input: &mut &'a str) -> PResult<Command<'a>> {
    (
        preceded(space1, alphanumeric1),
        preceded(space1, parse_gcodes),
    )
        .map(|(name, gcodes)| Command::Repeat(name, gcodes))
        .parse_next(input)
}

fn inner_command<'a>(input: &mut &'a str) -> PResult<Command<'a>> {
    let explicit = opt(":").parse_next(input)?;
    let command = opt(dispatch! {alpha1;
        "log" => parse_logger,
        "repeat" => parse_repeater,
        "print" => preceded(space0, rest).map(Command::Print),
        "tasks" => empty.map(|_| Command::Tasks),
        "stop" => preceded(space0, rest).map(Command::Stop),
        "help" => rest.map(Command::Help),
        "version" => empty.map(|_| Command::Version),
        "autoconnect" => empty.map(|_| Command::AutoConnect),
        "disconnect" => empty.map(|_| Command::Disconnect),
        "connect" => (preceded(space0, take_till(1.., [' '])), preceded(space0,opt(dec_uint))).map(|(path, baud)| Command::Connect(path, baud)),
        "send" => preceded(space0, parse_gcodes).map(Command::Gcodes),
        "clear" => empty.map(|_| Command::Clear),
        "quit" | "exit" => empty.map(|_| Command::Quit),
        _ => empty.map(|_| Command::Unrecognized)
    })
    .parse_next(input)?;
    match (explicit, command) {
        (None, Some(Command::Unrecognized)) => fail.parse_next(input),
        (_, None) => Ok(Command::Unrecognized),
        (_, Some(command)) => Ok(command),
    }
}

pub fn parse_command<'a>(input: &mut &'a str) -> PResult<Command<'a>> {
    alt((
        inner_command,
        parse_gcodes.map(|gcodes| {
            let gcodes = gcodes
                .into_iter()
                .map(|s| Cow::Owned(s.to_ascii_uppercase()))
                .collect();
            Command::Gcodes(gcodes)
        }),
    ))
    .parse_next(input)
}
