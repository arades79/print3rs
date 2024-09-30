use {app::App, cosmic::app::Settings, std::error::Error};

mod app;
mod components;
mod messages;

fn main() -> Result<(), Box<dyn Error>> {
    cosmic::app::run::<App>(Settings::default(), ())?;
    Ok(())
}
