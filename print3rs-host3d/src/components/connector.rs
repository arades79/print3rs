use iced::{
    widget::{row, text_input},
    Length,
};
use {
    super::centered_row::centered_row,
    iced::widget::{pick_list, radio},
};
use {
    iced::{
        widget::{button, column, combo_box},
        Element,
    },
    iced_aw::number_input,
};

use print3rs_commands::commands::connect::Connection;

use crate::app::App;
use crate::messages::Message;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Protocol {
    Auto,
    Serial,
    Tcp,
    Mqtt,
}

impl Protocol {
    fn from_connection(connection: &Connection<String>) -> Self {
        match connection {
            Connection::Auto => Protocol::Auto,
            Connection::Serial { .. } => Protocol::Serial,
            Connection::Tcp { .. } => Protocol::Tcp,
            Connection::Mqtt { .. } => Protocol::Mqtt,
            _ => todo!(),
        }
    }
}

pub(crate) fn connector(app: &App) -> Element<'_, Message> {
    let connection_details: Element<'_, Message> = match app.connection.clone() {
        Connection::Auto => "".into(),
        Connection::Serial { port, baud } => column![
            combo_box(&app.ports, "printer port", Some(&port), move |port| {
                Message::ChangeConnection(Connection::Serial { port, baud })
            },)
            .on_input(move |port| Message::ChangeConnection(Connection::Serial { port, baud })),
            pick_list([9600, 115200], baud, move |baud| Message::ChangeConnection(
                Connection::Serial {
                    port: port.clone(),
                    baud: Some(baud)
                }
            ),),
        ]
        .into(),
        Connection::Tcp { hostname, port } => text_input("hostname:port", &hostname)
            .on_input(move |hostname| Message::ChangeConnection(Connection::Tcp { hostname, port }))
            .into(),
        Connection::Mqtt {
            hostname,
            port,
            in_topic,
            out_topic,
        } => column![
            text_input("hostname:port", &hostname),
            text_input("in topic", &in_topic.unwrap_or_default()),
            text_input("out topic", &out_topic.unwrap_or_default())
        ]
        .into(),
        _ => todo!(),
    };
    let auto = radio(
        "Auto",
        Protocol::Auto,
        Some(Protocol::from_connection(&app.connection)),
        Message::SelectProtocol,
    )
    .spacing(5);
    let serial = radio(
        "Serial",
        Protocol::Serial,
        Some(Protocol::from_connection(&app.connection)),
        Message::SelectProtocol,
    )
    .spacing(5);
    let tcp = radio(
        "TCP/IP",
        Protocol::Tcp,
        Some(Protocol::from_connection(&app.connection)),
        Message::SelectProtocol,
    )
    .spacing(5);
    let mqtt = radio(
        "MQTT",
        Protocol::Mqtt,
        Some(Protocol::from_connection(&app.connection)),
        Message::SelectProtocol,
    )
    .spacing(5);
    let protocol_selector = row!["Protocol:", auto, serial, tcp, mqtt]
        .spacing(20.0)
        .align_items(iced::Alignment::Center);
    column![
        protocol_selector,
        connection_details,
        centered_row![button(if app.commander.printer().is_connected() {
            "disconnect"
        } else {
            "connect"
        })
        .on_press(Message::ToggleConnect)]
    ]
    .spacing(10.0)
    .into()
}
