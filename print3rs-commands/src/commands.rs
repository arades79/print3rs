use std::{borrow::Cow, collections::HashMap, time::Duration};

use winnow::{
    ascii::{alpha1, alphanumeric1, dec_uint, space0, space1},
    combinator::{alt, dispatch, empty, fail, opt, preceded, rest, separated},
    prelude::*,
    token::take_till,
};

use tokio::{io::AsyncWriteExt, task::JoinHandle, time::timeout};

use print3rs_core::{AsyncPrinterComm, Error as PrinterError, Printer, SerialPrinter};
use tokio_serial::{available_ports, SerialPort, SerialPortBuilderExt, SerialPortInfo};

async fn check_port(port: SerialPortInfo) -> Option<SerialPrinter> {
    tracing::debug!("checking port {}...", port.port_name);
    let mut printer_port = tokio_serial::new(port.port_name, 115200)
        .timeout(Duration::from_secs(10))
        .open_native_async()
        .ok()?;
    printer_port.write_data_terminal_ready(true).ok()?;
    let mut printer = SerialPrinter::new(printer_port);

    printer.send_raw(b"M115\n").ok()?;
    let look_for_ok = async {
        while let Ok(line) = printer.read_next_line().await {
            let sline = String::from_utf8_lossy(&line);
            if sline.to_ascii_lowercase().contains("ok") {
                return Some(printer);
            }
        }
        None
    };

    timeout(Duration::from_secs(5), look_for_ok).await.ok()?
}

pub async fn auto_connect() -> SerialPrinter {
    if let Ok(ports) = available_ports() {
        tracing::info!("found available ports: {ports:?}");
        for port in ports {
            if let Some(printer) = check_port(port).await {
                return printer;
            }
        }
    }
    Printer::Disconnected
}

pub fn version() -> &'static str {
    const VERSION: Option<&str> = option_env!("CARGO_PKG_VERSION");
    VERSION.unwrap_or("???")
}

static FULL_HELP: &str = "    
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
quit                          exit program
\n";

pub fn help(command: &str) -> &'static str {
    let command = command.trim().strip_prefix(':').unwrap_or(command.trim());
    match command {
        "send" => "send: explicitly send one or more commands (separated by gcode comment character `;`) commands to the printer, no uppercasing or additional parsing is performed. This can be used to send commands to the printer that would otherwise be detected as a console command.\n",
        "print" => "print: execute every line of G-code sequentially from the given file. The print job is added as a task which runs in the background with the filename as the task name. Other commands can be sent while a print is running, and a print can be stopped at any time with `stop`\n",
        "log" => "log: begin logging the specified pattern from the printer into a csv with the `name` given. This operation runs in the background and is added as a task which can be stopped with `stop`. The pattern given will be used to parse the logs, with values wrapped in `{}` being given a column of whatever is between the `{}`, and pulling a number in its place. If your pattern needs to include a literal `{` or `}`, double them up like `{{` or `}}` to have the parser read it as just a `{` or `}` in the output.\n",
        "repeat" => "repeat: repeat the given Gcodes (separated by gcode comment character `;`) in a loop until stopped. \n",
        "stop" => "stop: stops a task running in the background. All background tasks are required to have a name, thus this command can be used to stop them. Tasks can also stop themselves if they fail or can complete, after which running this will do nothing.\n",
        "connect" => "connect: Manually connect to a printer by specifying its path and optionally its baudrate. On windows this looks like `connect COM3 115200`, on linux more like `connect /dev/tty/ACM0 250000`. This does not test if the printer is capable of responding to messages, it will only open the port.\n",
        "autoconnect" => "autoconnect: On some supported printer firmwares, this will automatically detect a connected printer and verify that it's capable of receiving and responding to commands. This is done with an `M115` command sent to the device, and waiting at most 5 seconds for an `ok` response. If your printer does not support this command, this will not work and you will need manual connection.\n",
        "disconnect" => "disconnect: disconnect from the currently connected printer. All active tasks will be stopped\n",
        _ => FULL_HELP,
    }
}

use crate::logging::parsing::{parse_logger, Segment};

#[non_exhaustive]
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
    Macro(&'a str, Vec<Cow<'a, str>>),
    Macros,
    DeleteMacro(&'a str),
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
        preceded(space0, alphanumeric1),
        preceded(space1, parse_gcodes),
    )
        .map(|(name, gcodes)| Command::Repeat(name, gcodes))
        .parse_next(input)
}

fn parse_macro<'a>(input: &mut &'a str) -> PResult<Command<'a>> {
    let alpha_no_reserved_start =
        alpha1.verify(|s: &str| !s.starts_with(|c: char| "GTMND".contains(c.to_ascii_uppercase())));
    let (name, steps) = (
        preceded(space0, alpha_no_reserved_start),
        preceded(space1, parse_gcodes),
    )
        .parse_next(input)?;
    Ok(Command::Macro(name, steps))
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
        "macro" => parse_macro,
        "macros" => empty.map(|_| Command::Macros),
        "delmacro" => preceded(space0, rest).map(Command::DeleteMacro),
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

pub fn start_print_file<Transport>(
    filename: &str,
    printer: &Printer<Transport>,
) -> std::result::Result<BackgroundTask, print3rs_core::Error> {
    let filename = filename.to_owned();
    let socket = printer.socket()?.clone();
    let task: JoinHandle<Result<(), TaskError>> = tokio::spawn(async move {
        if let Ok(file) = std::fs::read_to_string(filename) {
            for line in file.lines() {
                socket.send(line).await?.await?;
            }
        }
        Ok(())
    });
    Ok(BackgroundTask {
        description: "print",
        abort_handle: task.abort_handle(),
    })
}

