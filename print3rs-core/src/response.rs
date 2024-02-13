use winnow::{
    ascii::{dec_int, dec_uint, multispace0, space0},
    combinator::{alt, opt, preceded, terminated},
    prelude::*,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Response {
    Ok,
    SequencedOk(i32),
    Resend(i32),
}

fn ok_response(input: &mut &[u8]) -> PResult<Response> {
    match preceded(
        (space0, "ok", opt(":"), space0, opt(b'N')),
        terminated(opt(dec_int), multispace0),
    )
    .parse_next(input)?
    {
        Some(num) => Ok(Response::SequencedOk(num)),
        None => Ok(Response::Ok),
    }
}

fn resend_response(input: &mut &[u8]) -> PResult<Response> {
    let sequence = preceded(
        (space0, "Resend:", space0),
        terminated(dec_int, multispace0),
    )
    .parse_next(input)?;
    Ok(Response::Resend(sequence))
}

pub fn response(input: &mut &[u8]) -> PResult<Response> {
    alt((ok_response, resend_response)).parse_next(input)
}
