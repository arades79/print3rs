use iced::{
    widget::{button, column, row, scrollable, text, text_input},
    Length,
};

use crate::app::{App, AppElement, Message};

pub(crate) fn console(app: &App) -> AppElement<'_> {
    column![
        scrollable(text(&app.output))
            .width(Length::Fill)
            .height(Length::Fill),
        row![
            text_input("type `help` for list of commands", &app.command)
                .on_input(Message::CommandInput)
                .on_submit(Message::ProcessCommand),
            button("send").on_press(Message::ProcessCommand)
        ]
    ]
    .into()
}
