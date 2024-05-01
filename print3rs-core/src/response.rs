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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ok_response() {
        let ok = ok_response.parse(b"ok").unwrap();
        assert_eq!(ok, Response::Ok(None));
    }

    #[test]
    fn test_ok_num_response() {
        let ok = ok_response.parse(b"ok: 100").unwrap();
        assert_eq!(ok, Response::Ok(Some(100)));
    }

    #[test]
    fn test_resend_response() {
        let ok = resend_response.parse(b"Resend: 100").unwrap();
        assert_eq!(ok, Response::Resend(Some(100)));
    }

    #[test]
    fn test_response() {
        let ok = response.parse(b"ok").unwrap();
        assert_eq!(ok, Response::Ok(None));
        let ok = response.parse(b"ok: 100").unwrap();
        assert_eq!(ok, Response::Ok(Some(100)));
        let ok = response.parse(b"Resend: 100").unwrap();
        assert_eq!(ok, Response::Resend(Some(100)));
    }
}
