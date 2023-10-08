use std::collections::HashMap;
use std::fmt::{Debug, Display};

use derive_more::Display;
use error_stack::{Report, ResultExt};
use once_cell::sync::Lazy;
use petgraph::algo::astar;
use petgraph::graphmap::DiGraphMap;

use crate::names::NamesType;
use crate::SPError;

pub mod cache;
mod db;
mod fabric_intermediary;
mod mojang;
mod proguard;
mod raw;
mod tiny;

type MappingsGraph = DiGraphMap<NamesType, MappingType>;

/// Graph with nodes of [`NamesType`]s and edges of [`MappingLoader`]s.
static MAPPINGS_GRAPH: Lazy<MappingsGraph> = Lazy::new(|| {
    DiGraphMap::from_edges(&[
        (
            NamesType::Obfuscated,
            NamesType::Mojang,
            MappingType::ObfToMojang,
        ),
        (
            NamesType::Mojang,
            NamesType::Obfuscated,
            MappingType::MojangToObf,
        ),
        (
            NamesType::Obfuscated,
            NamesType::FabricIntermediary,
            MappingType::ObfToFabricIntermediary,
        ),
        (
            NamesType::FabricIntermediary,
            NamesType::Obfuscated,
            MappingType::FabricIntermediaryToObf,
        ),
    ])
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MappingType {
    ObfToMojang,
    MojangToObf,
    ObfToFabricIntermediary,
    FabricIntermediaryToObf,
}

impl MappingType {
    fn load(self, version: String) -> Result<BaseMapper, Report<SPError>> {
        // TODO: Find some better way to encode this than a fucking bool, this is awful...
        match self {
            Self::ObfToMojang => mojang::load(version, false),
            Self::MojangToObf => mojang::load(version, true),
            Self::ObfToFabricIntermediary => fabric_intermediary::load(version, true),
            Self::FabricIntermediaryToObf => fabric_intermediary::load(version, false),
        }
    }
}

#[derive(Debug)]
pub struct Mappings {
    /// Indexed by the `from` name.
    pub classes: HashMap<String, ClassMapping>,
}

#[derive(Debug)]
pub struct ClassMapping {
    to_name: String,
    /// Indexed by the `from` ID, to the `to` ID.
    methods: HashMap<MethodId, MethodId>,
}

pub trait ClassMapper: Debug + Display {
    /// Map a class name.
    fn map_class(&self, name: &str) -> Option<&str>;
}

pub trait MethodMapper: ClassMapper {
    /// Map a class-specific method name. Uses the original class name to determine the class.
    /// If there is no entry, also tries the `name` alone, and if the result is unique, returns
    /// that. This is a hack for synthetic methods.
    ///
    /// # Returns
    /// A list of all possible mappings for the method, and their corresponding class names.
    fn map_method(
        &self,
        from_class_name: &str,
        name: &str,
        descriptor: Option<&Descriptor>,
    ) -> Vec<(&str, &MethodId)>;
}

pub trait MapSelfOnlyClass {
    fn map_self(self, mapper: &impl ClassMapper) -> Self;
}

pub trait MapSelf {
    fn map_self(self, mapper: &impl MethodMapper) -> Self;
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct MethodId {
    pub name: String,
    pub descriptor: Descriptor,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Descriptor {
    pub params: Vec<Type>,
    pub return_type: Type,
}

impl MapSelfOnlyClass for Descriptor {
    fn map_self(self, mapper: &impl ClassMapper) -> Self {
        Self {
            params: self
                .params
                .into_iter()
                .map(|t| t.map_self(mapper))
                .collect(),
            return_type: self.return_type.map_self(mapper),
        }
    }
}

#[derive(Debug, Display, Clone, Eq, PartialEq, Hash)]
pub enum Type {
    #[display(fmt = "void")]
    Void,
    #[display(fmt = "boolean")]
    Boolean,
    #[display(fmt = "byte")]
    Byte,
    #[display(fmt = "char")]
    Char,
    #[display(fmt = "short")]
    Short,
    #[display(fmt = "int")]
    Int,
    #[display(fmt = "long")]
    Long,
    #[display(fmt = "float")]
    Float,
    #[display(fmt = "double")]
    Double,
    /// Object type, e.g. `java.lang.String`.
    #[display(fmt = "{}", _0)]
    Object(String),
    #[display(fmt = "{}[]", _0)]
    Array(Box<Type>),
}

impl Type {
    pub fn from_source_name(name: String) -> Self {
        match name.as_str() {
            "void" => Self::Void,
            "boolean" => Self::Boolean,
            "byte" => Self::Byte,
            "char" => Self::Char,
            "short" => Self::Short,
            "int" => Self::Int,
            "long" => Self::Long,
            "float" => Self::Float,
            "double" => Self::Double,
            _ => {
                if let Some(non_array_name) = name.strip_suffix("[]") {
                    Self::Array(Box::new(Self::from_source_name(non_array_name.to_string())))
                } else {
                    Self::Object(name)
                }
            }
        }
    }
}

impl MapSelfOnlyClass for Type {
    fn map_self(self, mapper: &impl ClassMapper) -> Self {
        match self {
            Self::Object(name) => {
                if let Some(mapped_name) = mapper.map_class(&name) {
                    Self::Object(mapped_name.to_string())
                } else {
                    Self::Object(name)
                }
            }
            Self::Array(ty) => Self::Array(Box::new(ty.map_self(mapper))),
            _ => self,
        }
    }
}

#[derive(Display)]
#[display(fmt = "fn class mapper")]
pub struct FnClassMapper<D, F> {
    data: D,
    mapper: F,
}

impl<D: Debug, F: for<'a> Fn(&'a D, &str) -> Option<&'a str>> FnClassMapper<D, F> {
    pub fn new(data: D, mapper: F) -> Self {
        Self { data, mapper }
    }
}

impl<D: Debug, F> Debug for FnClassMapper<D, F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FnClassMapper")
            .field("data", &self.data)
            .field("mappings", &"<function>")
            .finish()
    }
}

