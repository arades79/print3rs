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


pub(crate) fn app_menu(_app: &App) -> AppElement<'_> {
    let mb = menu_bar!((
        text("File"),
        menu!((text("Print"))(text("Clear"))(text("Save"))(text("Quit")))
    )(
        text("Settings"),
        menu!((text("Log Level"))(text("Jog Increments"))(text(
            "Probably something else"
        )))
    )).spacing(4.0);
    mb.into()
}