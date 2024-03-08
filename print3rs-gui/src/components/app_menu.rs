


use iced::widget::{text};
use iced::{Application};





use iced_aw::{menu, menu::Item, menu_bar};






use crate::app::{App, AppElement};


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