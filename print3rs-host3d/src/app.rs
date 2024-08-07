use {
    crate::components,
    iced::{
        futures::prelude::stream::StreamExt,
        widget::{column, container, row},
        window::{self, Action},
    },
    print3rs_commands::{commander::Commander, commands},
    print3rs_core::Printer,
    std::sync::Arc,
};
use {crate::components::Console, print3rs_commands::commands::connect::Connection};

use iced::widget::combo_box::State as ComboState;
use iced::Command;

use tokio_serial::available_ports;
use tokio_stream::wrappers::BroadcastStream;

use winnow::prelude::*;

use rfd::AsyncFileDialog;

use crate::messages::{JogMove, Message};

pub(crate) type AppElement<'a> = iced_aw::Element<'a, <App as iced::Application>::Message>;

#[derive(Debug)]
pub(crate) struct App {
    pub(crate) ports: ComboState<String>,
    pub(crate) connection: Connection<String>,
    pub(crate) commander: Commander,
    pub(crate) console: Console,
    pub(crate) error_messages: Vec<String>,
    pub(crate) theme: iced::Theme,
    pub(crate) jog_scale: f32,
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
                connection: Connection::Auto,
                commander: Default::default(),
                console: Default::default(),
                error_messages: Default::default(),
                theme: iced::Theme::Light,
                jog_scale: 10.0,
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
                    .try_send_unsequenced(format!("G7X{x}Y{y}Z{z}"))
                {
                    self.error_messages.push(msg.to_string());
                }
                Command::none()
            }
            Message::ToggleConnect => {
                if self.commander.printer().is_connected() {
                    self.commander.set_printer(Printer::Disconnected);
                } else if let Err(msg) =
                    self.commander
                        .dispatch(print3rs_commands::commands::Command::Connect(
                            self.connection.to_borrowed(),
                        ))
                {
                    self.error_messages.push(msg.0);
                }

                Command::none()
            }
            Message::CommandInput(s) => {
                self.console.command = s;
                Command::none()
            }
            Message::SubmitCommand => {
                let command_string = &mut self.console.command;
                if command_string.is_empty() {
                    return Command::none();
                }
                if let Ok(command) =
                    print3rs_commands::commands::parse_command.parse(command_string)
                {
                    if let Err(msg) = self.commander.dispatch(command) {
                        self.error_messages.push(msg.0);
                    }
                    if !self.console.command_history.contains(command_string) {
                        self.console
                            .command_history
                            .push_back(command_string.clone());
                        if self.console.command_history.len() > 1000 {
                            self.console.command_history.pop_front();
                        }
                        self.console.command_history.make_contiguous();
                        self.console.command_state =
                            ComboState::new(self.console.command_history.as_slices().0.to_owned());
                    }
                    command_string.clear();
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
            Message::ConsoleAppend(s) => {
                use iced::widget::text_editor::{Action, Edit};
                for c in s.chars() {
                    let action = Action::Edit(Edit::Insert(c));
                    self.console.output.perform(action)
                }
                self.console.output.perform(Action::Edit(Edit::Enter));
                Command::none()
            }
            Message::AutoConnectComplete(a_printer) => {
                let printer = Arc::into_inner(a_printer)
                    .unwrap_or_default()
                    .into_inner()
                    .unwrap_or_default();
                self.commander.set_printer(printer);
                Command::none()
            }
            Message::ClearConsole => {
                self.console.output = iced::widget::text_editor::Content::new();
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
                Command::perform(tokio::fs::write(file, self.console.output.text()), |_| {
                    Message::NoOp
                })
            }
            Message::PushError(msg) => {
                self.error_messages.push(msg);
                Command::none()
            }
            Message::DismissError => {
                self.error_messages.pop();
                Command::none()
            }
            Message::OutputAction(action) => {
                if !action.is_edit() {
                    self.console.output.perform(action);
                }
                Command::none()
            }
            Message::NoOp => Command::none(),
            Message::ChangeTheme(theme) => {
                self.theme = theme;
                Command::none()
            }
            Message::JogScale(scale) => {
                self.jog_scale = scale;
                Command::none()
            }
            Message::Home(axis) => {
                let arg = match axis {
                    crate::messages::MoveAxis::X => "X",
                    crate::messages::MoveAxis::Y => "Y",
                    crate::messages::MoveAxis::Z => "Z",
                    crate::messages::MoveAxis::All => "",
                };
                if let Err(msg) = self
                    .commander
                    .printer()
                    .try_send_unsequenced(format!("G28{arg}"))
                {
                    self.error_messages.push(msg.to_string());
                }
                Command::none()
            }
            Message::SelectProtocol(proto) => {
                self.connection = match proto {
                    components::Protocol::Auto => Connection::Auto,
                    components::Protocol::Serial => Connection::Serial {
                        port: "".to_string(),
                        baud: None,
                    },
                    components::Protocol::Tcp => Connection::Tcp {
                        hostname: "".to_string(),
                        port: None,
                    },
                    components::Protocol::Mqtt => Connection::Mqtt {
                        hostname: "".to_string(),
                        port: None,
                        in_topic: None,
                        out_topic: None,
                    },
                };
                Command::none()
            }
            Message::ChangeConnection(connection) => {
                self.connection = connection;
                Command::none()
            }
        }
    }

    fn view(&self) -> iced::Element<'_, Self::Message, Self::Theme, iced::Renderer> {
        let main_content = container(column![
            row![components::connector(self), components::jogger(self)],
            self.console.view(),
        ])
        .padding(10.0);
        let screen = column![components::app_menu(self), main_content];

        components::error_prompt(self, screen).into()
    }

    fn theme(&self) -> Self::Theme {
        self.theme.clone()
    }
}
