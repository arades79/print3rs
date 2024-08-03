use iced::{
    widget::{button, column, row, text, vertical_slider},
    Length,
};

use crate::app::{App, AppElement};
use crate::messages::{JogMove, Message};

pub(crate) fn jogger(app: &App) -> AppElement<'_> {
    enum Jog {
        X(f32),
        Y(f32),
        Z(f32),
    }
    let maybe_jog = |jogmove| {
        app.commander
            .printer()
            .is_connected()
            .then_some(Message::Jog(jogmove))
    };
    let jog_button = |jog: Jog| {
        let (label, jogmove) = match jog {
            Jog::X(scale) => (text(format!("X{scale:+}")), JogMove::x(scale)),
            Jog::Y(scale) => (text(format!("Y{scale:+}")), JogMove::y(scale)),
            Jog::Z(scale) => (
                text(format!("Z{:+}", scale / 10.0)),
                JogMove::z(scale / 10.0),
            ),
        };
        button(label)
            .on_press_maybe(maybe_jog(jogmove))
            .width(Length::Fixed(64.0))
    };
    let scale = app.jog_scale.round().max(1.0);
    row![
        column![
            jog_button(Jog::Y(scale)),
            row![jog_button(Jog::X(-scale)), jog_button(Jog::X(scale)),],
            jog_button(Jog::Y(-scale)),
        ]
        .align_items(iced::Alignment::Center),
        column![jog_button(Jog::Z(scale)), jog_button(Jog::Z(-scale)),],
        vertical_slider(0.0..=100.0, app.jog_scale, Message::JogScale)
            .step(1.0)
            .shift_step(10.0)
            .height(Length::Fixed(100.0))
    ]
    .into()
}
