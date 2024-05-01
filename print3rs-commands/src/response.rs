use {
    crate::commander::ErrorKindOf,
    print3rs_core::Printer,
    std::sync::{Arc, Mutex},
};

#[derive(Debug, Clone)]
pub enum Response {
    Output(Arc<str>),
    Error(ErrorKindOf),
    AutoConnect(Arc<Mutex<Printer>>),
    Clear,
    Quit,
}

impl From<String> for Response {
    fn from(value: String) -> Self {
        Response::Output(Arc::from(value))
    }
}

impl<'a> From<&'a str> for Response {
    fn from(value: &'a str) -> Self {
        Response::Output(Arc::from(value))
    }
}

impl From<ErrorKindOf> for Response {
    fn from(value: ErrorKindOf) -> Self {
        Response::Error(value)
    }
}

impl From<Printer> for Response {
    fn from(value: Printer) -> Self {
        Response::AutoConnect(Arc::new(Mutex::new(value)))
    }
}
