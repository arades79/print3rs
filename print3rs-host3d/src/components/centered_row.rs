macro_rules! centered_row  {
    ($($x:expr),+ $(,)?) => (
        ::cosmic::iced_widget::row![::cosmic::iced::widget::horizontal_space(::cosmic::iced::Length::Fill
        ), $($x),+ , ::cosmic::iced_widget::horizontal_space(::cosmic::iced::Length::Fill
        )]
    );
}
pub(crate) use centered_row;
