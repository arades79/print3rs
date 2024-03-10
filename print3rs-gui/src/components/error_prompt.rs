use crate::app::App;
use iced_aw::{badge, modal, BadgeStyles, Modal};
use {
    crate::messages::Message,
    iced::{alignment, widget::text, Element, Length},
};

pub(crate) fn error_prompt<'a>(
    app: &App,
    underlay: impl Into<Element<'a, Message>>,
) -> Modal<'a, Message> {
    let error_bar = app.error_messages.last().map(|error_text| {
        badge(
            text(error_text)
                .width(Length::Fill)
                .size(20)
                .horizontal_alignment(alignment::Horizontal::Center),
        )
        .style(BadgeStyles::Danger)
        .align_y(alignment::Alignment::End)
    });
    modal(underlay, error_bar)
        .backdrop(Message::DismissError)
        .on_esc(Message::DismissError)
        .align_y(alignment::Vertical::Bottom)
}