impl<D: Debug, F: for<'a> Fn(&'a D, &str) -> Option<&'a str>> ClassMapper for FnClassMapper<D, F> {
    fn map_class(&self, name: &str) -> Option<&str> {
        (self.mapper)(&self.data, name)
    }
}

#[derive(Debug, Display)]
#[display(fmt = "{} -> {} for {}", from, to, version)]
pub struct BaseMapper {
    from: NamesType,
    to: NamesType,
    version: String,
    mappings: Mappings,
}

impl ClassMapper for BaseMapper {
    #[tracing::instrument(ret, skip(self), fields(self_d = %self), level = "debug")]
    fn map_class(&self, name: &str) -> Option<&str> {
        self.mappings.classes.get(name).map(|c| c.to_name.as_str())
    }
}

impl MethodMapper for BaseMapper {
    #[tracing::instrument(ret, skip(self), fields(self_d = %self), level = "debug")]
    fn map_method(
        &self,
        from_class_name: &str,
        name: &str,
        descriptor: Option<&Descriptor>,
    ) -> Vec<(&str, &MethodId)> {
        let scoped_result: Vec<_> = self
            .mappings
            .classes
            .get(from_class_name)
            .into_iter()
            .flat_map(|c| extract_method(name, descriptor, c))
            .collect();
        if !scoped_result.is_empty() {
            return scoped_result;
        }
        let unscoped_result: Vec<_> = self
            .mappings
            .classes
            .values()
            .flat_map(|c| extract_method(name, descriptor, c))
            .collect();
        if unscoped_result.len() <= 1 {
            return unscoped_result;
        }
        vec![]
    }
}

fn extract_method<'a>(
    name: &str,
    descriptor: Option<&Descriptor>,
    c: &'a ClassMapping,
) -> Vec<(&'a str, &'a MethodId)> {
    if let Some(desc) = descriptor {
        c.methods
            .get(&MethodId {
                name: name.to_string(),
                descriptor: desc.clone(),
            })
            .into_iter()
            .map(|m| (c.to_name.as_str(), m))
            .collect()
    } else {
        c.methods
            .iter()
            .filter_map(|(from, to)| (from.name == name).then_some((c.to_name.as_str(), to)))
            .collect()
    }
}

