use {
    crate::app::AppElement,
    iced::{
        widget::{button, column, pick_list, text},
        Length,
    },
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
    let menu_button = |label: &str| button(text(label)).width(Length::Fill);
    let themes = column(iced::Theme::ALL.iter().map(|theme| {
        menu_button(&theme.to_string())
            .on_press(Message::ChangeTheme(theme.clone()))
            .into()
    }));
    #[rustfmt::skip]
    let mb = 
    menu_bar!(
        (text("File"), menu_template!(
            (menu_button("Print").on_press(Message::PrintDialog))
            (menu_button("Clear").on_press(Message::ClearConsole))
            (menu_button("Save").on_press(Message::SaveDialog))
            (menu_button("Quit").on_press(Message::Quit))
        ))
        (text("Interface"), menu_template!(
            ("Theme", menu_template!((themes)))
        ))
    )
    .spacing(8.0);
    mb
}
