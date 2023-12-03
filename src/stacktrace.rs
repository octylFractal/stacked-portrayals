use std::borrow::Cow;
use std::fmt::{Display, Formatter};

use chumsky::error::Simple;
use chumsky::prelude::end;
use chumsky::primitive::just;
use chumsky::text::whitespace;
use chumsky::Parser;
use error_stack::Report;
use itertools::Itertools;

use crate::mappings::{MapSelf, MapSelfOnlyClass, MethodMapper, Type};
use crate::parsing::{
    eol, handle_errors, inline_whitespace, jtype, parse_recovery_debuggable, u32_digits, CharParser,
};
use crate::SPError;

#[derive(Debug)]
pub struct Stacktrace {
    pub ty: Type,
    pub message: String,
    pub frames: Vec<Frame>,
}

impl Display for Stacktrace {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}: {}", self.ty, self.message)?;
        for frame in &self.frames {
            writeln!(f, "\tat {}", frame)?;
        }
        Ok(())
    }
}

impl MapSelf for Stacktrace {
    fn map_self(self, mapper: &impl MethodMapper) -> Self {
        Self {
            ty: self.ty.map_self(mapper),
            message: self.message,
            frames: self
                .frames
                .into_iter()
                .map(|f| f.map_self(mapper))
                .collect(),
        }
    }
}

#[derive(Debug)]
pub struct Frame {
    pub module: Option<String>,
    pub class: String,
    pub method: String,
    pub file: String,
    pub line: Option<u32>,
}

impl Display for Frame {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(module) = &self.module {
            write!(f, "{}/", module)?;
        }
        write!(f, "{}.{}({}", self.class, self.method, self.file)?;
        if let Some(line) = self.line {
            write!(f, ":{}", line)?;
        }
        write!(f, ")")
    }
}

impl MapSelf for Frame {
    fn map_self(self, mapper: &impl MethodMapper) -> Self {
        let methods = mapper.map_method(&self.class.to_string(), &self.method, None);
        let method = if methods.is_empty() {
            self.method
        } else {
            methods.into_iter().map(|(_, m)| &m.name).join("/")
        };
        let mapped_file = self.file.split_once('.').and_then(|(name, ext)| {
            let as_class_name: Cow<str> = match self.class.rsplit_once('.') {
                Some((pkg, _)) => format!("{}.{}", pkg, name).into(),
                None => name.into(),
            };
            let _enter = tracing::debug_span!("frame_map_file", ?as_class_name, ?ext).entered();
            mapper.map_class(&as_class_name).map(|c| {
                format!(
                    "{}.{}",
                    match c.rsplit_once('.') {
                        Some((_, class)) => class,
                        None => &c,
                    },
                    ext
                )
            })
        });
        Self {
            module: self.module,
            class: mapper
                .map_class(&self.class)
                .map_or(self.class, |c| c.into()),
            method,
            file: mapped_file.unwrap_or(self.file),
            line: self.line,
        }
    }
}

pub fn parse_stacktrace(input: &str) -> Result<Stacktrace, Report<SPError>> {
    let res = parse_recovery_debuggable(stacktrace(), input);
    handle_errors(input, res, "Failed to parse stacktrace")
}

fn stacktrace() -> impl CharParser<Stacktrace> {
    jtype()
        .map(Type::from_source_name)
        .labelled("type")
        .then_ignore(just(": "))
        .then(eol().not().repeated().collect().labelled("message"))
        .then_ignore(eol())
        .then(frame().repeated())
        .then_ignore(whitespace().then(end()))
        .map(|((ty, message), frames)| Stacktrace {
            ty,
            message,
            frames,
        })
}

fn frame() -> impl CharParser<Frame> {
    inline_whitespace()
        .ignore_then(just("at "))
        .ignore_then(jtype().labelled("module").then_ignore(just("/")).or_not())
        .then(
            // This is a little tricky, since there is no clear delimiter between the class and the method.
            // Use jtype, which will read the method too, and split it on the last '.'.
            jtype()
                .labelled("class+method")
                .try_map(|class_method, span| {
                    let last_dot = class_method
                        .rfind('.')
                        .ok_or_else(|| Simple::custom(span, "no class name found in stacktrace"))?;
                    let (class, method) = class_method.split_at(last_dot);
                    let method = &method[1..];
                    Ok((class.to_string(), method.to_string()))
                }),
        )
        .then(
            // jtype is basically good for this too
            jtype()
                .labelled("file")
                .then(
                    just(":")
                        .ignore_then(u32_digits().labelled("line number"))
                        .or_not(),
                )
                .delimited_by(just("("), just(")")),
        )
        .then_ignore(
            just("]")
                .not()
                .repeated()
                .delimited_by(just(" ~["), just("]"))
                .or_not(),
        )
        .then_ignore(eol())
        .map(|((module, (class, method)), (file, line))| Frame {
            module,
            class,
            method: method.to_string(),
            file,
            line,
        })
}
