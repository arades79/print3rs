use {
    iced::{
        alignment,
        futures::prelude::{future::err, stream::StreamExt},
        widget::button,
        window::{self, Action},
        Application, Length,
    },
    print3rs_commands::commands::{self, Response},
    print3rs_core::Printer,
    std::sync::Arc,
};

use iced::widget::combo_box::State as ComboState;
use iced::widget::{column, text};
use iced::Command;

use print3rs_core::AsyncPrinterComm;
use tokio_serial::available_ports;
use tokio_stream::wrappers::BroadcastStream;

use winnow::prelude::*;

use rfd::{AsyncFileDialog, FileHandle};

use crate::messages::{JogMove, Message};

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
    pub(crate) error_messages: Vec<String>,
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
                error_messages: Default::default(),
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
        let response_stream =
            BroadcastStream::new(responses).map(|response| Message::from(response.unwrap()));
        iced::subscription::run_with_id(
            std::any::TypeId::of::<PrinterResponseSubscription>(),
            response_stream,
        )
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        match message {
            Message::Jog(JogMove { x, y, z }) => {
                if let Err(msg) = self
                    .commander
                    .printer()
                    .send_unsequenced(format!("G7X{x}Y{y}Z{z}"))
                {
                    self.error_messages.push(msg.to_string());
                }
                Command::none()
            }
            Message::ToggleConnect => {
                if self.commander.printer().is_connected() {
                    self.commander.set_printer(Printer::Disconnected);
                } else if let Some(ref port) = self.selected_port {
                    if port == "auto" {
                        if let Err(msg) = self
                            .commander
                            .dispatch(print3rs_commands::commands::Command::AutoConnect)
                        {
                            self.error_messages.push(msg.0);
                        }
                    } else if let Err(msg) = self.commander.dispatch(commands::Command::Connect(
                        port.as_str(),
                        self.selected_baud,
                    )) {
                        self.error_messages.push(msg.0);
                    }
                }

                Command::none()
            }
            Message::CommandInput(s) => {
                self.command = s;
                Command::none()
            }
            Message::SubmitCommand => {
                if let Ok(command) = print3rs_commands::commands::parse_command.parse(&self.command)
                {
                    if let Err(msg) = self.commander.dispatch(command) {
                        self.error_messages.push(msg.0);
                    }
                    self.command.clear();
                } else {
                    self.error_messages
                        .push("Could not parse command".to_string());
                }
                Command::none()
            }
            Message::ProcessCommand(command) => {
                if let Err(msg) = self.commander.dispatch(&command) {
                    self.error_messages.push(msg.0)
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
            Message::ConsoleAppend(s) => {
                self.output.push_str(&s);
                Command::none()
            }
            Message::AutoConnectComplete(a_printer) => {
                let printer = Arc::into_inner(a_printer).unwrap_or_default();
                self.commander.set_printer(printer);
                Command::none()
            }
            Message::ClearConsole => {
                self.output.clear();
                Command::none()
            }
            Message::Quit => Command::single(iced_runtime::command::Action::Window(Action::Close(
                window::Id::MAIN,
            ))),
            Message::PrintDialog => Command::perform(
                AsyncFileDialog::new()
                    .set_directory(directories_next::BaseDirs::new().unwrap().home_dir())
                    .pick_file(),
                |f| match f {
                    Some(file) => {
                        Message::ProcessCommand(print3rs_commands::commands::Command::Print(
                            file.path().to_string_lossy().into_owned(),
                        ))
                    }
                    None => Message::NoOp,
                },
            ),
            Message::SaveDialog => Command::perform(
                AsyncFileDialog::new()
                    .set_directory(directories_next::BaseDirs::new().unwrap().home_dir())
                    .save_file(),
                |f| match f {
                    Some(file) => Message::SaveConsole(file.into()),
                    None => Message::NoOp,
                },
            ),
            Message::SaveConsole(file) => {
                Command::perform(tokio::fs::write(file, self.output.clone()), |_| {
                    Message::NoOp
                })
            }
            Message::PushError(msg) => {
                for i in 0..10 {
                    self.error_messages.push(format!("{msg} {i}"));
                }

                Command::none()
            }
            Message::DismissError => {
                self.error_messages.pop();
                Command::none()
            }
            Message::NoOp => Command::none(),
        }
    }

    fn view(&self) -> iced::Element<'_, Self::Message, Self::Theme, iced::Renderer> {
        use super::components;
        let screen = column![
            components::app_menu(self),
            components::connector(self),
            components::jogger(self),
            components::console(self),
        ];

        components::error_prompt(self, screen).into()
    }
}
