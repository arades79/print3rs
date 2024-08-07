macro_rules! centered_row  {
    ($($x:expr),+ $(,)?) => (
        iced::widget::row![iced::widget::horizontal_space(), $($x),+ , iced::widget::horizontal_space()]
    );
}

pub(crate) use centered_row;
