use derive_more::Display;
use error_stack::{Context, Report};
use std::str::FromStr;

#[derive(Debug, Display)]
pub struct NamesFromStrError;

impl Context for NamesFromStrError {}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NamesType {
    #[display(fmt = "obf")]
    Obfuscated,
    #[display(fmt = "mojang")]
    Mojang,
    #[display(fmt = "fabric")]
    FabricIntermediary,
}

impl FromStr for NamesType {
    type Err = Report<NamesFromStrError>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "obf" => Ok(Self::Obfuscated),
            "mojang" => Ok(Self::Mojang),
            "fabric" => Ok(Self::FabricIntermediary),
            _ => Err(Report::new(NamesFromStrError)),
        }
    }
}
