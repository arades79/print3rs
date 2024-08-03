use iced::widget::{button, column, row, slider, text, Space};

use crate::app::{App, AppElement};
use crate::messages::{JogMove, Message, MoveAxis};

pub(crate) fn jogger(app: &App) -> AppElement<'_> {
    enum Jog {
        X(f32),
        Y(f32),
        Z(f32),
    }
    const BUTTON_WIDTH: f32 = 72.0;
    let if_connected = |message| app.commander.printer().is_connected().then_some(message);
    let jog_button = |jog: Jog| {
        let (label, jogmove) = match jog {
            Jog::X(scale) => (text(format!("X{scale:+}")), JogMove::x(scale)),
            Jog::Y(scale) => (text(format!("Y{scale:+}")), JogMove::y(scale)),
            Jog::Z(scale) => (
                text(format!("Z{:+.1}", scale / 10.0)),
                JogMove::z(scale / 10.0),
            ),
        };
        button(
            label
                .horizontal_alignment(iced::alignment::Horizontal::Center)
                .vertical_alignment(iced::alignment::Vertical::Center),
        )
        .on_press_maybe(if_connected(Message::Jog(jogmove)))
        .width(BUTTON_WIDTH)
    };
    let scale = app.jog_scale.round().max(1.0);
    let xy_buttons = column![
        jog_button(Jog::Y(scale)),
        row![
            jog_button(Jog::X(-scale)),
            Space::with_width(BUTTON_WIDTH),
            jog_button(Jog::X(scale)),
        ],
        jog_button(Jog::Y(-scale)),
    ]
    .align_items(iced::Alignment::Center);

    column![
        row![
            xy_buttons,
            Space::with_width(10.0),
            column![
                Space::with_height(10.0),
                jog_button(Jog::Z(scale)),
                Space::with_height(10.0),
                jog_button(Jog::Z(-scale))
            ]
        ]
        .align_items(iced::Alignment::Center),
        Space::with_height(10.0),
        row![
            button(text("home").horizontal_alignment(iced::alignment::Horizontal::Center))
                .width(BUTTON_WIDTH)
                .on_press_maybe(if_connected(Message::Home(MoveAxis::All))),
            button(text("X").horizontal_alignment(iced::alignment::Horizontal::Center))
                .width(BUTTON_WIDTH / 2.0)
                .on_press_maybe(if_connected(Message::Home(MoveAxis::X))),
            button(text("Y").horizontal_alignment(iced::alignment::Horizontal::Center))
                .width(BUTTON_WIDTH / 2.0)
                .on_press_maybe(if_connected(Message::Home(MoveAxis::Y))),
            button(text("Z").horizontal_alignment(iced::alignment::Horizontal::Center))
                .width(BUTTON_WIDTH / 2.0)
                .on_press_maybe(if_connected(Message::Home(MoveAxis::Z)))
        ]
        .align_items(iced::Alignment::Center),
        slider(0.0..=100.0, app.jog_scale, Message::JogScale)
            .step(1.0)
            .shift_step(10.0)
    ]
    .into()
}
