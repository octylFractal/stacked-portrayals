use std::fmt::Debug;
use std::io::Read;

use clap::Parser;
use derive_more::Display;
use error_stack::{Context, Report, ResultExt};

use crate::mappings::{generate_mapper, MapSelf};
use crate::names::NamesType;
use crate::parsing::ParseErrors;
use crate::stacktrace::parse_stacktrace;

mod http;
mod mappings;
mod mojang_api;
mod names;
mod parsing;
mod stacktrace;

/// Reads a stacktrace from stdin and maps the names according plan.
///
/// Note that a stacktrace cannot uniquely identify a method, so the mapping
/// may give multiple results. In this case, the methods are joined with a `/`.
#[derive(Parser, Debug)]
#[clap(version)]
struct StackedPortrayals {
    /// The version of Minecraft to use.
    mc_version: String,
    /// The names to start with.
    ///
    #[doc = include_str!("docs/name_types.md")]
    from_names: NamesType,
    /// The names to end with.
    ///
    #[doc = include_str!("docs/name_types.md")]
    to_names: NamesType,
    /// Verbosity level, repeat to increase.
    #[clap(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Debug, Display)]
pub struct SPError;

impl Context for SPError {}

fn main() -> Result<(), Report<SPError>> {
    let args = StackedPortrayals::parse();
    let env_filt = tracing_subscriber::filter::EnvFilter::builder()
        .with_default_directive(
            match args.verbose {
                0 => tracing_subscriber::filter::LevelFilter::INFO,
                1 => tracing_subscriber::filter::LevelFilter::DEBUG,
                _ => tracing_subscriber::filter::LevelFilter::TRACE,
            }
            .into(),
        )
        .from_env_lossy()
        // Set some loud things to warn
        .add_directive("reqwest=warn".parse().unwrap())
        .add_directive("hyper=warn".parse().unwrap());
    tracing_subscriber::fmt().with_env_filter(env_filt).init();

    if let Err(e) = main_for_result(args) {
        if let Some(parse) = e.downcast_ref::<ParseErrors>() {
            parse.eprint();
        }
        return Err(e);
    }

    Ok(())
}

fn main_for_result(args: StackedPortrayals) -> Result<(), Report<SPError>> {
    let stacktrace = {
        let mut buf = String::new();
        tracing::info!("Enter stacktrace (Ctrl+D to finish):");
        std::io::stdin()
            .read_to_string(&mut buf)
            .change_context(SPError)
            .attach_printable("Failed to read stacktrace from stdin")?;
        buf
    };
    let stacktrace = parse_stacktrace(&stacktrace)?;

    tracing::info!("Generating mapper...");
    let mapper = generate_mapper(args.mc_version, args.from_names, args.to_names)
        .attach_printable_lazy(|| {
            format!(
                "Failed to generate mapper from {} to {}",
                args.from_names, args.to_names
            )
        })?;

    tracing::info!("Mapping stacktrace...");
    let mapped_stacktrace = stacktrace.map_self(&mapper);

    println!("{}", mapped_stacktrace);
    Ok(())
}
