// SPDX-License-Identifier: MIT
//! Multi-file project session.
//!
//! Walks a project root, lexes + parses every `.phc` file, and
//! groups the results by the `pack <path>;` declaration each file
//! ships with. Cross-pack name resolution and pack acyclicity
//! enforcement live in follow-up commits; this module's job is
//! just to get the corpus loaded and indexed.

use phc_ast::{SourceFile, UseDecl};
use phc_errors::{Diagnostic, Severity};
use phc_parser::parse;
use phc_pkg::discover_sources;
use phc_semantic::{resolve, Resolved, SymbolId, Visibility};
use phc_span::{FileId, Span};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// One parsed source file, plus its per-file resolver output.
#[derive(Debug)]
pub struct LoadedFile {
    pub file_id: FileId,
    pub path: PathBuf,
    pub source: String,
    pub ast: SourceFile,
    pub resolved: Resolved,
    /// Dotted pack path joined with `.` — e.g. `app.auth`.
    pub pack: String,
}

/// One side of a resolved cross-pack import: which file owns the
/// imported item and its SymbolId in that file's table.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ImportTarget {
    pub file_idx: usize,
    pub symbol: SymbolId,
}

/// All files in the project plus their by-pack grouping.
#[derive(Debug, Default)]
pub struct Session {
    pub files: Vec<LoadedFile>,
    /// Pack name → indices into `files`. Sorted on insert so iteration
    /// is deterministic.
    pub packs: BTreeMap<String, Vec<usize>>,
    /// Each `use` site span → the resolved import target. Populated
    /// by [`resolve_cross_pack_uses`]; filled in for grouped imports
    /// with one entry per imported name.
    pub cross_uses: HashMap<Span, ImportTarget>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Session {
    pub fn ok(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|d| !matches!(d.severity, Severity::Error))
    }
}

/// Load every `.phc` file under `root`, parse + resolve each one,
/// and group them by their `pack` declaration. Failures (I/O, parse
/// errors, missing pack decl) accumulate in the returned Session's
/// diagnostics.
pub fn load_session(root: &Path) -> Session {
    let mut session = Session::default();
    let paths = match discover_sources(root) {
        Ok(p) => p,
        Err(e) => {
            session.diagnostics.push(diag_error(format!(
                "cannot walk project root `{}`: {e}",
                root.display()
            )));
            return session;
        }
    };
    for (idx, path) in paths.iter().enumerate() {
        let file_id = FileId(idx as u32);
        let source = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                session
                    .diagnostics
                    .push(diag_error(format!("cannot read `{}`: {e}", path.display())));
                continue;
            }
        };
        let parsed = parse(&source, file_id);
        if !parsed.diagnostics.is_empty() {
            session.diagnostics.extend(parsed.diagnostics);
            continue;
        }
        let Some(ast) = parsed.file else {
            session.diagnostics.push(diag_error(format!(
                "parser produced no SourceFile for `{}`",
                path.display()
            )));
            continue;
        };
        let pack = ast
            .pack
            .path
            .segments
            .iter()
            .map(|i| i.name.as_str())
            .collect::<Vec<_>>()
            .join(".");
        let resolved = resolve(&ast);
        if !resolved.diagnostics.is_empty() {
            session.diagnostics.extend(resolved.diagnostics.clone());
        }
        session.files.push(LoadedFile {
            file_id,
            path: path.clone(),
            source,
            ast,
            resolved,
            pack,
        });
    }
    // Group by pack.
    for (idx, file) in session.files.iter().enumerate() {
        session
            .packs
            .entry(file.pack.clone())
            .or_default()
            .push(idx);
    }
    session
}

/// Walk every file's `use` declarations and resolve them against
/// the session's pack table. Each imported (pack, name) lands in
/// [`Session::cross_uses`] keyed by the use-site span. Unresolved
/// imports surface as error diagnostics.
pub fn resolve_cross_pack_uses(session: &mut Session) {
    // Snapshot the by-name lookup tables we need so we can mutate
    // session.cross_uses + diagnostics without a borrow conflict.
    let pack_index: BTreeMap<String, Vec<(usize, &Resolved)>> = session
        .packs
        .iter()
        .map(|(pack, indices)| {
            let entries: Vec<(usize, &Resolved)> = indices
                .iter()
                .map(|i| (*i, &session.files[*i].resolved))
                .collect();
            (pack.clone(), entries)
        })
        .collect();
    let mut imports: Vec<(Span, Result<ImportTarget, String>)> = Vec::new();
    for file in &session.files {
        for use_decl in &file.ast.uses {
            collect_imports(&pack_index, use_decl, &mut imports);
        }
    }
    for (span, result) in imports {
        match result {
            Ok(target) => {
                session.cross_uses.insert(span, target);
            }
            Err(message) => {
                session.diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    message,
                    span,
                });
            }
        }
    }
}

fn collect_imports(
    packs: &BTreeMap<String, Vec<(usize, &Resolved)>>,
    use_decl: &UseDecl,
    out: &mut Vec<(Span, Result<ImportTarget, String>)>,
) {
    match &use_decl.group {
        // Grouped: `use a.b.{X, Y};` — pack is path verbatim, items
        // are the group entries.
        Some(names) => {
            let pack = use_decl
                .path
                .segments
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>()
                .join(".");
            for ident in names {
                let result = lookup(packs, &pack, &ident.name);
                out.push((ident.span, result));
            }
        }
        // Single: `use a.b.C;` — pack is everything except the last
        // segment; item is the last segment.
        None => {
            let segs = &use_decl.path.segments;
            if segs.len() < 2 {
                out.push((
                    use_decl.path.span,
                    Err(format!(
                        "single-item `use` needs at least pack.item shape, got `{}`",
                        segs.iter()
                            .map(|i| i.name.as_str())
                            .collect::<Vec<_>>()
                            .join(".")
                    )),
                ));
                return;
            }
            let item = &segs[segs.len() - 1];
            let pack = segs[..segs.len() - 1]
                .iter()
                .map(|i| i.name.as_str())
                .collect::<Vec<_>>()
                .join(".");
            let result = lookup(packs, &pack, &item.name);
            out.push((item.span, result));
        }
    }
}

