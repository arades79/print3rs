
use iced::Application;

mod components;
mod app;


fn main() -> iced::Result {
    app::App::run(iced::Settings::default())
}
