use winnow::{
    ascii::{dec_int, multispace0, space0},
    combinator::{alt, opt, preceded, terminated},
    prelude::*,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Response {
    Ok(Option<i32>),
    Resend(Option<i32>),
}

fn ok_response(input: &mut &[u8]) -> PResult<Response> {
    preceded(
        (space0, "ok", opt(":"), space0, opt(b'N')),
        terminated(opt(dec_int), multispace0),
    )
    .map(Response::Ok)
    .parse_next(input)
}

fn resend_response(input: &mut &[u8]) -> PResult<Response> {
    preceded(
        (space0, "Resend:", space0),
        terminated(opt(dec_int), multispace0),
    )
    .map(Response::Resend)
    .parse_next(input)
}

pub fn response(input: &mut &[u8]) -> PResult<Response> {
    alt((ok_response, resend_response)).parse_next(input)
}
