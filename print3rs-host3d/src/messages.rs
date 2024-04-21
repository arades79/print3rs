use {
    print3rs_commands::commands::{Command, Response},
    print3rs_core::Printer,
    std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    },
};

#[derive(Debug, Clone, Default)]
pub(crate) struct JogMove {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) z: f32,
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
    SubmitCommand,
    ProcessCommand(Command<String>),
    Quit,
    ClearConsole,
    PrintDialog,
    SaveDialog,
    SaveConsole(PathBuf),
    ConsoleAppend(String),
    AutoConnectComplete(Arc<Mutex<Printer>>),
    PushError(String),
    DismissError,
    OutputAction(iced::widget::text_editor::Action),
    ChangeTheme(iced::Theme),
    NoOp,
}

impl From<Response> for Message {
    fn from(value: Response) -> Self {
        match value {
            Response::Output(s) => Message::ConsoleAppend(s.to_string()),
            Response::Error(e) => Message::PushError(e.0),
            Response::AutoConnect(a) => Message::AutoConnectComplete(a),
            Response::Clear => Message::ClearConsole,
            Response::Quit => Message::Quit,
        }
    }
}
