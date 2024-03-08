use {
    iced::futures::prelude::stream::StreamExt,
    print3rs_commands::commands::{self, Macros, Response},
    print3rs_core::{Printer, SerialPrinter},
    std::collections::HashMap,
};

use iced::widget::combo_box::State as ComboState;
use iced::widget::{button, column, combo_box, row, scrollable, text, text_input};
use iced::{Application, Command, Length};
use print3rs_commands::commands::BackgroundTask;
use print3rs_core::AsyncPrinterComm;
use tokio_serial::{available_ports, SerialPortBuilderExt};
use tokio_stream::wrappers::BroadcastStream;

use iced_aw::{grid, grid_row, menu, menu::Item, menu_bar, Element};

use winnow::prelude::*;

use iced::widget::horizontal_space;
use std::sync::Arc;

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
