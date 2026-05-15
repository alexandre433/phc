// SPDX-License-Identifier: MIT
//! `phc.json` manifest parser per D-020.
//!
//! Required fields: `name` (the dotted pack-path root), `version`
//! (SemVer), `edition` (calendar-year string; `"2026"` for v0).
//! Optional fields: `authors`, `license`, `repository`,
//! `description`, `dependencies`, `dev-dependencies`.
//!
//! Dependencies today are a flat `name → version` map. The richer
//! source/git/path forms land alongside the dependency resolver.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// One parsed `phc.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub edition: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub dependencies: Dependencies,
    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: Dependencies,
}

/// Flat `name → version` map. Will grow into a richer enum (path,
/// git, registry) when the dependency resolver lands.
pub type Dependencies = BTreeMap<String, String>;

/// Errors produced by [`read_manifest`].
#[derive(Debug)]
pub enum ManifestError {
    /// I/O error reading the file (typically: file does not exist).
    Io(std::io::Error),
    /// JSON parse / shape error from serde_json.
    Parse(serde_json::Error),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::Io(e) => write!(f, "{e}"),
            ManifestError::Parse(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ManifestError {}

/// Read and parse the `phc.json` at `path`.
pub fn read_manifest(path: &Path) -> Result<Manifest, ManifestError> {
    let raw = fs::read_to_string(path).map_err(ManifestError::Io)?;
    let manifest: Manifest = serde_json::from_str(&raw).map_err(ManifestError::Parse)?;
    Ok(manifest)
}
