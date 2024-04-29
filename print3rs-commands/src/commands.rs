use {
    self::{
        connect::Connection,
        log::{get_headers, make_parser, parse_logger, Segment},
    },
    crate::commands::connect::parse_connection,
    core::borrow::Borrow,
    print3rs_core::Socket,
    std::{
        collections::HashMap,
        fmt::Debug,
        sync::{Arc, Mutex},
    },
    winnow::{
        ascii::digit1,
        combinator::terminated,
        stream::{AsChar, Stream},
        token::take_while,
    },
};

use winnow::{
    ascii::{alpha1, space0, space1},
    combinator::{alt, dispatch, empty, fail, opt, preceded, rest, separated},
    prelude::*,
    token::take_till,
};

use tokio::{
    io::{AsyncWriteExt, BufReader},
    net::TcpStream,
    task::JoinHandle,
};

use print3rs_core::{Error as PrinterError, Printer};
use tokio_serial::SerialPortBuilderExt;

pub mod connect;
pub mod help;
pub mod log;
pub mod macros;
pub mod version;

pub fn identifier<'a>(input: &mut &'a str) -> PResult<&'a str> {
    const NAME_CHARS: (
        std::ops::RangeInclusive<char>,
        std::ops::RangeInclusive<char>,
        std::ops::RangeInclusive<char>,
        [char; 3],
    ) = ('a'..='z', 'A'..='Z', '0'..='9', ['-', '_', '.']);
    take_while(1.., NAME_CHARS)
        .verify(|ident| plausible_code.parse(ident).is_err())
        .parse_next(input)
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum Command<S> {
    Gcodes(Vec<S>),
    Print(S),
    Log(S, Vec<Segment<S>>),
    Repeat(S, Vec<S>),
    Tasks,
    Stop(S),
    Connect(Connection<S>),
    Disconnect,
    Macro(S, Vec<S>),
    Macros,
    DeleteMacro(S),
    Help(S),
    Version,
    Clear,
    Quit,
    Unrecognized,
}

impl<S> Command<S> {
    pub fn into_owned(self) -> Command<S::Owned>
    where
        S: ToOwned,
        Segment<S::Owned>: From<Segment<S>>,
    {
        use Command::*;
        match self {
            Gcodes(codes) => Gcodes(
                codes
                    .into_iter()
                    .map(|arg0: S| ToOwned::to_owned(&arg0))
                    .collect(),
            ),
            Print(filename) => Print(filename.to_owned()),
            Log(name, pattern) => Log(
                name.to_owned(),
                pattern.into_iter().map(|s| s.into()).collect(),
            ),
            Repeat(name, codes) => Repeat(
                name.to_owned(),
                codes
                    .into_iter()
                    .map(|arg0: S| ToOwned::to_owned(&arg0))
                    .collect(),
            ),
            Tasks => Tasks,
            Stop(s) => Stop(s.to_owned()),
            Connect(connection) => Connect(connection.into_owned()),
            Disconnect => Disconnect,
            Macro(name, codes) => Macro(
                name.to_owned(),
                codes
                    .into_iter()
                    .map(|arg0: S| ToOwned::to_owned(&arg0))
                    .collect(),
            ),
            Macros => Macros,
            DeleteMacro(s) => DeleteMacro(s.to_owned()),
            Help(s) => Help(s.to_owned()),
            Version => Version,
            Clear => Clear,
            Quit => Quit,
            Unrecognized => Unrecognized,
        }
    }
}

impl<S> Command<S> {
    pub fn to_borrowed<'a, Borrowed: ?Sized>(&'a self) -> Command<&'a Borrowed>
    where
        S: Borrow<Borrowed>,
        Segment<&'a Borrowed>: From<Segment<S>>,
    {
        use Command::*;
        match self {
            Gcodes(codes) => Gcodes(codes.iter().map(|s| s.borrow()).collect()),
            Print(filename) => Print(filename.borrow()),
            Log(name, pattern) => Log(
                name.borrow(),
                pattern.iter().map(|s| s.to_borrowed()).collect(),
            ),
            Repeat(name, codes) => {
                Repeat(name.borrow(), codes.iter().map(|s| s.borrow()).collect())
            }
            Tasks => Tasks,
            Stop(s) => Stop(s.borrow()),
            Connect(connection) => Connect(connection.to_borrowed()),
            Disconnect => Disconnect,
            Macro(name, codes) => Macro(name.borrow(), codes.iter().map(|s| s.borrow()).collect()),
            Macros => Macros,
            DeleteMacro(s) => DeleteMacro(s.borrow()),
            Help(s) => Help(s.borrow()),
            Version => Version,
            Clear => Clear,
            Quit => Quit,
            Unrecognized => Unrecognized,
        }
    }
}

