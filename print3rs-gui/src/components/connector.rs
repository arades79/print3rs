use iced::widget::{button, combo_box, row};
use iced::Length;

use crate::app::{App, AppElement, Message};

pub(crate) fn connector(app: &App) -> AppElement<'_> {
    let port_list = combo_box(
        &app.ports,
        "printer port",
        app.selected_port.as_ref(),
        Message::ChangePort,
    )
    .width(Length::FillPortion(5))
    .on_input(Message::ChangePort);
    let baud_list = combo_box(
        &app.bauds,
        "baudrate",
        app.selected_baud.as_ref(),
        Message::ChangeBaud,
    )
    .width(Length::FillPortion(1))
    .on_input(|s| Message::ChangeBaud(s.parse().unwrap_or_default()));
    row![
        port_list,
        baud_list,
        button(if app.commander.printer().is_connected() {
            "disconnect"
        } else {
            "connect"
        })
        .on_press(Message::ToggleConnect)
    ]
    .into()
}