pub fn generate_mapper(
    version: String,
    from: NamesType,
    to: NamesType,
) -> Result<EitherMapper, Report<SPError>> {
    let g = &*MAPPINGS_GRAPH;
    let path = astar(g, from, |finish| finish == to, |_| 1, |_| 0)
        .ok_or_else(|| Report::from(SPError))
        .attach_printable_lazy(|| format!("No path from {} to {}", from, to))?
        .1;
    assert!(path.len() >= 2, "Path must have at least two elements");

    fn sanity_check_mapper(m: BaseMapper, from: NamesType, to: NamesType) -> BaseMapper {
        assert_eq!(
            m.from, from,
            "found bad mapper in mapping graph when looking for {} -> {}",
            from, to
        );
        assert_eq!(
            m.to, to,
            "found bad mapper in mapping graph when looking for {} -> {}",
            from, to
        );
        m
    }

    if path.len() == 2 {
        return MAPPINGS_GRAPH
            .edge_weight(path[0], path[1])
            .expect("astar gave a path with no edge")
            .load(version)
            .map(|b| EitherMapper::Base(sanity_check_mapper(b, from, to)));
    }
    let mut mappers = Vec::with_capacity(path.len() - 1);
    for i in 0..path.len() - 1 {
        let mapper = MAPPINGS_GRAPH
            .edge_weight(path[i], path[i + 1])
            .expect("astar gave a path with no edge")
            .load(version.clone())?;
        mappers.push(sanity_check_mapper(mapper, path[i], path[i + 1]));
    }
    Ok(EitherMapper::Multi(MultiMapper { mappers }))
}

#[derive(Debug)]
pub enum EitherMapper {
    Base(BaseMapper),
    Multi(MultiMapper),
}

impl Display for EitherMapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EitherMapper::Base(m) => Display::fmt(m, f),
            EitherMapper::Multi(m) => Display::fmt(m, f),
        }
    }
}

impl ClassMapper for EitherMapper {
    fn map_class(&self, name: &str) -> Option<&str> {
        match self {
            EitherMapper::Base(m) => m.map_class(name),
            EitherMapper::Multi(m) => m.map_class(name),
        }
    }
}

impl MethodMapper for EitherMapper {
    fn map_method(
        &self,
        from_class_name: &str,
        name: &str,
        descriptor: Option<&Descriptor>,
    ) -> Vec<(&str, &MethodId)> {
        match self {
            EitherMapper::Base(m) => m.map_method(from_class_name, name, descriptor),
            EitherMapper::Multi(m) => m.map_method(from_class_name, name, descriptor),
        }
    }
}

#[derive(Debug)]
pub struct MultiMapper {
    mappers: Vec<BaseMapper>,
}

impl Display for MultiMapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.mappers.iter().map(|m| m.to_string()))
            .finish()
    }
}

impl ClassMapper for MultiMapper {
    #[tracing::instrument(ret, skip(self), fields(self_d = %self), level = "debug")]
    fn map_class(&self, name: &str) -> Option<&str> {
        let mut push_name = name;
        let mut ret_name = None;
        for mapper in &self.mappers {
            let mapped_name = mapper.map_class(push_name)?;
            push_name = mapped_name;
            ret_name = Some(mapped_name);
        }
        ret_name
    }
}

impl MethodMapper for MultiMapper {
    #[tracing::instrument(ret, skip(self), fields(self_d = %self), level = "debug")]
    fn map_method(
        &self,
        from_class_name: &str,
        name: &str,
        descriptor: Option<&Descriptor>,
    ) -> Vec<(&str, &MethodId)> {
        let mut push_data = vec![(from_class_name, name, descriptor)];
        let mut ret_ids = vec![];
        for mapper in &self.mappers {
            // drop existing set of return values first
            ret_ids.clear();
            for (class_name, name, descriptor) in push_data.drain(..) {
                let mapped_names = mapper.map_method(class_name, name, descriptor);
                ret_ids.extend(mapped_names);
            }
            // Now that we have all the values mapped, setup for the next iteration
            push_data.extend(
                ret_ids
                    .iter()
                    .copied()
                    .map(|(cname, id)| (cname, id.name.as_str(), Some(&id.descriptor))),
            );
        }
        ret_ids
    }
}
