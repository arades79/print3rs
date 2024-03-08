use {
    iced::futures::prelude::stream::StreamExt,
    print3rs_commands::commands::{self, Macros, Response},
    print3rs_core::{Printer, SerialPrinter},
    std::collections::HashMap,
    std::sync::Arc,
};

use iced::widget::combo_box::State as ComboState;
use iced::widget::{button, column, combo_box, row, scrollable, text, text_input};
use iced::{Application, Command, Length};
use print3rs_commands::commands::BackgroundTask;
use print3rs_core::AsyncPrinterComm;
use tokio_serial::{available_ports, SerialPortBuilderExt};
use tokio_stream::wrappers::BroadcastStream;

use iced_aw::{grid, grid_row, menu, menu::Item, menu_bar, Element};

use winnow::prelude::*;

pub(crate) type AppElement<'a> = iced_aw::Element<'a, <App as iced::Application>::Message>;

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
pub(crate) struct App {
    pub(crate) ports: ComboState<String>,
    pub(crate) selected_port: Option<String>,
    pub(crate) commander: commands::Commander,
    pub(crate) bauds: ComboState<u32>,
    pub(crate) selected_baud: Option<u32>,
    pub(crate) command: String,
    pub(crate) output: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct JogMove {
    x: f32,
    y: f32,
    z: f32,
}

impl JogMove {
    pub(crate) fn x(x: f32) -> Self {
        Self {
            x,
            ..Default::default()
        }
    }
    pub(crate) fn y(y: f32) -> Self {
        Self {
            y,
            ..Default::default()
        }
    }
    pub(crate) fn z(z: f32) -> Self {
        Self {
            z,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
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
        use super::components;
        column![
            components::app_menu(self),
            components::connector(self),
            components::jogger(self),
            components::console(self),
        ]
        .into()
    }
}
