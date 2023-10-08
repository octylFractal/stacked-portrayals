use std::cell::RefCell;
use std::rc::Rc;
use std::str;

use chumsky::Parser;
use chumsky::prelude::choice;
use chumsky::primitive::{empty, end, filter, just};
use chumsky::recursive::recursive;
use error_stack::Report;

use crate::mappings::{Descriptor, Type};
use crate::parsing::{CharParser, eol, handle_errors, parse_recovery_debuggable};
use crate::SPError;

#[derive(Debug)]
pub struct TinyMappings {
    pub header: TinyHeader,
    pub content: TinyContent,
}

pub fn parse_tiny_v2(input: &str) -> Result<TinyMappings, Report<SPError>> {
    let res = parse_recovery_debuggable(tiny_mappings(), input);
    handle_errors(input, res, "Failed to parse tiny v2 mappings")
}

fn tiny_mappings() -> impl CharParser<TinyMappings> {
    header()
        .then_with(|header| {
            let c = content(&header);
            let header_smuggler = Rc::new(RefCell::new(Some(header)));
            c.map(move |c| {
                // This is a workaround since we know we are only going to be called once.
                // We can't just move the header into the closure because `map` can legally
                // be called multiple times, but we assert here that it won't be.
                let smuggled_header = header_smuggler
                    .take()
                    .expect("content map called more than once");
                TinyMappings {
                    header: smuggled_header,
                    content: c,
                }
            })
        })
        .then_ignore(end())
}

#[derive(Debug)]
pub struct TinyHeader {
    pub namespace_a: String,
    pub namespace_b: String,
    pub extra_namespaces: Vec<String>,
    pub properties: Vec<String>,
}

fn header() -> impl CharParser<TinyHeader> {
    just("tiny\t")
        .labelled("magic")
        .ignore_then(just("2\t"))
        .labelled("major version")
        .ignore_then(just("0\t"))
        .labelled("minor version")
        .ignore_then(
            safe_string()
                .labelled("namespace A")
                .then(
                    just("\t")
                        .ignore_then(safe_string())
                        .labelled("namespace B"),
                )
                // Extra namespaces
                .then(
                    just("\t")
                        .ignore_then(safe_string())
                        .repeated()
                        .labelled("extra namespaces"),
                )
                .then_ignore(eol())
                // Properties
                .then(
                    just("\t")
                        .labelled("Properties tab")
                        .ignore_then(safe_string())
                        .then_ignore(eol())
                        .repeated()
                        .labelled("properties"),
                ),
        )
        .map(
            |(((namespace_a, namespace_b), extra_namespaces), properties)| TinyHeader {
                namespace_a,
                namespace_b,
                extra_namespaces,
                properties,
            },
        )
}

#[derive(Debug)]
pub struct TinyContent {
    pub classes: Vec<TinyClass>,
}

fn content(header: &TinyHeader) -> impl CharParser<TinyContent> {
    class_section(1 + header.extra_namespaces.len())
        .repeated()
        .map(|classes| TinyContent { classes })
}

#[derive(Debug, Clone)]
pub struct TinyMapping {
    pub primary_name: String,
    pub mapped_names: Vec<Option<String>>,
}

#[derive(Debug)]
pub struct TinyClass {
    pub mapping: TinyMapping,
    pub methods: Vec<TinyMethod>,
    // Ignoring fields as we don't need them. Might want them later for mapping NPEs.
    // pub fields: HashMap<String, TinyField>,
}

fn class_section(names_count: usize) -> impl CharParser<TinyClass> {
    just("c\t")
        .ignore_then(safe_string().labelled("primary class name"))
        .then(
            just("\t")
                .ignore_then(conf_safe_string().or_not())
                .repeated()
                .exactly(names_count)
                .labelled("mapped class names"),
        )
        .then_ignore(eol())
        .then(
            field_section()
                .labelled("field section")
                .map(|_| None)
                .or(method_section(names_count)
                    .labelled("method section")
                    .map(Some))
                .repeated()
                .flatten(),
        )
        .map(|((name_a, mapped_names), method_sections)| TinyClass {
            mapping: TinyMapping {
                primary_name: name_a.replace('/', "."),
                mapped_names: mapped_names
                    .into_iter()
                    .map(|n| n.map(|name| name.replace('/', ".")))
                    .collect(),
            },
            methods: method_sections,
        })
}

#[derive(Debug)]
pub struct TinyMethod {
    // Needs mapping into other classes, which can't be done until we have all the classes.
    pub primary_desc: Descriptor,
    pub mapping: TinyMapping,
    // For now, dropping parameters as we don't need them. Might want them later for mapping NPEs.
    // pub parameters: Vec<TinyParameter>,
    // Variables too.
    // pub variables: HashMap<String, TinyVariable>,
}

fn method_section(names_count: usize) -> impl CharParser<TinyMethod> {
    just("\tm\t")
        .ignore_then(descriptor())
        .labelled("method desc a")
        .then_ignore(just("\t"))
        .then(conf_safe_string())
        .labelled("method name a")
        .then(
            just("\t")
                .ignore_then(conf_safe_string().or_not())
                .repeated()
                .exactly(names_count)
                .labelled("mapped method names"),
        )
        .then_ignore(eol())
        .then_ignore(skip_method_subsections())
        .map(|((primary_desc, primary_name), mapped_names)| {
            TinyMethod {
                primary_desc,
                mapping: TinyMapping {
                    primary_name,
                    mapped_names,
                },
            }
        })
}

fn descriptor() -> impl CharParser<Descriptor> {
    descriptor_type()
        .repeated()
        .delimited_by(just("("), just(")"))
        .then(descriptor_type())
        .map(|(params, return_type)| Descriptor {
            params,
            return_type,
        })
}

fn descriptor_type() -> impl CharParser<Type> {
    recursive(|t| {
        choice((
            just("V").map(|_| Type::Void),
            just("Z").map(|_| Type::Boolean),
            just("B").map(|_| Type::Byte),
            just("S").map(|_| Type::Short),
            just("C").map(|_| Type::Char),
            just("I").map(|_| Type::Int),
            just("J").map(|_| Type::Long),
            just("F").map(|_| Type::Float),
            just("D").map(|_| Type::Double),
            type_name()
                .delimited_by(just("L"), just(";"))
                .map(|s| s.replace('/', "."))
                .map(Type::Object),
            t.delimited_by(just("["), empty())
                .map(Box::new)
                .map(Type::Array),
        ))
    })
}

fn skip_method_subsections() -> impl CharParser<()> {
    // Don't really care to parse this exactly
    // Doesn't handle comments for now.
    just("\t\t")
        .ignore_then(eol().not().repeated().ignored())
        .ignore_then(eol())
        .repeated()
        .to(())
}

fn field_section() -> impl CharParser<()> {
    // Don't really care to parse this exactly
    // Doesn't handle comments for now.
    just("\tf")
        .ignore_then(eol().not().ignored().repeated())
        .ignore_then(eol())
}

fn safe_string() -> impl CharParser<String> {
    filter(|&c| c != '\t' && c != '\n' && c != '\r' && c != '\0' && c != '\\')
        .repeated()
        .at_least(1)
        .collect()
}

fn type_name() -> impl CharParser<String> {
    filter(|&c| c != '\t' && c != '\n' && c != '\r' && c != '\0' && c != '\\' && c != ';')
        .repeated()
        .at_least(1)
        .collect()
}

fn conf_safe_string() -> impl CharParser<String> {
    // Later may handle escaped string property.
    safe_string()
}
