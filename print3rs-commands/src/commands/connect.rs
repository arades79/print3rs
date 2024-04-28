use {
    super::Command,
    print3rs_core::Printer,
    std::{borrow::Borrow, time::Duration},
    tokio::{io::BufReader, time::timeout},
    tokio_serial::{available_ports, SerialPort, SerialPortBuilderExt, SerialPortInfo},
    winnow::{
        ascii::{alpha0, dec_uint, space0, space1},
        combinator::{dispatch, empty, opt, preceded},
        prelude::*,
        token::take_till,
    },
};

pub async fn auto_connect() -> Printer {
    async fn check_port(port: SerialPortInfo) -> Option<Printer> {
        tracing::debug!("checking port {}...", port.port_name);
        let mut printer_port = tokio_serial::new(port.port_name, 115200)
            .timeout(Duration::from_secs(10))
            .open_native_async()
            .ok()?;
        printer_port.write_data_terminal_ready(true).ok()?;
        let printer = Printer::new(BufReader::new(printer_port));

        let look_for_ok = printer.send_unsequenced(b"M115\n").await.ok()?;

        if timeout(Duration::from_secs(5), look_for_ok)
            .await
            .is_ok_and(|inner| inner.is_ok())
        {
            Some(printer)
        } else {
            None
        }
    }
    if let Ok(ports) = available_ports() {
        tracing::info!("found available ports: {ports:?}");
        for port in ports {
            if let Some(printer) = check_port(port).await {
                return printer;
            }
        }
    }
    Printer::Disconnected
}

#[non_exhaustive]
#[derive(Debug, Default, Clone)]
pub enum Connection<S> {
    #[default]
    Auto,
    Serial {
        port: S,
        baud: Option<u32>,
    },
    Tcp {
        hostname: S,
        port: Option<u16>,
    },
    Mqtt {
        hostname: S,
        port: Option<u16>,
        in_topic: Option<S>,
        out_topic: Option<S>,
    },
}

impl<S> Connection<S> {
    pub fn into_owned(self) -> Connection<<S as ToOwned>::Owned>
    where
        S: ToOwned,
    {
        match self {
            Connection::Auto => Connection::Auto,
            Connection::Serial { port, baud } => Connection::Serial {
                port: port.to_owned(),
                baud,
            },
            Connection::Tcp { hostname, port } => Connection::Tcp {
                hostname: hostname.to_owned(),
                port,
            },
            Connection::Mqtt {
                hostname,
                port,
                in_topic,
                out_topic,
            } => Connection::Mqtt {
                hostname: hostname.to_owned(),
                port,
                in_topic: in_topic.map(|s| s.to_owned()),
                out_topic: out_topic.map(|s| s.to_owned()),
            },
        }
    }
    pub fn to_borrowed<Borrowed: ?Sized>(&self) -> Connection<&Borrowed>
    where
        S: Borrow<Borrowed>,
    {
        match self {
            Connection::Auto => Connection::Auto,
            Connection::Serial { port, baud } => Connection::Serial {
                port: port.borrow(),
                baud: *baud,
            },
            Connection::Tcp { hostname, port } => Connection::Tcp {
                hostname: hostname.borrow(),
                port: *port,
            },
            Connection::Mqtt {
                hostname,
                port,
                in_topic,
                out_topic,
            } => Connection::Mqtt {
                hostname: hostname.borrow(),
                port: *port,
                in_topic: in_topic.as_ref().map(|s| s.borrow()),
                out_topic: out_topic.as_ref().map(|s| s.borrow()),
            },
        }
    }
}

fn parse_serial_connection<'a>(input: &mut &'a str) -> PResult<Connection<&'a str>> {
    let (port, baud) = (
        preceded(space0, take_till(1.., ' ')),
        preceded(space1, opt(dec_uint)),
    )
        .parse_next(input)?;
    Ok(Connection::Serial { port, baud })
}

fn parse_tcp_connection<'a>(input: &mut &'a str) -> PResult<Connection<&'a str>> {
    let (hostname, port) = (
        preceded(space0, take_till(1.., ' ')),
        preceded(space1, opt(dec_uint)),
    )
        .parse_next(input)?;
    Ok(Connection::Tcp { hostname, port })
}

fn parse_mqtt_connection<'a>(input: &mut &'a str) -> PResult<Connection<&'a str>> {
    let (hostname, port) = (
        preceded(space0, take_till(1.., ' ')),
        preceded(space1, opt(dec_uint)),
    )
        .parse_next(input)?;
    let (in_topic, out_topic) = (
        preceded(space0, opt(take_till(1.., ' '))),
        preceded(space0, opt(take_till(1.., ' '))),
    )
        .parse_next(input)?;
    Ok(Connection::Mqtt {
        hostname,
        port,
        in_topic,
        out_topic,
    })
}

pub fn parse_connection<'a>(input: &mut &'a str) -> PResult<Command<&'a str>> {
    let connection = dispatch! { preceded(space0, alpha0);
        "serial" => parse_serial_connection,
        "tcp" | "ip" => parse_tcp_connection,
        "mqtt" => parse_mqtt_connection,
        _ => empty.map(|_| Connection::Auto),
    }
    .parse_next(input)?;
    Ok(Command::Connect(connection))
}
