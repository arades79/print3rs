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