fn lookup(
    packs: &BTreeMap<String, Vec<(usize, &Resolved)>>,
    pack: &str,
    item: &str,
) -> Result<ImportTarget, String> {
    let entries = packs
        .get(pack)
        .ok_or_else(|| format!("no pack named `{pack}` in this project"))?;
    for (file_idx, resolved) in entries {
        if let Some(sym_id) = resolved.top_level.get(item).copied() {
            // D-008: only `public` items cross pack boundaries.
            // The pack-default form exists but is not importable.
            if resolved.symbol(sym_id).visibility != Visibility::Public {
                return Err(format!(
                    "item `{item}` in pack `{pack}` is pack-scoped; mark it `public` to import across packs"
                ));
            }
            return Ok(ImportTarget {
                file_idx: *file_idx,
                symbol: sym_id,
            });
        }
    }
    Err(format!(
        "pack `{pack}` does not export an item named `{item}`"
    ))
}

/// Build the pack-level DAG from every file's `use` declarations
/// and surface a diagnostic for each cycle. A cycle is reported
/// once, with the participating packs joined by `→` for context.
pub fn check_pack_acyclicity(session: &mut Session) {
    use std::collections::BTreeSet;
    // Adjacency: pack → set of packs it depends on.
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut span_for_pair: HashMap<(String, String), Span> = HashMap::new();
    for file in &session.files {
        for use_decl in &file.ast.uses {
            // Pack name is everything except the last segment for a
            // single import; the full path for a grouped import.
            let target_pack = match &use_decl.group {
                Some(_) => use_decl
                    .path
                    .segments
                    .iter()
                    .map(|i| i.name.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
                None => {
                    let segs = &use_decl.path.segments;
                    if segs.len() < 2 {
                        continue;
                    }
                    segs[..segs.len() - 1]
                        .iter()
                        .map(|i| i.name.as_str())
                        .collect::<Vec<_>>()
                        .join(".")
                }
            };
            if target_pack == file.pack {
                continue; // self-import is a no-op, never a cycle.
            }
            graph
                .entry(file.pack.clone())
                .or_default()
                .insert(target_pack.clone());
            span_for_pair
                .entry((file.pack.clone(), target_pack))
                .or_insert(use_decl.path.span);
        }
    }
    // Iterative DFS with a colour map. White = unvisited, Gray =
    // on the current stack, Black = finished. A back-edge to a Gray
    // node is a cycle.
    #[derive(Copy, Clone, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }
    let mut color: BTreeMap<String, Color> = graph
        .keys()
        .chain(graph.values().flat_map(|s| s.iter()))
        .map(|p| (p.clone(), Color::White))
        .collect();
    let mut reported: BTreeSet<Vec<String>> = BTreeSet::new();
    fn dfs(
        node: &str,
        graph: &BTreeMap<String, BTreeSet<String>>,
        color: &mut BTreeMap<String, Color>,
        stack: &mut Vec<String>,
        span_for_pair: &HashMap<(String, String), Span>,
        reported: &mut BTreeSet<Vec<String>>,
        diags: &mut Vec<Diagnostic>,
    ) {
        color.insert(node.to_string(), Color::Gray);
        stack.push(node.to_string());
        if let Some(neighbours) = graph.get(node) {
            for next in neighbours {
                match color.get(next).copied().unwrap_or(Color::White) {
                    Color::White => {
                        dfs(next, graph, color, stack, span_for_pair, reported, diags);
                    }
                    Color::Gray => {
                        // Found a cycle: stack[start..] + next.
                        if let Some(start) = stack.iter().position(|p| p == next) {
                            let mut cycle: Vec<String> = stack[start..].to_vec();
                            cycle.push(next.clone());
                            // Canonicalise: rotate so smallest pack
                            // is first, so we don't double-report
                            // the same cycle from different starts.
                            let mut canonical = cycle.clone();
                            canonical.pop(); // drop trailing duplicate
                            let min_pos = canonical
                                .iter()
                                .enumerate()
                                .min_by_key(|(_, p)| p.as_str())
                                .map(|(i, _)| i);
                            if let Some(i) = min_pos {
                                canonical.rotate_left(i);
                            }
                            if reported.insert(canonical.clone()) {
                                let span = span_for_pair
                                    .get(&(node.to_string(), next.clone()))
                                    .copied()
                                    .unwrap_or(Span::new(FileId(0), 0, 0));
                                diags.push(Diagnostic {
                                    severity: Severity::Error,
                                    message: format!("pack import cycle: {}", cycle.join(" → ")),
                                    span,
                                });
                            }
                        }
                    }
                    Color::Black => {}
                }
            }
        }
        color.insert(node.to_string(), Color::Black);
        stack.pop();
    }
    let nodes: Vec<String> = graph.keys().cloned().collect();
    let mut stack: Vec<String> = Vec::new();
    for start in nodes {
        if color.get(&start).copied() == Some(Color::White) {
            dfs(
                &start,
                &graph,
                &mut color,
                &mut stack,
                &span_for_pair,
                &mut reported,
                &mut session.diagnostics,
            );
        }
    }
}

fn diag_error(message: String) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        message,
        span: Span::new(FileId(0), 0, 0),
    }
}
