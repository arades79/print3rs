use iced::widget::text;

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
    #[rustfmt::skip]
    let mb = 
    menu_bar!(
        (text("File"), menu_template!(
            (text("Print"))
            (text("Clear"))
            (text("Save"))
            (text("Quit"))
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
