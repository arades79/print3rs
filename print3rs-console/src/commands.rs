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

    printer.send_raw(b"M115\n").await.ok()?;
    let look_for_ok = tokio::spawn(async move {
        while let Ok(line) = printer.read_next_line().await {
            let sline = String::from_utf8_lossy(&line);
            if sline.to_ascii_lowercase().contains("ok") {
                return Some(printer);
            }
        }
        None
    });

    timeout(Duration::from_secs(10), look_for_ok)
        .await
        .ok()?
        .ok()?
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
Anything entered not matching one of the following commands is uppercased and sent to
the printer for it to interpret.

Some commands cannot be ran until a printer is connected.
Some printers support 'autoconnect', otherwise you will need to connect using the serial port name.

Multiple Gcodes can be sent on the same line by separating with ';'.

Arguments with ? are optional.

Available commands:
help         <command?>       display this message or details for specified command
version                       display version
clear                         clear all text on the screen
printerinfo                   display any information found about the connected printer
print        <file>           send gcodes from file to printer
log          <name> <pattern> begin logging parsed output from printer
repeat       <name> <gcodes>  run the given gcodes in a loop until stop
stop         <name>           stop an active print, log, or repeat
send         <gcodes>         explicitly send commands (split by ;) to printer exactly as typed
connect      <path> <baud?>   connect to a specified serial device at baud (default: 115200)
autoconnect                   attempt to find and connect to a printer
disconnect                    disconnect from printer
debugging    <level>          change the amount of debugging output (default: off)
quit                          exit program
\n";

pub async fn help(writer: &mut rustyline_async::SharedWriter, command: &str) {
    let command = command.trim().strip_prefix(':').unwrap_or(command.trim());
    let msg: &[u8] = match command {
        "send" => b"send: explicitly send one or more commands (separated by gcode comment character `;`) commands to the printer, no uppercasing or additional parsing is performed. This can be used to send commands to the printer that would otherwise be detected as a console command.\n",
        "print" => b"print: execute every line of G-code sequentially from the given file. The print job is added as a task which runs in the background with the filename as the task name. Other commands can be sent while a print is running, and a print can be stopped at any time with `stop`\n",
        "log" => b"log: begin logging the specified pattern from the printer into a csv with the `name` given. This operation runs in the background and is added as a task which can be stopped with `stop`. The pattern given will be used to parse the logs, with values wrapped in `{}` being given a column of whatever is between the `{}`, and pulling a number in its place. If your pattern needs to include a literal `{` or `}`, double them up like `{{` or `}}` to have the parser read it as just a `{` or `}` in the output.\n",
        "repeat" => b"repeat: repeat the given Gcodes (separated by gcode comment character `;`) in a loop until stopped. \n",
        "stop" => b"stop: stops a task running in the background. All background tasks are required to have a name, thus this command can be used to stop them. Tasks can also stop themselves if they fail or can complete, after which running this will do nothing.\n",
        "connect" => b"connect: Manually connect to a printer by specifying it's path and optionally its baudrate. On windows this looks like `connect COM3 115200`, on linux more like `connect /dev/tty/ACM0 250000`. This does not test if the printer is capable of responding to messages, it will only open the port.\n",
        "autoconnect" => b"autoconnect: On some supported printer firmwares, this will automatically detect a connected printer and verify that it's capable of receiving and responding to commands. This is done with an `M115` command sent to the device, and waiting at most 5 seconds for an `ok` response. If your printer does not support this command, this will not work and you will need manual connection.\n",
        "disconnect" => b"disconnect: disconnect from the currently connected printer. All active tasks will be stopped\n",
        "debugging" => b"debugging: control the amount of extra information printed to the screen. By default this is `off`, meaning only strictly necessary messages, sent commands, and printer responses are shown in the console. The levels supported are `off`, `error`, `warn`, `info`, `debug`, `trace`, in increasing levels of verbosity. This is mostly for debugging issues with this console, but could be used to get extra information about when a printer is crashing.\n",
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
    Debugging(&'a str),
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
        "debugging" => preceded(space0, rest).map(Command::Debugging),
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
