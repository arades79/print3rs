use iced::{
    widget::{button, text},
    Length,
};

use iced_aw::{menu, menu::Item, menu_bar};

use crate::app::App;
use crate::messages::Message;

macro_rules! menu_template {
     ($($x:tt)+) => {
         menu!($($x)+).max_width(180.0)
     };
 }

pub(crate) fn app_menu(
    _app: &App,
) -> menu::MenuBar<'_, Message, <App as iced::Application>::Theme, iced::Renderer> {
    let menu_button = |label| button(label).width(Length::Fill);
    #[rustfmt::skip]
    let mb = 
    menu_bar!(
        (text("File"), menu_template!(
            (menu_button("Print").on_press(Message::PrintDialog))
            (menu_button("Clear").on_press(Message::ClearConsole))
            (menu_button("Save").on_press(Message::SaveDialog))
            (menu_button("Quit").on_press(Message::Quit))
        ))
        (text("Settings"), menu_template!(
            (text("Log Level"))
            (text("Jog Increments"))
            (text("Probably something else"))
        ))
    )
    .spacing(8.0);
    mb
}
