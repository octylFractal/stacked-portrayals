use std::iter::once;
use std::str;

use chumsky::primitive::{empty, end, just};
use chumsky::Parser;
use error_stack::Report;

use crate::mappings::{Descriptor, Type};
use crate::parsing::{
    eol, handle_errors, jname, jtype, parse_recovery_debuggable, u32_digits, CharParser,
};
use crate::SPError;

#[derive(Debug)]
pub struct PGMappings {
    pub classes: Vec<PGClass>,
}

pub fn parse_proguard(input: &str) -> Result<PGMappings, Report<SPError>> {
    let res = parse_recovery_debuggable(proguard_mappings(), input);
    handle_errors(input, res, "Failed to parse proguard mappings")
}

fn proguard_mappings() -> impl CharParser<PGMappings> {
    comment()
        .repeated()
        .ignore_then(
            class_section()
                .repeated()
                .map(|classes| PGMappings { classes }),
        )
        .then_ignore(end())
}

fn comment() -> impl CharParser<()> {
    just("#")
        .ignore_then(eol().not().repeated())
        .ignore_then(eol())
}

#[derive(Debug, Clone)]
pub struct PGMapping {
    pub primary_name: String,
    pub secondary_name: String,
}

#[derive(Debug)]
pub struct PGClass {
    pub mapping: PGMapping,
    pub methods: Vec<PGMethod>,
    // Ignoring fields as we don't need them. Might want them later for mapping NPEs.
    // pub fields: HashMap<String, PGField>,
}

fn class_section() -> impl CharParser<PGClass> {
    class_line()
        .then(
            field_line()
                .map(|_| None)
                .or(method_line().map(Some))
                .repeated()
                .map(|methods| methods.into_iter().flatten().collect::<Vec<_>>()),
        )
        .map(|(mapping, methods)| PGClass { mapping, methods })
}

fn class_line() -> impl CharParser<PGMapping> {
    jtype()
        .then_ignore(just(" -> "))
        .then(jtype())
        .then_ignore(just(":").then(eol()))
        .map(|(primary_name, secondary_name)| PGMapping {
            primary_name,
            secondary_name,
        })
}

#[derive(Debug)]
pub struct PGMethod {
    pub primary_descriptor: Descriptor,
    pub mapping: PGMapping,
}

fn method_line() -> impl CharParser<PGMethod> {
    just("    ")
        .ignore_then(line_data().then_ignore(just(":")).or_not())
        .ignore_then(jtype().labelled("return type").debug("return type"))
        .then_ignore(just(" "))
        .then_ignore(
            jtype()
                .delimited_by(empty(), just("."))
                .or_not()
                .labelled("class name prefix"),
        )
        .then(jname().labelled("original method name").debug("mname"))
        .then(
            jtype()
                .then(just(",").ignore_then(jtype()).debug("an arg").repeated())
                .or_not()
                .labelled("method arguments")
                .delimited_by(just("("), just(")"))
                .debug("args"),
        )
        .then_ignore(just(":").then(line_data()).or_not().debug("line data 2"))
        .then_ignore(just(" -> "))
        .then(jname().labelled("obf method name"))
        .then_ignore(eol())
        .map(|(((ret_type, primary_name), params), secondary_name)| {
            let params = match params {
                Some((first, rest)) => once(first)
                    .chain(rest)
                    .map(Type::from_source_name)
                    .collect(),
                None => Vec::new(),
            };
            let primary_descriptor = Descriptor {
                params,
                return_type: Type::from_source_name(ret_type),
            };
            let mapping = PGMapping {
                primary_name,
                secondary_name,
            };
            PGMethod {
                primary_descriptor,
                mapping,
            }
        })
}

fn line_data() -> impl CharParser<(u32, u32)> {
    u32_digits().then_ignore(just(":")).then(u32_digits())
}

fn field_line() -> impl CharParser<()> {
    just("    ")
        .ignore_then(jtype())
        .ignore_then(just(" "))
        .ignore_then(jname())
        .ignore_then(just(" -> "))
        .ignore_then(jname())
        .ignore_then(eol())
}
