use iced::{
    widget::{button, column, horizontal_space, row, text},
    Element, Length,
};

use iced_aw::{menu, menu::Item, menu_bar};

use print3rs_commands::commands::Command;

use crate::app::App;
use crate::messages::Message;

macro_rules! menu_template {
     ($($x:tt)+) => {
         menu!($($x)+).max_width(180.0)
     };
 }

pub(crate) fn app_menu(app: &App) -> Element<'_, Message> {
    let menu_button = |label: &str| button(text(label)).width(Length::Fill);
    let themes = column(iced::Theme::ALL.iter().map(|theme| {
        menu_button(&theme.to_string())
            .on_press(Message::ChangeTheme(theme.clone()))
            .into()
    }));
    let macros = column(app.commander.macros.iter().map(|(label, content)| {
        menu_button(label.as_str())
            .on_press_maybe(
                app.commander
                    .printer()
                    .is_connected()
                    .then(|| Message::ProcessCommand(Command::Gcodes(content.clone()))),
            )
            .into()
    }));
    let tasks = column(app.commander.tasks.keys().map(|name| {
        row![
            text(name),
            horizontal_space(),
            button("X").on_press(Message::ProcessCommand(Command::Stop(name.to_string())))
        ]
        .into()
    }));
    #[rustfmt::skip]
    let mb = 
    menu_bar!(
        ("File", menu_template!(
            (menu_button("Print").on_press(Message::PrintDialog))
            (menu_button("Clear").on_press(Message::ClearConsole))
            (menu_button("Save").on_press(Message::SaveDialog))
            (menu_button("Quit").on_press(Message::Quit))
        ))
        ("Interface", menu_template!(
            ("Theme", menu_template!((themes)))
        ))
        ("Macros", menu_template!(
            (macros)
        ))
        ("Tasks", menu_template!(
            (tasks)
        ))

    )
    .spacing(10.0).padding(10.0);
    mb.into()
}
