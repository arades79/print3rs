use {
    iced::futures::prelude::stream::StreamExt,
    print3rs_commands::commands::{self, Macros, Response},
    print3rs_core::{Printer, SerialPrinter},
    std::collections::HashMap,
};

use iced::widget::combo_box::State as ComboState;
use iced::widget::{button, column, combo_box, row, scrollable, text, text_input};
use iced::{Application, Command, Length};
use print3rs_commands::commands::BackgroundTask;
use print3rs_core::AsyncPrinterComm;
use tokio_serial::{available_ports, SerialPortBuilderExt};
use tokio_stream::wrappers::BroadcastStream;

use winnow::prelude::*;

use std::sync::Arc;

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
    commander: commands::Commander,
    bauds: ComboState<u32>,
    selected_baud: Option<u32>,
    command: String,
    output: String,
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
    CommandInput(String),
    ProcessCommand,
    BackgroundResponse(commands::Response),
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
                commander: Default::default(),
                command: Default::default(),
                output: Default::default(),
            },
            iced::Command::none(),
        )
    }

    fn title(&self) -> String {
        let status = if self.commander.printer().is_connected() {
            "Connected"
        } else {
            "Disconnected"
        };
        format!("Print3rs - {status}")
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        struct PrinterResponseSubscription;
        let responses = self.commander.subscribe_responses();
        let response_stream = BroadcastStream::new(responses)
            .map(|response| Message::BackgroundResponse(response.unwrap()));
        iced::subscription::run_with_id(
            std::any::TypeId::of::<PrinterResponseSubscription>(),
            response_stream,
        )
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        match message {
            Message::Jog(JogMove { x, y, z }) => {
                let _ = self
                    .commander
                    .printer()
                    .send_unsequenced(format!("G7X{x}Y{y}Z{z}"));
                Command::none()
            }
            Message::ToggleConnect => {
                if self.commander.printer().is_connected() {
                    self.commander.set_printer(Printer::Disconnected);
                } else if let Some(ref port) = self.selected_port {
                    if port == "auto" {
                        let _ = self
                            .commander
                            .dispatch(print3rs_commands::commands::Command::AutoConnect);
                    } else {
                        let _ = self.commander.dispatch(commands::Command::Connect(
                            port.as_str(),
                            self.selected_baud,
                        ));
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
                    let _ = self.commander.dispatch(command);
                    self.command.clear();
                }
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
            Message::BackgroundResponse(response) => {
                match response {
                    Response::Output(s) => {
                        self.output.push_str(&s);
                    }
                    Response::Error(_) => todo!(),
                    Response::AutoConnect(a_printer) => {
                        let printer = Arc::into_inner(a_printer).unwrap_or_default();
                        self.commander.set_printer(printer);
                    }
                    Response::Clear => {
                        self.output.clear();
                    }
                    Response::Quit => {
                        todo!()
                    }
                };
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
        let maybe_jog = |jogmove| {
            self.commander
                .printer()
                .is_connected()
                .then_some(Message::Jog(jogmove))
        };
        column![
            row![
                port_list,
                baud_list,
                button(if self.commander.printer().is_connected() {
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
                ],

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
