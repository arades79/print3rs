use iced::{
    widget::{button, column, combo_box, combo_box::State, row, scrollable, text, text_input},
    Length,
};

use crate::app::{App, AppElement};
use crate::messages::Message;

pub(crate) fn console(app: &App) -> AppElement<'_> {
    let prompt = combo_box(
        &app.command_state,
        "type `help` for list of commands",
        app.command.as_ref(),
        Message::CommandInput,
    )
    .on_input(Message::CommandInput);
    column![
        scrollable(text(&app.output))
            .width(Length::Fill)
            .height(Length::Fill),
        row![prompt, button("send").on_press(Message::SubmitCommand),]
    ]
    .into()
}
