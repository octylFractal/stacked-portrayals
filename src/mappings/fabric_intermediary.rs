use std::io::Read;

use error_stack::{Report, ResultExt};
use zip::ZipArchive;

use crate::http::good_error_request;
use crate::mappings::cache::{load_mappings, HashCode, MappingDownload};
use crate::mappings::raw::{RawClassMapping, RawMethodMapping};
use crate::mappings::tiny::parse_tiny_v2;
use crate::mappings::{raw, BaseMapper};
use crate::names::NamesType;
use crate::SPError;

pub fn load(version: String, obf_to_fabric: bool) -> Result<BaseMapper, Report<SPError>> {
    let content = extract_mappings(&version)?;

    let mappings = parse_tiny_v2(&content)?;

    // Sanity check that we got the mapping we expected.
    if mappings.header.namespace_a != "official" || mappings.header.namespace_b != "intermediary" {
        return Err(Report::new(SPError)
            .attach_printable(format!("Invalid tiny mappings for {}", version))
            .attach_printable(format!("Header: {:?}", mappings.header)));
    }

    Ok(raw::convert_mappings(
        NamesType::Obfuscated,
        NamesType::FabricIntermediary,
        version,
        mappings.content.classes.into_iter().filter_map(|c| {
            Some(RawClassMapping {
                mapping: (
                    c.mapping.primary_name,
                    c.mapping
                        .mapped_names
                        .into_iter()
                        .next()
                        .expect("missing first mapped name")?,
                ),
                methods: c.methods.into_iter().filter_map(|m| {
                    Some(RawMethodMapping {
                        descriptor: m.primary_desc,
                        mapping: (
                            m.mapping.primary_name,
                            m.mapping
                                .mapped_names
                                .into_iter()
                                .next()
                                .expect("missing first mapped name")?,
                        ),
                    })
                }),
            })
        }),
        !obf_to_fabric,
    ))
}

fn extract_mappings(version: &String) -> Result<String, Report<SPError>> {
    let dl = fetch_mappings_info(version)?;
    let mappings = load_mappings(dl)?;
    let mut zip = ZipArchive::new(mappings)
        .change_context(SPError)
        .attach_printable_lazy(|| format!("Failed to open mappings JAR for {}", version))?;
    let mut tiny_file = zip
        .by_name("mappings/mappings.tiny")
        .change_context(SPError)
        .attach_printable_lazy(|| format!("Failed to get mappings.tiny for {}", version))?;
    let mut content = String::new();
    tiny_file
        .read_to_string(&mut content)
        .change_context(SPError)
        .attach_printable_lazy(|| format!("Failed to read mappings.tiny for {}", version))?;
    Ok(content)
}

const BASE_URL: &str = "https://maven.fabricmc.net/net/fabricmc/intermediary";

fn artifact_url(version: &str) -> String {
    format!("{}/{}/intermediary-{}-v2.jar", BASE_URL, version, version)
}

fn fetch_mappings_info(version: &str) -> Result<MappingDownload, Report<SPError>> {
    let url = artifact_url(version);
    let sha512 = good_error_request(&format!("{}.sha512", url))?
        .text()
        .change_context(SPError)
        .attach_printable_lazy(|| format!("Failed to get sha512 for {}", version))?;
    Ok(MappingDownload {
        kind: "fabric_intermediary".into(),
        source: url,
        hash: HashCode::Sha512(sha512),
        size: None,
    })
}
