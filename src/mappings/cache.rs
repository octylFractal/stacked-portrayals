use crate::http::good_error_request;
use crate::SPError;
use digest::Output;
use directories::ProjectDirs;
use error_stack::{Report, ResultExt};
use once_cell::sync::Lazy;
use sha1::{Digest, Sha1};
use sha2::Sha512;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};

static DIRS: Lazy<ProjectDirs> = Lazy::new(|| {
    ProjectDirs::from("net", "octyl", "stacked-portrayals").expect("Failed to get project dirs")
});

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MappingDownload {
    pub kind: String,
    pub source: String,
    pub hash: HashCode,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HashCode {
    Sha1(String),
    Sha512(String),
}

impl HashCode {
    fn name(&self) -> &'static str {
        match self {
            Self::Sha1(_) => "sha1",
            Self::Sha512(_) => "sha512",
        }
    }

    fn value(&self) -> &str {
        match self {
            Self::Sha1(v) => v,
            Self::Sha512(v) => v,
        }
    }

    fn verify(&self, mut content: impl Read) -> Result<(), Report<SPError>> {
        match self {
            Self::Sha1(expect) => self.verify_impl::<Sha1>(&mut content, expect),
            Self::Sha512(expect) => self.verify_impl::<Sha512>(&mut content, expect),
        }
    }

    fn verify_impl<D>(&self, mut content: impl Read, expect: &str) -> Result<(), Report<SPError>>
    where
        D: Digest + Write,
        Output<D>: std::fmt::LowerHex,
    {
        let mut digest = D::new();
        std::io::copy(&mut content, &mut digest)
            .change_context(SPError)
            .attach_printable("Failed to hash content")?;
        let hash = format!("{:x}", digest.finalize());
        if hash != expect.to_ascii_lowercase() {
            return Err(Report::new(SPError).attach_printable(format!(
                "Mappings file had hash {}, expected {}",
                hash, expect
            )));
        }
        Ok(())
    }
}

pub fn load_mappings(dl: MappingDownload) -> Result<File, Report<SPError>> {
    let cache_file = DIRS.cache_dir().join(format!(
        "{}/{}.{}.mapsrc",
        dl.kind,
        dl.hash.name(),
        dl.hash.value()
    ));
    let mut failures = Vec::new();
    for _attempt in 0..5 {
        let mut file = match File::open(&cache_file) {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                std::fs::create_dir_all(cache_file.parent().unwrap())
                    .change_context(SPError)
                    .attach_printable(format!(
                        "Failed to create cache directory {}",
                        cache_file.parent().unwrap().display()
                    ))?;
                let mut file: File = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .read(true)
                    .open(&cache_file)
                    .change_context(SPError)
                    .attach_printable(format!(
                        "Failed to create mappings cache file {}",
                        cache_file.display()
                    ))?;
                let mut download = good_error_request(&dl.source)?;
                download
                    .copy_to(&mut file)
                    .change_context(SPError)
                    .attach_printable(format!(
                        "Failed to copy mappings to cache file {}",
                        cache_file.display()
                    ))?;
                file.seek(SeekFrom::Start(0))
                    .change_context(SPError)
                    .attach_printable("Failed to reset cached mappings file position")?;
                file
            }
            Err(e) => {
                // likely unrecoverable, bail
                return Err(Report::new(e)
                    .change_context(SPError)
                    .attach_printable(format!(
                        "Failed to open cached mappings {}",
                        cache_file.display()
                    )));
            }
        };
        let Err(validate_error) = validate_mappings(&dl, &mut file) else {
            file.seek(SeekFrom::Start(0))
                .change_context(SPError)
                .attach_printable("Failed to reset cached mappings file position")?;
            return Ok(file);
        };
        drop(file);
        failures.push(validate_error.attach_printable(format!("Source: {}", dl.source)));
        // delete and try again
        std::fs::remove_file(&cache_file)
            .change_context(SPError)
            .attach_printable(format!(
                "Failed to remove invalid cached mappings file {}",
                cache_file.display()
            ))?;
    }
    let mut report = Report::new(SPError).attach_printable(format!(
        "Failed to download and validate mappings {}",
        cache_file.display()
    ));
    for failure in failures {
        report = report.attach_printable(format!("Suppressed: {:?}", failure));
    }
    Err(report)
}

fn validate_mappings(dl: &MappingDownload, mut output: &mut File) -> Result<(), Report<SPError>> {
    let size = output
        .metadata()
        .map(|m| m.len())
        .change_context(SPError)
        .attach_printable("Failed to get tempfile size")?;
    if let Some(expect_size) = dl.size {
        if size != expect_size {
            return Err(Report::new(SPError).attach_printable(format!(
                "Mappings file was {} bytes, expected {}",
                size, expect_size
            )));
        }
    }
    dl.hash.verify(&mut output)
}
