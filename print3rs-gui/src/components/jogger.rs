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

use crate::app::{App, AppElement, Message, JogMove};

pub(crate) fn jogger(app: &App) -> AppElement<'_> {
    let maybe_jog = |jogmove| {
        app.commander
            .printer()
            .is_connected()
            .then_some(Message::Jog(jogmove))
    };
    row![
        column![
            button("Y+100.0").on_press_maybe(maybe_jog(JogMove::y(100.0))),
            button("Y+10.0").on_press_maybe(maybe_jog(JogMove::y(10.0))),
            button("Y+1.0").on_press_maybe(maybe_jog(JogMove::y(1.0))),
            row![
                button("X-100.0").on_press_maybe(maybe_jog(JogMove::x(-100.0))),
                button("X-10.0").on_press_maybe(maybe_jog(JogMove::x(-10.0))),
                button("X-1.0").on_press_maybe(maybe_jog(JogMove::x(-1.0))),
                button("X+1.0").on_press_maybe(maybe_jog(JogMove::x(1.0))),
                button("X+10.0").on_press_maybe(maybe_jog(JogMove::x(10.0))),
                button("X+100.0").on_press_maybe(maybe_jog(JogMove::x(100.0)))
            ],
            button("Y-1.0").on_press_maybe(maybe_jog(JogMove::y(-1.0))),
            button("Y-10.0").on_press_maybe(maybe_jog(JogMove::y(-10.0))),
            button("Y-100.0").on_press_maybe(maybe_jog(JogMove::y(-100.0))),
        ]
        .align_items(iced::Alignment::Center),
        column![
            button("Z+10.0").on_press_maybe(maybe_jog(JogMove::z(-10.0))),
            button("Z+1.0").on_press_maybe(maybe_jog(JogMove::z(-1.0))),
            button("Z+0.1").on_press_maybe(maybe_jog(JogMove::z(-0.1))),
            button("Z-0.1").on_press_maybe(maybe_jog(JogMove::z(0.1))),
            button("Z-1.0").on_press_maybe(maybe_jog(JogMove::z(1.0))),
            button("Z-10.0").on_press_maybe(maybe_jog(JogMove::z(10.0))),
        ],
    ]
    .into()
}