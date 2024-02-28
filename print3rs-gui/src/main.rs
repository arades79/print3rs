use std::collections::HashMap;

use iced::widget::combo_box::State as ComboState;
use iced::widget::{button, column, combo_box, row, scrollable, text, text_input};
use iced::{Application, Command, Length};
use print3rs_commands::commands::{BackgroundTask, HandleCommand};
use print3rs_core::AsyncPrinterComm;
use tokio_serial::{available_ports, SerialPortBuilderExt};

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
    GcodeFinish,
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
                if let Ok(socket) = self.printer.socket() {
                    let socket = socket.clone();
                    Command::perform(
                        async move { socket.send_unsequenced(format!("G7X{x}Y{y}Z{z}")).await },
                        |_| Message::GcodeFinish,
                    )
                } else {
                    Command::none()
                }
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
            Message::GcodeFinish => Command::none(),
            Message::CommandInput(s) => {
                self.command = s;
                Command::none()
            }
            Message::ProcessCommand => {
                if self.command.is_empty() {
                    return Command::none();
                }
                self.command.push('\n');
                self.output.push_str(&self.command);
                self.command.clear();
                Command::none()
            }
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
        let maybe_jog_x = |dist| {
            self.printer
                .is_connected()
                .then_some(Message::Jog(JogMove::x(dist)))
        };
        let maybe_jog_y = |dist| {
            self.printer
                .is_connected()
                .then_some(Message::Jog(JogMove::y(dist)))
        };
        column![
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
                button("Y+100.0").on_press_maybe(maybe_jog_y(100.0)),
                button("Y+10.0").on_press_maybe(maybe_jog_y(10.0)),
                button("Y+1.0").on_press_maybe(maybe_jog_y(1.0)),
                row![
                    button("X-100.0").on_press_maybe(maybe_jog_x(-100.0)),
                    button("X-10.0").on_press_maybe(maybe_jog_x(-10.0)),
                    button("X-1.0").on_press_maybe(maybe_jog_x(-1.0)),
                    button("X+1.0").on_press_maybe(maybe_jog_x(1.0)),
                    button("X+10.0").on_press_maybe(maybe_jog_x(10.0)),
                    button("X+100.0").on_press_maybe(maybe_jog_x(100.0))
                ],
                button("Y-1.0").on_press_maybe(maybe_jog_y(-1.0)),
                button("Y-10.0").on_press_maybe(maybe_jog_y(-10.0)),
                button("Y-100.0").on_press_maybe(maybe_jog_y(-100.0)),
            ]
            .align_items(iced::Alignment::Center),
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
