use ariadne::{Color, Label, Source};
use chumsky::error::Simple;
use chumsky::primitive::{filter, just};
use chumsky::text::Character;
use chumsky::{text, Error, Parser, Stream};
use error_stack::Report;

use crate::SPError;

pub fn parse_recovery_debuggable<'a, P, I: Clone, O, E: Error<I>, Iter, S>(
    parser: P,
    stream: S,
) -> (Option<O>, Vec<E>)
where
    P: Parser<I, O, Error = E> + Sized,
    Iter: Iterator<Item = (I, E::Span)> + 'a,
    S: Into<Stream<'a, I, E::Span, Iter>>,
{
    #[cfg(feature = "debug")]
    {
        parser.parse_recovery_verbose(stream)
    }
    #[cfg(not(feature = "debug"))]
    {
        parser.parse_recovery(stream)
    }
}

pub fn handle_errors<O>(
    source: &str,
    (output, errors): (Option<O>, Vec<Simple<char>>),
    message: &str,
) -> Result<O, Report<SPError>> {
    if errors.is_empty() {
        Ok(output.unwrap())
    } else {
        Err(Report::new(SPError)
            .attach_printable(message.to_string())
            .attach(ParseErrors {
                source: source.to_string(),
                errors,
            }))
    }
}

pub struct ParseErrors {
    source: String,
    errors: Vec<Simple<char>>,
}

impl ParseErrors {
    pub fn eprint(&self) {
        for err in &self.errors {
            ariadne::Report::build(ariadne::ReportKind::Error, (), err.span().start)
                .with_message(err.to_string())
                .with_label(
                    Label::new(err.span())
                        .with_message(format!("{:?}", err.reason()))
                        .with_color(Color::Red),
                )
                .finish()
                .eprint(Source::from(self.source.clone()))
                .expect("Failed to print error");
        }
    }
}

pub trait CharParser<T>: Parser<char, T, Error = Simple<char>> {}

impl<T, P> CharParser<T> for P where P: Parser<char, T, Error = Simple<char>> {}

pub fn is_java_identifier_part(c: char) -> bool {
    is_java_digit(c) || is_java_letter(c)
}
pub fn is_java_letter(c: char) -> bool {
    c.is_alphabetic() || c == '$' || c == '_'
}
pub fn is_java_digit(c: char) -> bool {
    c.is_ascii_digit()
}

// Sloppy, but it should be fine.
pub fn jtype() -> impl CharParser<String> {
    filter(|&c| is_java_identifier_part(c) || c == '.' || c == '[' || c == ']' || c == '-')
        .repeated()
        .at_least(1)
        .collect()
}

// Sloppy, but it should be fine.
pub fn jname() -> impl CharParser<String> {
    filter(|&c| is_java_identifier_part(c))
        .repeated()
        .at_least(1)
        .collect()
        .or(just("<init>").map(String::from))
        .or(just("<clinit>").map(String::from))
}

pub fn inline_whitespace() -> impl CharParser<()> {
    filter(|c: &char| c.is_inline_whitespace())
        .repeated()
        .ignored()
}

pub fn eol() -> impl CharParser<()> {
    just("\n").or(just("\r\n")).ignored()
}

pub fn u32_digits() -> impl CharParser<u32> {
    text::digits(10).try_map(|s: String, span| {
        s.parse::<u32>()
            .map_err(|e| Simple::custom(span, format!("{}", e)))
    })
}