impl<'a> From<Command<&'a str>> for Command<String> {
    fn from(command: Command<&'a str>) -> Self {
        command.into_owned().into()
    }
}

impl<'a> From<&'a Command<String>> for Command<&'a str> {
    fn from(command: &'a Command<String>) -> Self {
        use Command::*;
        match command {
            Gcodes(codes) => Gcodes(codes.iter().map(|s| s.as_str()).collect()),
            Print(filename) => Print(filename.as_str()),
            Log(name, pattern) => Log(name.as_str(), pattern.iter().map(|s| s.into()).collect()),
            Repeat(name, codes) => {
                Repeat(name.as_str(), codes.iter().map(|s| s.as_str()).collect())
            }
            Tasks => Tasks,
            Stop(s) => Stop(s.as_str()),
            Connect(connection) => Connect(connection.to_borrowed()),
            Disconnect => Disconnect,
            Macro(name, codes) => Macro(name.as_str(), codes.iter().map(|s| s.as_str()).collect()),
            Macros => Macros,
            DeleteMacro(s) => DeleteMacro(s.as_str()),
            Help(s) => Help(s.as_str()),
            Version => Version,
            Clear => Clear,
            Quit => Quit,
            Unrecognized => Unrecognized,
        }
    }
}

fn plausible_code<'a>(input: &mut &'a str) -> PResult<&'a str> {
    let checkpoint = input.checkpoint();
    let _ = preceded(space0, (take_while(1, AsChar::is_alpha), digit1)).parse_next(input)?;
    input.reset(&checkpoint);
    take_till(2.., ';').parse_next(input)
}

fn parse_gcodes<'a>(input: &mut &'a str) -> PResult<Vec<&'a str>> {
    terminated(separated(0.., plausible_code, ';'), opt(";")).parse_next(input)
}

fn parse_repeater<'a>(input: &mut &'a str) -> PResult<Command<&'a str>> {
    (preceded(space0, identifier), preceded(space1, parse_gcodes))
        .map(|(name, gcodes)| Command::Repeat(name, gcodes))
        .parse_next(input)
}

fn parse_macro<'a>(input: &mut &'a str) -> PResult<Command<&'a str>> {
    let (name, steps) =
        (preceded(space0, identifier), preceded(space1, parse_gcodes)).parse_next(input)?;
    Ok(Command::Macro(name, steps))
}

fn inner_command<'a>(input: &mut &'a str) -> PResult<Command<&'a str>> {
    dispatch! {preceded(space0, alpha1);
        "log" => parse_logger,
        "repeat" => parse_repeater,
        "print" => preceded(space0, rest).map(Command::Print),
        "tasks" => empty.map(|_| Command::Tasks),
        "stop" => preceded(space0, rest).map(Command::Stop),
        "help" => rest.map(Command::Help),
        "version" => empty.map(|_| Command::Version),
        "disconnect" => empty.map(|_| Command::Disconnect),
        "connect" => parse_connection,
        "macro" => parse_macro,
        "macros" => empty.map(|_| Command::Macros),
        "delmacro" => preceded(space0, rest).map(Command::DeleteMacro),
        "clear" => empty.map(|_| Command::Clear),
        "quit" | "exit" => empty.map(|_| Command::Quit),
        _ => fail
    }
    .parse_next(input)
}

pub fn parse_command<'a>(input: &mut &'a str) -> PResult<Command<&'a str>> {
    alt((
        inner_command,
        parse_gcodes.map(|gcodes| {
            let gcodes = gcodes.into_iter().collect();
            Command::Gcodes(gcodes)
        }),
    ))
    .parse_next(input)
}

pub fn start_print_file(filename: &str, socket: Socket) -> BackgroundTask {
    let filename = filename.to_owned();
    let task: JoinHandle<Result<(), TaskError>> = tokio::spawn(async move {
        if let Ok(file) = tokio::fs::read_to_string(filename).await {
            for line in file.lines() {
                let line = match line.split_once(';') {
                    Some((s, _)) => s,
                    None => line,
                };
                if line.is_empty() {
                    continue;
                };
                socket.send(line).await?.await?;
            }
        }
        Ok(())
    });
    BackgroundTask {
        description: "print",
        abort_handle: task.abort_handle(),
    }
}

