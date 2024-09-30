use cosmic::iced_widget::{button, column, row};
use cosmic::widget::{container, slider, text, Space};
use cosmic::Element;
use {super::centered_row::centered_row, cosmic::iced::alignment};
use {crate::app::App, cosmic::iced::Alignment};
use {
    crate::messages::{JogMove, Message, MoveAxis},
    iced_aw::number_input,
};

pub(crate) fn jogger(app: &App) -> Element<'_, Message> {
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
                .horizontal_alignment(alignment::Horizontal::Center)
                .vertical_alignment(alignment::Vertical::Center),
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
        ]
        .spacing(0.0),
        jog_button(Jog::Y(-scale)),
    ]
    .spacing(0.0)
    .align_items(Alignment::Center);

    container(
        column![
            centered_row![
                xy_buttons,
                column![
                    Space::with_height(10.0),
                    jog_button(Jog::Z(scale)),
                    Space::with_height(10.0),
                    jog_button(Jog::Z(-scale))
                ]
                .spacing(10.0),
            ]
            .spacing(10.0)
            .align_items(Alignment::Center),
            centered_row![
                slider(0.0..=100.0, app.jog_scale, Message::JogScale)
                    .step(1.0)
                    .width(240),
                number_input(scale, 0.0..100.0, Message::JogScale).width(70),
            ]
            .spacing(10.0),
            centered_row![
                button(text("home").horizontal_alignment(alignment::Horizontal::Center))
                    .width(BUTTON_WIDTH)
                    .on_press_maybe(if_connected(Message::Home(MoveAxis::All))),
                button(text("X").horizontal_alignment(alignment::Horizontal::Center))
                    .width(BUTTON_WIDTH / 2.0)
                    .on_press_maybe(if_connected(Message::Home(MoveAxis::X))),
                button(text("Y").horizontal_alignment(alignment::Horizontal::Center))
                    .width(BUTTON_WIDTH / 2.0)
                    .on_press_maybe(if_connected(Message::Home(MoveAxis::Y))),
                button(text("Z").horizontal_alignment(alignment::Horizontal::Center))
                    .width(BUTTON_WIDTH / 2.0)
                    .on_press_maybe(if_connected(Message::Home(MoveAxis::Z))),
            ],
        ]
        .spacing(10.0),
    )
    .center_x()
    .padding(10)
    .into()
}
