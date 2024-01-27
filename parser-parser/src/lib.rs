use winnow::{
    ascii::{alphanumeric1, float},
    combinator::{alt, preceded, repeat, terminated},
    prelude::*,
    token::take_till,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Segment<'a> {
    Tag(&'a str),
    Value(&'a str),
}

fn parse_tag<'a>(input: &mut &'a str) -> PResult<Segment<'a>> {
    Ok(Segment::Tag(take_till(1.., '{').parse_next(input)?))
}

fn parse_value<'a>(input: &mut &'a str) -> PResult<Segment<'a>> {
    Ok(Segment::Value(
        preceded("{", terminated(alphanumeric1.recognize(), "}")).parse_next(input)?,
    ))
}

fn parse_segment<'a>(input: &mut &'a str) -> PResult<Segment<'a>> {
    alt((parse_tag, parse_value)).parse_next(input)
}

pub fn parse_segments<'a>(input: &mut &'a str) -> PResult<Vec<Segment<'a>>> {
    repeat(0.., parse_segment).parse_next(input)
}

pub fn make_parser<'a, 'b>(
    segments: &'b [Segment<'a>],
) -> impl FnMut(&mut &'b str) -> PResult<Vec<f32>> {
    move |input: &mut &'b str| -> PResult<Vec<f32>> {
        let mut values = vec![];
        for segment in segments {
            match segment {
                Segment::Tag(mut s) => {
                    s.parse_next(input)?;
                }
                Segment::Value(_) => {
                    values.push(float.parse_next(input)?);
                }
            };
        }
        Ok(values)
    }
}

pub fn get_headers(segments: &[Segment]) -> String {
    let mut s = String::new();
    for segment in segments {
        if let Segment::Value(label) = segment {
            s.push_str(label);
            s.push(',');
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use Segment::*;

    #[test]
    fn test_parse_segments() {
        let input = " this {is}so12.?me{segm2ents}";
        let expected: &[Segment] = &[
            Tag(" this "),
            Value("is"),
            Tag("so12.?me"),
            Value("segm2ents"),
        ];
        let parsed = parse_segments.parse(input).unwrap();
        assert_eq!(expected, parsed);
    }

    #[test]
    fn test_headers() {
        let segments = [Tag("one"), Value("two"), Tag("three"), Value("four")];
        let headers = get_headers(&segments);
        assert_eq!(&headers, "two,four,");
    }

    #[test]
    fn test_parsed_parser() {
        let parse_pattern = "millis: {millis},pos:{pos},current:{current}";
        let segments = parse_segments.parse(parse_pattern).unwrap();
        let mut parser = make_parser(&segments);
        let final_out = parser.parse("millis: 1234.5,pos:-4.0,current:100").unwrap();
        assert_eq!(final_out, vec![1234.5, -4.0, 100.0]);
    }
}