#[derive(Debug, thiserror::Error)]
enum TaskError {
    #[error("{0}")]
    Printer(#[from] print3rs_core::Error),
    #[error("failed in background: {0}")]
    Join(#[from] tokio::task::JoinError),
}

pub fn start_logging(
    name: &str,
    pattern: Vec<Segment<&'_ str>>,
    printer: &Printer,
) -> std::result::Result<BackgroundTask, print3rs_core::Error> {
    let filename = format!(
        "{name}_{timestamp}.csv",
        timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    let header = get_headers(&pattern);

    let mut parser = make_parser(pattern);
    let mut log_printer_reader = printer.subscribe_lines()?;
    let log_task_handle = tokio::spawn(async move {
        let mut log_file = tokio::fs::File::create(filename).await.unwrap();
        log_file.write_all(header.as_bytes()).await.unwrap();
        while let Ok(log_line) = log_printer_reader.recv().await {
            if let Ok(parsed) = parser.parse(log_line.as_bytes()) {
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

pub fn start_repeat(gcodes: Vec<String>, socket: Socket) -> BackgroundTask {
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

pub type Tasks = HashMap<String, BackgroundTask>;

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

pub fn send_gcodes(socket: Socket, codes: Vec<String>) -> BackgroundTask {
    let task: JoinHandle<Result<(), PrinterError>> = tokio::spawn(async move {
        for code in codes {
            socket.send_unsequenced(code.as_str()).await?.await?;
        }
        Ok(())
    });
    BackgroundTask {
        description: "gcodes",
        abort_handle: task.abort_handle(),
    }
}

#[derive(Debug, Clone)]
pub enum Response {
    Output(Arc<str>),
    Error(ErrorKindOf),
    AutoConnect(Arc<Mutex<Printer>>),
    Clear,
    Quit,
}

impl From<String> for Response {
    fn from(value: String) -> Self {
        Response::Output(Arc::from(value))
    }
}

impl<'a> From<&'a str> for Response {
    fn from(value: &'a str) -> Self {
        Response::Output(Arc::from(value))
    }
}

impl From<ErrorKindOf> for Response {
    fn from(value: ErrorKindOf) -> Self {
        Response::Error(value)
    }
}

impl From<Printer> for Response {
    fn from(value: Printer) -> Self {
        Response::AutoConnect(Arc::new(Mutex::new(value)))
    }
}

type CommandReceiver = tokio::sync::mpsc::Receiver<Command<String>>;
type ResponseSender = tokio::sync::broadcast::Sender<Response>;
type ResponseReceiver = tokio::sync::broadcast::Receiver<Response>;

#[derive(Debug)]
pub struct Commander {
    printer: Printer,
    pub tasks: Tasks,
    pub macros: macros::Macros,
    responder: ResponseSender,
}
#[derive(Debug, Clone)]
pub struct ErrorKindOf(pub String);

impl<T> From<T> for ErrorKindOf
where
    T: ToString,
{
    fn from(value: T) -> Self {
        Self(value.to_string())
    }
}

impl Default for Commander {
    fn default() -> Self {
        Commander::new()
    }
}

impl Commander {
    pub fn new() -> Self {
        let (responder, _) = tokio::sync::broadcast::channel(32);
        Self {
            printer: Default::default(),
            responder,
            tasks: Default::default(),
            macros: Default::default(),
        }
    }

    pub fn printer(&self) -> &Printer {
        &self.printer
    }

    pub fn set_printer(&mut self, printer: Printer) {
        self.tasks.clear();
        self.printer = printer;
    }

    pub fn subscribe_responses(&self) -> ResponseReceiver {
        self.responder.subscribe()
    }

    fn forward_broadcast(
        mut in_channel: tokio::sync::broadcast::Receiver<Arc<str>>,
        out_channel: tokio::sync::broadcast::Sender<Response>,
    ) {
        tokio::spawn(async move {
            while let Ok(in_message) = in_channel.recv().await {
                out_channel.send(Response::Output(in_message)).unwrap();
            }
        });
    }

    fn add_printer_output_to_responses(&self) {
        if let Ok(print_messages) = self.printer.subscribe_lines() {
            let responder = self.responder.clone();
            Self::forward_broadcast(print_messages, responder);
        }
    }

    pub fn background(mut self, mut commands: CommandReceiver) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                while let Some(command) = commands.recv().await {
                    if let Err(e) = self.dispatch(&command) {
                        let e = e.0;
                        let _ = self.responder.send(format!("Error: {e}").into());
                    }
                }
            }
        })
    }
    pub fn dispatch<'a>(
        &'a mut self,
        command: impl Into<Command<&'a str>>,
    ) -> Result<(), ErrorKindOf> {
        let command = command.into();
        use Command::*;
        match command {
            Clear => {
                self.responder.send(Response::Clear)?;
            }
            Quit => {
                self.responder.send(Response::Quit)?;
            }
            Gcodes(codes) => {
                let socket = self.printer().socket()?.clone();
                let codes = self.macros.expand(codes);
                let task = send_gcodes(socket, codes);
                static COUNTER: std::sync::atomic::AtomicUsize =
                    std::sync::atomic::AtomicUsize::new(0);
                self.tasks.insert(
                    format!(
                        "gcodes_{}",
                        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    ),
                    task,
                );
            }
            Print(filename) => {
                let socket = self.printer.socket()?.clone();
                let print = start_print_file(filename, socket);
                self.tasks.insert(filename.to_string(), print);
            }
            Log(name, pattern) => {
                let log = start_logging(name, pattern, &self.printer)?;
                self.tasks.insert(name.to_string(), log);
            }
            Repeat(name, gcodes) => {
                let socket = self.printer.socket()?.clone();
                let gcodes = self.macros.expand(gcodes);
                let repeat = start_repeat(gcodes, socket);
                self.tasks.insert(name.to_string(), repeat);
            }
            Tasks => {
                for (
                    name,
                    BackgroundTask {
                        description,
                        abort_handle: _,
                    },
                ) in self.tasks.iter()
                {
                    self.responder
                        .send(format!("{name}\t{description}\n").into())?;
                }
            }
            Stop(name) => {
                self.tasks.remove(name);
            }
            Macro(name, commands) => {
                if self.macros.add(name, commands).is_err() {
                    self.responder
                        .send("Infinite macro detected! Macro not added.\n".into())?;
                }
            }
            Macros => {
                for (name, steps) in self.macros.iter() {
                    let steps = steps.join(";");
                    self.responder
                        .send(format!("{name}:    {steps}\n").into())?;
                }
            }
            DeleteMacro(name) => {
                self.macros.remove(name);
            }
            Connect(connection) => {
                self.tasks.clear();
                match connection {
                    Connection::Auto => {
                        self.tasks.clear();
                        self.responder.send("Connecting...\n".into())?;
                        let autoconnect_responder = self.responder.clone();
                        tokio::spawn(async move {
                            let printer = connect::auto_connect().await;
                            let response = if printer.is_connected() {
                                Response::Output("Found Printer!\n".into())
                            } else {
                                Response::Error("No printer found.\n".into())
                            };
                            if let Ok(printer_responses) = printer.subscribe_lines() {
                                let forward_responder = autoconnect_responder.clone();
                                Self::forward_broadcast(printer_responses, forward_responder);
                            }
                            let _ = autoconnect_responder.send(printer.into());
                            let _ = autoconnect_responder.send(response);
                        });
                    }
                    Connection::Serial { port, baud } => {
                        let connection =
                            tokio_serial::new(port, baud.unwrap_or(115200)).open_native_async()?;
                        let connection = BufReader::new(connection);
                        self.tasks.clear();
                        self.printer.connect(connection);
                        self.add_printer_output_to_responses();
                    }
                    Connection::Tcp { hostname, port } => {
                        let addr = if let Some(port) = port {
                            format!("{hostname}:{port}")
                        } else {
                            hostname.to_owned()
                        };
                        let connection = std::net::TcpStream::connect(addr)?;
                        let connection = BufReader::new(TcpStream::from_std(connection)?);
                        self.tasks.clear();
                        self.printer.connect(connection);
                        self.add_printer_output_to_responses();
                    }
                    Connection::Mqtt {
                        hostname,
                        port,
                        in_topic,
                        out_topic,
                    } => todo!(),
                };
            }
            Disconnect => {
                self.tasks.clear();
                self.printer.disconnect()
            }
            Help(subcommand) => {
                self.responder.send(help::help(subcommand).into())?;
            }
            Version => {
                self.responder.send(version::version().into())?;
            }
            _ => {
                self.responder.send("Unsupported command!\n".into())?;
            }
        };
        Ok(())
    }
}
