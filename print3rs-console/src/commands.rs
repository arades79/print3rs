use winnow::{
    ascii::{alpha1, space1},
    combinator::{alt, dispatch, empty, preceded, separated},
    prelude::*,
    stream::Stream,
    token::{any, take_till},
};

use crate::logging;

#[derive(Debug)]
pub enum Command<'a> {
    Gcodes(Vec<&'a str>),
    Log(&'a str, Vec<logging::parsing::Segment<'a>>),
    Nothing,
}

fn parse_gcodes<'a>(input: &mut &'a str) -> PResult<Command<'a>> {
    separated(0.., take_till(1.., [';', '\n']), ';')
        .map(|gcode_list| Command::Gcodes(gcode_list))
        .parse_next(input)
}

fn inner_command<'a>(input: &mut &'a str) -> PResult<Command<'a>> {
    dispatch! {alpha1;
    "log" => logging::parsing::parse_command.map(|(name, segments)| Command::Log(name, segments)),
_ => empty.map(|_| Command::Nothing)}
    .parse_next(input)
}

pub fn parse_command<'a>(input: &mut &'a str) -> PResult<Command<'a>> {
    alt((preceded(":", inner_command), parse_gcodes)).parse_next(input)
}
