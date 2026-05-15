// SPDX-License-Identifier: MIT
//! Project source discovery: walk a project root and collect every
//! `.phc` file. Skips `target/`, hidden directories, and any path
//! component starting with `.` so build scratch directories and
//! version-control metadata stay invisible.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Walk `root` and return every `.phc` file beneath it, sorted by
/// path so build output is deterministic.
pub fn discover_sources(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    visit(root, &mut out)?;
    out.sort();
    Ok(out)
}

fn visit(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if is_skipped(&name_str) {
            continue;
        }
        let ft = entry.file_type()?;
        if ft.is_dir() {
            visit(&path, out)?;
        } else if ft.is_file() && path.extension().and_then(|s| s.to_str()) == Some("phc") {
            out.push(path);
        }
    }
    Ok(())
}

fn is_skipped(name: &str) -> bool {
    // Cargo build artefacts and any dotfile / dotdir.
    name == "target" || name.starts_with('.')
}
