use {iced::{widget::{button, text}, Length}};

use iced_aw::{menu, menu::Item, menu_bar};

use crate::app::App;
use crate::messages::Message;

macro_rules! menu_template {
     ($($x:tt)+) => {
         menu!($($x)+).max_width(180.0).offset(6.0).spacing(5.0)
     };
 }

pub(crate) fn app_menu(
    _app: &App,
) -> menu::MenuBar<'_, Message, <App as iced::Application>::Theme, iced::Renderer> {
    let menu_button = |label, message| {
        button(label).on_press(message).width(Length::Fill)
    };
    #[rustfmt::skip]
    let mb = 
    menu_bar!(
        (text("File"), menu_template!(
            (menu_button("Print", Message::PrintDialog))
            (menu_button("Clear",Message::ClearConsole))
            (menu_button("Save",Message::SaveDialog))
            (menu_button("Quit",Message::Quit))
        ))
        (text("Settings"), menu_template!(
            (text("Log Level"))
            (text("Jog Increments"))
            (text("Probably something else"))
        ))
    )
    .spacing(4.0);
    mb
}