#[derive(Debug, thiserror::Error)]
enum TaskError {
    #[error("{0}")]
    Printer(#[from] print3rs_core::Error),
    #[error("failed in background: {0}")]
    Join(#[from] tokio::task::JoinError),
}

pub fn start_logging<Transport>(
    name: &str,
    pattern: Vec<crate::logging::parsing::Segment<'_>>,
    printer: &Printer<Transport>,
) -> std::result::Result<BackgroundTask, print3rs_core::Error> {
    let filename = format!(
        "{name}_{timestamp}.csv",
        timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    let header = crate::logging::parsing::get_headers(&pattern);

    let mut parser = crate::logging::parsing::make_parser(pattern);
    let mut log_printer_reader = printer.subscribe_lines()?;
    let log_task_handle = tokio::spawn(async move {
        let mut log_file = tokio::fs::File::create(filename).await.unwrap();
        log_file.write_all(header.as_bytes()).await.unwrap();
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
    Ok(BackgroundTask {
        description: "log",
        abort_handle: log_task_handle.abort_handle(),
    })
}

pub fn start_repeat(gcodes: Vec<Cow<'_, str>>, socket: print3rs_core::Socket) -> BackgroundTask {
    let gcodes: Vec<String> = gcodes.into_iter().map(|s| s.into_owned()).collect();

    let task: JoinHandle<Result<(), TaskError>> = tokio::spawn(async move {
        for ref line in gcodes.into_iter().cycle() {
            socket.send(line).await?.await?;
        }
        Ok(())
    });
    BackgroundTask {
        description: "repeat",
        abort_handle: task.abort_handle(),
    }
}

#[derive(Debug)]
pub struct BackgroundTask {
    pub description: &'static str,
    pub abort_handle: tokio::task::AbortHandle,
}

impl Drop for BackgroundTask {
    fn drop(&mut self) {
        self.abort_handle.abort()
    }
}

pub trait HandleCommand {
    type Error: From<PrinterError> + From<tokio_serial::Error>;
    fn printer(&self) -> &SerialPrinter;
    fn on_connect(&self, printer: SerialPrinter);
    fn respond(&self, message: &str);
    fn error(&self, err: Self::Error);
    fn add_task(&self, task_name: &str, task: BackgroundTask);
    fn remove_task(&self, task_name: &str);
    fn task_iter(&self) -> impl Iterator<Item = (&String, &BackgroundTask)>;
}

pub fn send_gcodes(
    printer: &impl AsyncPrinterComm,
    codes: impl IntoIterator<Item = impl AsRef<str>>,
    macros: Option<&HashMap<String, Vec<String>>>,
) -> Result<(), PrinterError> {
    match macros {
        Some(macros) => {
            fn expand(
                printer: &impl AsyncPrinterComm,
                code: &str,
                macros: &HashMap<String, Vec<String>>,
            ) -> Result<(), PrinterError> {
                match macros.get(code) {
                    Some(expansion) => {
                        for extra in expansion {
                            expand(printer, extra, macros)?;
                        }
                        Ok(())
                    }
                    None => printer.send_unsequenced(code),
                }
            }
            for code in codes {
                expand(printer, code.as_ref(), macros)?;
            }
        }
        None => {
            for line in codes {
                printer.send_unsequenced(line.as_ref())?;
            }
        }
    }

    Ok(())
}

// fn handle_command(handler: &impl HandleCommand, command: Command<'_>) {
//     use Command::*;
//     match command {
//         Gcodes(gcodes) => {
//             send_gcodes(handler.printer(), &gcodes);
//         }
//         Log(name, pattern) => {
//             match start_logging(name, pattern, handler.printer()) {
//                 Ok(log_task_handle) => {
//                     handler.add_task(
//                         name,
//                         BackgroundTask {
//                             description: "log",
//                             abort_handle: log_task_handle.abort_handle(),
//                         },
//                     );
//                 }
//                 Err(e) => handler.error(e.into()),
//             };
//         }
//         Repeat(name, gcodes) => {
//             let repeat_task = start_repeat(gcodes, handler.printer());

//             handler.add_task(
//                 name,
//                 BackgroundTask {
//                     description: "repeat",
//                     abort_handle: repeat_task.abort_handle(),
//                 },
//             );
//         }
//         Connect(path, baud) => {
//             match tokio_serial::new(path, baud.unwrap_or(115200)).open_native_async() {
//                 Ok(port) => {
//                     handler.on_connect(Printer::new(port));
//                 }
//                 Err(e) => handler.error(e.into()),
//             };
//         }
//         AutoConnect => {
//             handler.respond("Connecting...\n");
//         }
//         Disconnect => {
//             handler.on_connect(Printer::Disconnected);
//         }
//         Help(sub) => handler.respond(help(sub)),
//         Version => handler.respond(version()),
//         Unrecognized => {
//             handler.respond("Invalid command! use ':help' for valid commands and syntax\n");
//         }
//         Tasks => {
//             for (
//                 name,
//                 BackgroundTask {
//                     description,
//                     abort_handle: _,
//                 },
//             ) in handler.task_iter()
//             {
//                 handler.respond(format!("{name}\t{description}\n").as_str());
//             }
//         }
//         Stop(label) => handler.remove_task(label),
//         Print(filename) => match start_print_file(filename, handler.printer()) {
//             Ok(print_task) => {
//                 handler.add_task(
//                     filename,
//                     BackgroundTask {
//                         description: "print",
//                         abort_handle: print_task.abort_handle(),
//                     },
//                 );
//             }
//             Err(e) => {
//                 handler.error(e.into());
//             }
//         },
//         Clear => (), // needs external handling
//         Quit => (),  // needs external handling
//     };
// }
