use std::io::Read;

use error_stack::{Report, ResultExt};

use crate::http::good_error_request_json;
use crate::mappings::cache::load_mappings;
use crate::mappings::proguard::parse_proguard;
use crate::mappings::raw::{RawClassMapping, RawMethodMapping};
use crate::mappings::{raw, BaseMapper};
use crate::mojang_api::{Download, VersionInfo, VersionManifest};
use crate::names::NamesType;
use crate::SPError;

pub fn load(version: String, moj_to_obf: bool) -> Result<BaseMapper, Report<SPError>> {
    let content = {
        let dl = fetch_mappings_info(&version)?;
        let mut mappings = load_mappings(dl.into())?;
        let mut content = String::new();
        mappings
            .read_to_string(&mut content)
            .change_context(SPError)
            .attach_printable_lazy(|| format!("Failed to read mappings.tiny for {}", version))?;
        content
    };

    let mappings = parse_proguard(&content)?;

    Ok(raw::convert_mappings(
        NamesType::Mojang,
        NamesType::Obfuscated,
        version,
        mappings.classes.into_iter().map(|c| RawClassMapping {
            mapping: (c.mapping.primary_name, c.mapping.secondary_name),
            methods: c.methods.into_iter().map(|m| RawMethodMapping {
                descriptor: m.primary_descriptor,
                mapping: (m.mapping.primary_name, m.mapping.secondary_name),
            }),
        }),
        !moj_to_obf,
    ))
}

fn fetch_mappings_info(version: &str) -> Result<Download, Report<SPError>> {
    let version_manifest: VersionManifest =
        good_error_request_json("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")?;
    let version = version_manifest
        .versions
        .into_iter()
        .find(|v| v.id == version)
        .ok_or_else(|| {
            Report::new(SPError).attach_printable(format!("No version id matched '{}'", version))
        })?;

    let version_info: VersionInfo = good_error_request_json(&version.url)?;
    let download = version_info.downloads.client_mappings;
    Ok(download)
}
