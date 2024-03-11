use {
    iced::{
        widget::{
            button, column, combo_box, combo_box::State as ComboState, container, row, text_editor,
        },
        Length,
    },
    std::collections::VecDeque,
};

use crate::app::{App, AppElement};
use crate::messages::Message;
use iced::widget::text_editor::Content;

#[derive(Debug)]
pub(crate) struct State {
    pub(crate) output: Content,
    pub(crate) command_state: ComboState<String>,
    pub(crate) command_history: VecDeque<String>,
    pub(crate) command: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            output: Default::default(),
            command_state: ComboState::new(vec![]), // TODO: load history from file here
            command_history: Default::default(),
            command: Default::default(),
        }
    }
}

impl State {
    pub(crate) fn view(&self) -> AppElement<'_> {
        let prompt = combo_box(
            &self.command_state,
            "type `help` for list of commands",
            self.command.as_ref(),
            Message::CommandInput,
        )
        .on_input(Message::CommandInput);
        let content = text_editor(&self.output)
            .on_action(Message::OutputAction)
            .height(Length::Fill);
        column![
            content,
            row![prompt, button("send").on_press(Message::SubmitCommand),]
        ]
        .into()
    }
}
