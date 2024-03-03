use {
    print3rs_commands::commands::Macros, print3rs_core::SerialPrinter, std::collections::HashMap,
};

use iced::widget::combo_box::State as ComboState;
use iced::widget::{button, column, combo_box, row, scrollable, text, text_input};
use iced::{Application, Command, Length};
use print3rs_commands::commands::BackgroundTask;
use print3rs_core::AsyncPrinterComm;
use tokio_serial::{available_ports, SerialPortBuilderExt};

use winnow::prelude::*;

#[derive(Debug, Clone)]
struct ErrorKindOf(String);

impl<T> From<T> for ErrorKindOf
where
    T: ToString,
{
    fn from(value: T) -> Self {
        Self(value.to_string())
    }
}

#[derive(Debug)]
struct App {
    ports: ComboState<String>,
    selected_port: Option<String>,
    printer: print3rs_core::SerialPrinter,
    bauds: ComboState<u32>,
    selected_baud: Option<u32>,
    command: String,
    output: String,
    tasks: HashMap<String, BackgroundTask>,
    macros: Macros,
}

#[derive(Debug, Clone, Default)]
struct JogMove {
    x: f32,
    y: f32,
    z: f32,
}

impl JogMove {
    fn x(x: f32) -> Self {
        Self {
            x,
            ..Default::default()
        }
    }
    fn y(y: f32) -> Self {
        Self {
            y,
            ..Default::default()
        }
    }
    fn z(z: f32) -> Self {
        Self {
            z,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Jog(JogMove),
    ChangePort(String),
    ChangeBaud(u32),
    ToggleConnect,
    //Connected(SerialPrinter),
    CommandInput(String),
    ProcessCommand,
}

impl iced::Application for App {
    type Executor = iced::executor::Default;

    type Message = Message;

    type Theme = iced::Theme;

    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        let mut ports: Vec<String> = available_ports()
            .unwrap_or_default()
            .into_iter()
            .map(|port| port.port_name)
            .collect();
        ports.push("auto".to_string());
        (
            Self {
                ports: ComboState::new(ports),
                selected_port: None,
                bauds: ComboState::new(vec![2400, 9600, 19200, 38400, 57600, 115200, 250000]),
                selected_baud: Some(115200),
                printer: Default::default(),
                command: Default::default(),
                output: Default::default(),
                tasks: Default::default(),
                macros: Default::default(),
            },
            iced::Command::none(),
        )
    }

    fn title(&self) -> String {
        "Print3rs".to_string()
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        match message {
            Message::Jog(JogMove { x, y, z }) => {
                let _ = self.printer.send_unsequenced(format!("G7X{x}Y{y}Z{z}"));
                Command::none()
            }
            Message::ToggleConnect => {
                if self.printer.is_connected() {
                    self.printer.disconnect();
                } else if let Some(ref port) = self.selected_port {
                    if port == "auto" {
                        // this is where it would auto connect
                    }
                    if let Some(baud) = self.selected_baud {
                        self.printer
                            .connect(tokio_serial::new(port, baud).open_native_async().unwrap());
                    }
                }

                Command::none()
            }
            Message::CommandInput(s) => {
                self.command = s;
                Command::none()
            }
            Message::ProcessCommand => {
                if let Ok(command) = print3rs_commands::commands::parse_command.parse(&self.command)
                {
                    use print3rs_commands::commands;
                    use print3rs_commands::commands::Command::*;
                    const DISCONNECTED_ERROR: &str = "No printer connected!\n";
                    match command {
                        Clear => {
                            self.output.clear();
                        }
                        Quit => {
                            todo!()
                        }
                        Gcodes(codes) => {
                            let codes = self.macros.expand(codes);
                            if let Err(_e) = commands::send_gcodes(&self.printer, codes) {
                                self.output.push_str(DISCONNECTED_ERROR);
                            }
                        }
                        Print(filename) => {
                            if let Ok(print) = commands::start_print_file(filename, &self.printer) {
                                self.tasks.insert(filename.to_string(), print);
                            } else {
                                self.output.push_str(DISCONNECTED_ERROR);
                            }
                        }
                        Log(name, pattern) => {
                            if let Ok(log) = commands::start_logging(name, pattern, &self.printer) {
                                self.tasks.insert(name.to_string(), log);
                            } else {
                                self.output.push_str(DISCONNECTED_ERROR);
                            }
                        }
                        Repeat(name, gcodes) => {
                            if let Ok(socket) = self.printer.socket() {
                                let gcodes = self.macros.expand(gcodes);
                                let repeat = commands::start_repeat(gcodes, socket.clone());
                                self.tasks.insert(name.to_string(), repeat);
                            } else {
                                self.output.push_str(DISCONNECTED_ERROR);
                            }
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
                                self.output
                                    .push_str(format!("{name}\t{description}\n").as_str());
                            }
                        }
                        Stop(name) => {
                            self.tasks.remove(name);
                        }
                        Macro(name, steps) => {
                            if self.macros.add(name, steps).is_err() {
                                self.output.push_str(
                                    "Infinite recursion detected for macro! Macro was not added.\n",
                                )
                            }
                        }
                        Macros => {
                            for (name, steps) in self.macros.iter() {
                                self.output.push_str(name);
                                self.output.push_str("        ");
                                self.output.push_str(&steps.join(";"));
                                self.output.push('\n');
                            }
                        }
                        DeleteMacro(name) => {
                            self.macros.remove(name);
                        }
                        Connect(path, baud) => {
                            if let Ok(port) =
                                tokio_serial::new(path, baud.unwrap_or(115200)).open_native_async()
                            {
                                self.printer.connect(port);
                            } else {
                                self.output.push_str("Connection failed.\n");
                            }
                        }
                        AutoConnect => {
                            self.output.push_str("Connecting...\n");
                            todo!();
                            // self.printer = commands::auto_connect().await;
                            // self.output.push_str(if self.printer.is_connected() {
                            //     "Found printer!\n"
                            // } else {
                            //     "No printer found.\n"
                            // });
                        }
                        Disconnect => self.printer.disconnect(),
                        Help(subcommand) => self.output.push_str(commands::help(subcommand)),
                        Version => self.output.push_str(commands::version()),
                        Unrecognized => self.output.push_str("Unrecognized command!\n"),
                        _ => (),
                    };
                    //self.output.push_str(&self.command);
                    self.command.clear();
                }
                Command::none()
            }
            // Message::Connected(printer) => {
            //     self.printer = printer;
            // }
            Message::ChangePort(port) => {
                self.selected_port = Some(port);
                Command::none()
            }
            Message::ChangeBaud(baud) => {
                self.selected_baud = Some(baud);
                Command::none()
            }
        }
    }

