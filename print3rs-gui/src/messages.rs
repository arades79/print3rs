use print3rs_commands::commands::Response;

#[derive(Debug, Clone, Default)]
pub(crate) struct JogMove {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) z: f32,
}

impl JogMove {
    pub(crate) fn x(x: f32) -> Self {
        Self {
            x,
            ..Default::default()
        }
    }
    pub(crate) fn y(y: f32) -> Self {
        Self {
            y,
            ..Default::default()
        }
    }
    pub(crate) fn z(z: f32) -> Self {
        Self {
            z,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Jog(JogMove),
    ChangePort(String),
    ChangeBaud(u32),
    ToggleConnect,
    CommandInput(String),
    ProcessCommand,
    BackgroundResponse(Response),
}
