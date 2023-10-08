use serde::Deserialize;

use crate::mappings::cache::{HashCode, MappingDownload};

#[derive(Deserialize, Debug)]
pub struct VersionManifest {
    pub versions: Vec<Version>,
}

#[derive(Deserialize, Debug)]
pub struct Version {
    pub id: String,
    pub url: String,
}

#[derive(Deserialize, Debug)]
pub struct VersionInfo {
    pub downloads: Downloads,
}

#[derive(Deserialize, Debug)]
pub struct Downloads {
    // Use client_mappings because it should include the server too.
    pub client_mappings: Download,
}

#[derive(Deserialize, Debug)]
pub struct Download {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

impl From<Download> for MappingDownload {
    fn from(value: Download) -> Self {
        Self {
            kind: "mojang".into(),
            source: value.url,
            hash: HashCode::Sha1(value.sha1),
            size: Some(value.size),
        }
    }
}
