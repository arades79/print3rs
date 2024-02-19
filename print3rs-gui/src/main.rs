use iced::widget::{button, column, combo_box, row, text_input};
use iced::{Application, Command};
use tokio_serial::SerialPortBuilderExt;

#[derive(Debug)]
struct App {
    printer: print3rs_core::Printer,
    printer_path: String,
    printer_baud: u32,
    temperature: f32,
}

#[derive(Debug, Clone, Default)]
struct JogMove {
    x: f32,
    y: f32,
    z: f32,
}

impl JogMove {
    fn x(x: f32) -> Self {
        Self {
            x,
            ..Default::default()
        }
    }
    fn y(y: f32) -> Self {
        Self {
            y,
            ..Default::default()
        }
    }
    fn z(z: f32) -> Self {
        Self {
            z,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
enum Message {
    Jog(JogMove),
    Connect,
    GcodeFinish,
    ConnectTextInput(String),
    Disconnect,
    Autoconnect,
    Command(String),
}

impl iced::Application for App {
    type Executor = iced::executor::Default;

    type Message = Message;

    type Theme = iced::Theme;

    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        (
            Self {
                printer: print3rs_core::Printer::new_disconnected(),
                printer_path: String::new(),
                printer_baud: 115200,
                temperature: 0.0,
            },
            iced::Command::none(),
        )
    }

    fn title(&self) -> String {
        "Print3rs".to_string()
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        match message {
            Message::Jog(JogMove { x, y, z }) => {
                let socket = self.printer.socket();
                Command::perform(
                    async move { socket.send_unsequenced(format!("G7X{x}Y{y}Z{z}")).await },
                    |_| Message::GcodeFinish,
                )
            }
            Message::Autoconnect => Command::none(),
            Message::ConnectTextInput(s) => {
                self.printer_path = s;
                Command::none()
            }
            Message::Connect => {
                self.printer.connect(
                    tokio_serial::new(&self.printer_path, self.printer_baud)
                        .open_native_async()
                        .unwrap(),
                );
                Command::none()
            }
            Message::GcodeFinish => Command::none(),
            Message::Disconnect => {
                self.printer.disconnect();
                Command::none()
            }
            Message::Command(_) => Command::none(),
        }
    }

    fn view(&self) -> iced::Element<'_, Self::Message, Self::Theme, iced::Renderer> {
        let connect_params = text_input("eg. 'COM1', '/dev/ttyACM0'", &self.printer_path)
            .on_input(Message::ConnectTextInput)
            .on_submit(Message::Connect);
        iced::widget::column![
            row![connect_params, button("Connect").on_press(Message::Connect)],
            button("Y+100.0").on_press(Message::Jog(JogMove::y(100.0))),
            button("Y+10.0").on_press(Message::Jog(JogMove::y(10.0))),
            button("Y+1.0").on_press(Message::Jog(JogMove::y(1.0))),
            row![
                button("X-100.0").on_press(Message::Jog(JogMove::x(-100.0))),
                button("X-10.0").on_press(Message::Jog(JogMove::x(-10.0))),
                button("X-1.0").on_press(Message::Jog(JogMove::x(-1.0))),
                button("X+1.0").on_press(Message::Jog(JogMove::x(1.0))),
                button("X+10.0").on_press(Message::Jog(JogMove::x(10.0))),
                button("X+100.0").on_press(Message::Jog(JogMove::x(100.0)))
            ],
            button("Y-1.0").on_press(Message::Jog(JogMove::y(-1.0))),
            button("Y-10.0").on_press(Message::Jog(JogMove::y(-10.0))),
            button("Y-100.0").on_press(Message::Jog(JogMove::y(-100.0))),
        ]
        .align_items(iced::Alignment::Center)
        .into()
    }
}
fn main() -> iced::Result {
    App::run(iced::Settings::default())
}
