use core::time;
use std::time::Duration;

use futures_util::AsyncWriteExt;
use winnow::{
    ascii::{alpha1, alphanumeric1, dec_uint, space0, space1},
    combinator::{alt, dispatch, empty, fail, opt, preceded, rest, separated},
    prelude::*,
    token::{any, take_till, take_while},
};

use tokio::time::timeout;

use print3rs_core::Printer;
use tokio_serial::{available_ports, SerialPort, SerialPortBuilderExt, SerialStream};

pub async fn auto_connect() -> Option<Printer> {
    let ports = available_ports().ok()?;
    tracing::info!("found available ports: {ports:?}");
    for port in ports {
        tracing::debug!("checking port {}...", port.port_name);
        let mut printer_port = tokio_serial::new(port.port_name, 115200)
            .timeout(std::time::Duration::from_secs(10))
            .open_native_async()
            .ok()?;
        printer_port.write_data_terminal_ready(true).ok()?;
        let mut printer = Printer::new(printer_port);
        timeout(Duration::from_secs(5), printer.send("M115").await.ok()?)
            .await
            .ok()?
            .ok()?;
        return Some(printer);
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

pub async fn help(writer: &mut rustyline_async::SharedWriter) {
    writer.write_all(b"    
    commands can be explicitly invoked with ':', e.g. ':log timelogger millis:{{millis}}'
    if ':' is not used, an unrecognized command is sent to the connected printer as a Gcode.

    Some commands, including Gcodes, cannot be ran until a printer is connected.
    Some printers support 'autoconnect', otherwise you will need to connect using the serial port name.

    Multiple Gcodes can be sent on the same line by separating with ';'.

    Arguments with ? are optional.

    Available commands:
    help
    version
    print <file>
    log <name> <pattern>
    repeat <name> <gcodes>
    stop <name>
    connect <path> <baud?>
    autoconnect
    disconnect
    \n"
    ).await.unwrap();
}

use crate::logging::parsing::{parse_logger, Segment};

#[derive(Debug)]
pub enum Command<'a> {
    Gcodes(Vec<&'a str>),
    Print(&'a str),
    Log(&'a str, Vec<Segment<'a>>),
    Repeat(&'a str, Vec<&'a str>),
    Stop(&'a str),
    Connect(&'a str, Option<u32>),
    AutoConnect,
    Disconnect,
    Help,
    Version,
    Clear,
    Unrecognized,
}

fn parse_gcodes<'a>(input: &mut &'a str) -> PResult<Vec<&'a str>> {
    separated(0.., take_till(1.., ';'), ';').parse_next(input)
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
        "print" => preceded(space0, rest).map(|s| Command::Print(s)),
        "stop" => preceded(space0, rest).map(|s| Command::Stop(s)),
        "help" => empty.map(|_| Command::Help),
        "version" => empty.map(|_| Command::Version),
        "autoconnect" => empty.map(|_| Command::AutoConnect),
        "disconnect" => empty.map(|_| Command::Disconnect),
        "connect" => (take_till(1.., [' ']), opt(dec_uint)).map(|(path, baud)| Command::Connect(path, baud)),
        "send" => preceded(space0, parse_gcodes).map(|gcodes| Command::Gcodes(gcodes)),
        "clear" => empty.map(|_| Command::Clear),
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
        parse_gcodes.map(|gcodes| Command::Gcodes(gcodes)),
    ))
    .parse_next(input)
}