    fn view(&self) -> iced::Element<'_, Self::Message, Self::Theme, iced::Renderer> {
        let port_list = combo_box(
            &self.ports,
            "printer port",
            self.selected_port.as_ref(),
            Message::ChangePort,
        )
        .width(Length::FillPortion(5))
        .on_input(Message::ChangePort);
        let baud_list = combo_box(
            &self.bauds,
            "baudrate",
            self.selected_baud.as_ref(),
            Message::ChangeBaud,
        )
        .width(Length::FillPortion(1))
        .on_input(|s| Message::ChangeBaud(s.parse().unwrap_or_default()));
        let maybe_jog = |jogmove| self.printer.is_connected().then_some(Message::Jog(jogmove));
        column![
            row![
                port_list,
                baud_list,
                button(if self.printer.is_connected() {
                    "disconnect"
                } else {
                    "connect"
                })
                .on_press(Message::ToggleConnect)
            ],
            row![
                column![
                    button("Y+100.0").on_press_maybe(maybe_jog(JogMove::y(100.0))),
                    button("Y+10.0").on_press_maybe(maybe_jog(JogMove::y(10.0))),
                    button("Y+1.0").on_press_maybe(maybe_jog(JogMove::y(1.0))),
                    row![
                        button("X-100.0").on_press_maybe(maybe_jog(JogMove::x(-100.0))),
                        button("X-10.0").on_press_maybe(maybe_jog(JogMove::x(-10.0))),
                        button("X-1.0").on_press_maybe(maybe_jog(JogMove::x(-1.0))),
                        button("X+1.0").on_press_maybe(maybe_jog(JogMove::x(1.0))),
                        button("X+10.0").on_press_maybe(maybe_jog(JogMove::x(10.0))),
                        button("X+100.0").on_press_maybe(maybe_jog(JogMove::x(100.0)))
                    ],
                    button("Y-1.0").on_press_maybe(maybe_jog(JogMove::y(-1.0))),
                    button("Y-10.0").on_press_maybe(maybe_jog(JogMove::y(-10.0))),
                    button("Y-100.0").on_press_maybe(maybe_jog(JogMove::y(-100.0))),
                ]
                .align_items(iced::Alignment::Center),
                column![
                    button("Z+10.0").on_press_maybe(maybe_jog(JogMove::z(-10.0))),
                    button("Z+1.0").on_press_maybe(maybe_jog(JogMove::z(-1.0))),
                    button("Z+0.1").on_press_maybe(maybe_jog(JogMove::z(-0.1))),
                    button("Z-0.1").on_press_maybe(maybe_jog(JogMove::z(0.1))),
                    button("Z-1.0").on_press_maybe(maybe_jog(JogMove::z(1.0))),
                    button("Z-10.0").on_press_maybe(maybe_jog(JogMove::z(10.0))),
                ]
            ],
            scrollable(text(&self.output))
                .width(Length::Fill)
                .height(Length::Fill),
            row![
                text_input("type `help` for list of commands", &self.command)
                    .on_input(Message::CommandInput)
                    .on_submit(Message::ProcessCommand),
                button("send").on_press(Message::ProcessCommand)
            ],
        ]
        .into()
    }
}

fn main() -> iced::Result {
    App::run(iced::Settings::default())
}
