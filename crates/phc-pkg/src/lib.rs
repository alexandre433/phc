// SPDX-License-Identifier: MIT
//! Package metadata + (eventually) dependency resolution.
//!
//! This first slice covers `phc.json` reading per D-020. Dependency
//! resolution, lock files, and registries land alongside the
//! multi-file build pipeline.

mod discover;
mod manifest;

#[cfg(test)]
mod tests;

pub use discover::discover_sources;
pub use manifest::{read_manifest, Dependencies, Manifest, ManifestError};
