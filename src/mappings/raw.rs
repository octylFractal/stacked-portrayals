use std::collections::HashMap;

use crate::mappings::{
    BaseMapper, ClassMapping, Descriptor, FnClassMapper, MapSelfOnlyClass, Mappings, MethodId,
};
use crate::names::NamesType;

pub struct RawClassMapping<M> {
    pub mapping: (String, String),
    pub methods: M,
}

pub struct RawMethodMapping {
    pub descriptor: Descriptor,
    pub mapping: (String, String),
}

#[tracing::instrument(skip(mappings), level = "debug")]
pub fn convert_mappings<CM, M>(
    primary_nt: NamesType,
    secondary_nt: NamesType,
    version: String,
    mappings: CM,
    should_flip: bool,
) -> BaseMapper
where
    CM: IntoIterator<Item = RawClassMapping<M>>,
    M: IntoIterator<Item = RawMethodMapping>,
{
    let mappings = mappings.into_iter();

    let guess_size = mappings.size_hint().0.max(8);

    let mut class_mappings = HashMap::with_capacity(guess_size);
    let mut todo_classes = Vec::with_capacity(guess_size);
    for class_mapping in mappings {
        // fix inference
        let class_mapping: RawClassMapping<M> = class_mapping;
        class_mappings.insert(
            class_mapping.mapping.0.clone(),
            class_mapping.mapping.1.clone(),
        );
        todo_classes.push(class_mapping);
    }
    let class_mappings =
        FnClassMapper::new(class_mappings, |cm, name| cm.get(name).map(|s| s.as_str()));
    let result = todo_classes
        .into_iter()
        .map(|class| {
            let (from, to) = do_flip(should_flip, class.mapping);
            let methods = class
                .methods
                .into_iter()
                .map(|method: RawMethodMapping| {
                    let first_id = MethodId {
                        name: method.mapping.0,
                        descriptor: method.descriptor.clone(),
                    };
                    let second_id = MethodId {
                        name: method.mapping.1,
                        descriptor: method.descriptor.map_self(&class_mappings),
                    };
                    do_flip(should_flip, (first_id, second_id))
                })
                .collect();
            (
                from,
                ClassMapping {
                    to_name: to,
                    methods,
                },
            )
        })
        .collect();
    let (from, to) = do_flip(should_flip, (primary_nt, secondary_nt));
    let mappings = Mappings { classes: result };
    tracing::trace!("Converted mappings: {:#?}", mappings);
    BaseMapper {
        from,
        to,
        version,
        mappings,
    }
}

fn do_flip<T>(should_flip: bool, (first, second): (T, T)) -> (T, T) {
    if should_flip {
        (second, first)
    } else {
        (first, second)
    }
}
