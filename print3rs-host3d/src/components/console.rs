use {
    iced::{
        widget::{button, column, combo_box::State as ComboState, row, text_editor, text_input},
        Length,
    },
    std::collections::VecDeque,
};

use crate::app::AppElement;
use crate::messages::Message;
use iced::widget::text_editor::Content;

#[derive(Debug)]
pub(crate) struct State {
    pub(crate) output: Content,
    pub(crate) command_state: ComboState<String>,
    pub(crate) command_history: VecDeque<String>,
    pub(crate) command: String,
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
        // let prompt = combo_box(
        //     &self.command_state,
        //     "type `help` for list of commands",
        //     self.command.as_ref(),
        //     Message::CommandInput,
        // )
        // .on_input(Message::CommandInput);
        let content = text_editor(&self.output)
            .on_action(Message::OutputAction)
            .height(Length::Fill);
        column![
            content,
            row![
                text_input("type `help` for list of commands", self.command.as_str())
                    .on_input(Message::CommandInput)
                    .on_submit(Message::SubmitCommand),
                button("send").on_press(Message::SubmitCommand),
            ]
        ]
        .into()
    }
}
