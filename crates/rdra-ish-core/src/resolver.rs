use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::{LocatedSpan, SourceId};
use crate::model::SemanticModel;
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use rdra_ish_syntax::{ast::*, format_parse_error, parse_with_deprecations, Parsed};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// A fully-resolved multi-file program.
#[derive(Debug)]
pub struct ResolvedProgram {
    /// SourceId → (canonical path, source text, AST)
    pub sources: Vec<(PathBuf, String, Ast)>,
    /// Import dependency graph: edge from_id → to_id means from_id imports to_id.
    pub import_graph: DiGraph<SourceId, ()>,
    /// SourceId → petgraph NodeIndex (parallel to `sources`).
    pub node_indices: Vec<NodeIndex>,
}

/// Load all files reachable from `entry_paths` through `import` declarations,
/// build the dependency graph, detect cycles, and return the resolved program.
pub fn resolve(
    entry_paths: &[PathBuf],
    include_paths: &[PathBuf],
) -> (ResolvedProgram, Vec<Diagnostic>) {
    resolve_overlaid(entry_paths, include_paths, None)
}

/// Like [`resolve`], but uses in-memory source text for open editor buffers.
///
/// Keys in `overlays` should be canonical paths when possible; non-canonical
/// paths are also checked as a fallback.
pub fn resolve_overlaid(
    entry_paths: &[PathBuf],
    include_paths: &[PathBuf],
    overlays: Option<&HashMap<PathBuf, String>>,
) -> (ResolvedProgram, Vec<Diagnostic>) {
    let mut diags: Vec<Diagnostic> = vec![];
    let mut sources: Vec<(PathBuf, String, Ast)> = vec![];
    let mut path_to_id: HashMap<PathBuf, SourceId> = HashMap::new();
    let mut graph: DiGraph<SourceId, ()> = DiGraph::new();
    let mut node_indices: Vec<NodeIndex> = vec![];

    // BFS: collect all reachable files.
    let mut queue: VecDeque<PathBuf> = entry_paths.iter().cloned().collect();
    let mut queued: HashSet<PathBuf> = HashSet::new();
    for p in entry_paths {
        if let Ok(c) = dunce::canonicalize(p) {
            queued.insert(c);
        }
    }

    while let Some(path) = queue.pop_front() {
        let canon = dunce::canonicalize(&path).unwrap_or_else(|_| path.clone());

        if path_to_id.contains_key(&canon) {
            continue;
        }

        let src = match read_source(&canon, overlays) {
            Ok(text) => text,
            Err(e) => {
                diags.push(Diagnostic::error(RdraError::IoError {
                    path: canon.display().to_string(),
                    msg: e.to_string(),
                }));
                continue;
            }
        };

        let id: SourceId = sources.len();

        let ast = take_parsed(parse_with_deprecations(&src), id, &canon, &mut diags);
        let ni = graph.add_node(id);
        path_to_id.insert(canon.clone(), id);
        node_indices.push(ni);

        // Collect imports before moving ast into sources.
        let imports: Vec<ImportDecl> = ast
            .items
            .iter()
            .filter_map(|item| {
                if let Item::Import(imp) = item {
                    Some(imp.clone())
                } else {
                    None
                }
            })
            .collect();

        sources.push((canon.clone(), src, ast));

        // Enqueue unvisited dependencies. An import with no file at its path may name a
        // module a loaded file declares; `build_import_scopes` reports it when none does.
        for imp in &imports {
            if let Some(dep_path) = resolve_import_path(&imp.path, include_paths) {
                let dep_canon = dunce::canonicalize(&dep_path).unwrap_or(dep_path.clone());
                if !queued.contains(&dep_canon) {
                    queued.insert(dep_canon);
                    queue.push_back(dep_path);
                }
            }
        }
    }

    let declared = declared_modules(&sources);

    // Second pass: add edges now that all SourceIds are assigned.
    let mut edges: Vec<ImportEdge> = Vec::new();
    for (from_id, (_, _, ast)) in sources.iter().enumerate() {
        for item in &ast.items {
            let Item::Import(imp) = item else {
                continue;
            };
            let dotted = &imp.path;
            let by_module = declared.get(&dotted.0.join(".")).copied();
            let by_path = || {
                let dep_path = resolve_import_path(dotted, include_paths)?;
                let dep_canon = dunce::canonicalize(&dep_path).ok()?;
                path_to_id.get(&dep_canon).copied()
            };
            if let Some(to_id) = by_module.or_else(by_path) {
                graph.add_edge(node_indices[from_id], node_indices[to_id], ());
                edges.push(ImportEdge {
                    from: from_id,
                    to: to_id,
                    module: dotted.0.join("."),
                    span: imp.span.clone(),
                });
            }
        }
    }

    // Cycle detection via Tarjan's SCC.
    let sccs = tarjan_scc(&graph);
    for scc in &sccs {
        if scc.len() > 1 {
            let files: Vec<String> = scc
                .iter()
                .map(|ni| sources[graph[*ni]].0.display().to_string())
                .collect();
            let members: Vec<SourceId> = scc.iter().map(|ni| graph[*ni]).collect();
            let closing = closing_import(&edges, &members);
            diags.push(Diagnostic::warning_at(
                RdraError::CircularImport {
                    files,
                    closing: closing.module.clone(),
                    closing_file: sources[closing.from].0.display().to_string(),
                },
                LocatedSpan::new(closing.from, closing.span.clone()),
            ));
        }
    }

    (
        ResolvedProgram {
            sources,
            import_graph: graph,
            node_indices,
        },
        diags,
    )
}

/// One `import` line that names a loaded file.
struct ImportEdge {
    from: SourceId,
    to: SourceId,
    module: String,
    span: Span,
}

/// The import that closes a cycle among `members`: walking their imports in load order from
/// the first-loaded member, the first one back to a file still on the walk.
fn closing_import<'e>(edges: &'e [ImportEdge], members: &[SourceId]) -> &'e ImportEdge {
    fn walk<'e>(
        at: SourceId,
        edges: &'e [ImportEdge],
        members: &[SourceId],
        stack: &mut Vec<SourceId>,
        done: &mut HashSet<SourceId>,
    ) -> Option<&'e ImportEdge> {
        stack.push(at);
        for edge in edges
            .iter()
            .filter(|e| e.from == at && members.contains(&e.to))
        {
            if stack.contains(&edge.to) {
                return Some(edge);
            }
            if done.contains(&edge.to) {
                continue;
            }
            if let Some(found) = walk(edge.to, edges, members, stack, done) {
                return Some(found);
            }
        }
        stack.pop();
        done.insert(at);
        None
    }
    let start = *members.iter().min().expect("a cycle has members");
    walk(start, edges, members, &mut Vec::new(), &mut HashSet::new())
        .expect("a strongly connected component of two or more files has a cycle")
}

/// Each `module` path to the first loaded source that declares it. A second declarer is
/// `DuplicateModule`, reported by `build_import_scopes`.
fn declared_modules(sources: &[(PathBuf, String, Ast)]) -> HashMap<String, SourceId> {
    let mut declared = HashMap::new();
    for (id, (_, _, ast)) in sources.iter().enumerate() {
        for item in &ast.items {
            if let Item::Module(path, _) = item {
                declared.entry(path.0.join(".")).or_insert(id);
            }
        }
    }
    declared
}

/// The modules loaded files declare that look like `module`: the same last segment, one last
/// segment of four or more characters inside the other (`registration` and
/// `buc_registration`), or at most two edits apart. Each is `<module> (<file>)`, the file relative to the include path nearest it.
fn similar_modules(
    program: &ResolvedProgram,
    include_paths: &[PathBuf],
    module: &str,
) -> Vec<String> {
    let bases: Vec<PathBuf> = include_paths
        .iter()
        .map(|base| dunce::canonicalize(base).unwrap_or_else(|_| base.clone()))
        .collect();
    let last = |m: &str| m.rsplit('.').next().unwrap_or(m).to_lowercase();
    let wanted = last(module);
    let mut found: Vec<String> = Vec::new();
    for (path, _, ast) in &program.sources {
        for item in &ast.items {
            let Item::Module(declared, _) = item else {
                continue;
            };
            let declared = declared.0.join(".");
            let have = last(&declared);
            let inside = |long: &str, short: &str| short.len() >= 4 && long.contains(short);
            let like = have == wanted
                || inside(&have, &wanted)
                || inside(&wanted, &have)
                || edit_distance(&declared, module) <= 2;
            if !like || declared == module {
                continue;
            }
            let shown = bases
                .iter()
                .filter_map(|base| path.strip_prefix(base).ok())
                .min_by_key(|rel| rel.components().count())
                .unwrap_or(path);
            // `/` on every platform, so a hint reads the same as the import paths it sits beside.
            let shown: Vec<String> = shown
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            found.push(format!("{declared} ({})", shown.join("/")));
        }
    }
    found.sort();
    found.dedup();
    found
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diag = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let next = (row[j + 1] + 1)
                .min(row[j] + 1)
                .min(diag + usize::from(ca != *cb));
            diag = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()]
}

/// Convert a dotted module path to a file path and search `include_paths`.
///
/// `shared.actors` → `shared/actors.rdra`
fn resolve_import_path(dotted: &DottedName, include_paths: &[PathBuf]) -> Option<PathBuf> {
    let mut rel = PathBuf::new();
    for segment in &dotted.0 {
        rel.push(segment);
    }
    let rel = rel.with_extension("rdra");

    for base in include_paths {
        let candidate = base.join(&rel);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn read_source(
    path: &std::path::Path,
    overlays: Option<&HashMap<PathBuf, String>>,
) -> std::io::Result<String> {
    if let Some(overlays) = overlays {
        if let Some(text) = overlays.get(path) {
            return Ok(text.clone());
        }
        if let Ok(canon) = dunce::canonicalize(path) {
            if let Some(text) = overlays.get(&canon) {
                return Ok(text.clone());
            }
        }
        for (key, text) in overlays {
            let same = dunce::canonicalize(key)
                .ok()
                .zip(dunce::canonicalize(path).ok())
                .is_some_and(|(a, b)| a == b)
                || key == path;
            if same {
                return Ok(text.clone());
            }
        }
    }
    std::fs::read_to_string(path)
}

/// The AST of one parsed file, reporting its parse errors as errors and its deprecated forms
/// (already rewritten in the AST) as `DeprecatedSyntax` warnings.
fn take_parsed(
    parsed: Parsed,
    id: SourceId,
    path: &std::path::Path,
    diags: &mut Vec<Diagnostic>,
) -> Ast {
    let path = path.display().to_string();
    for err in &parsed.errors {
        diags.push(Diagnostic::error_at(
            RdraError::SyntaxError {
                path: path.clone(),
                msg: format_parse_error(err),
            },
            LocatedSpan::new(id, err.span()),
        ));
    }
    for deprecation in parsed.deprecations {
        diags.push(Diagnostic::warning_at(
            RdraError::DeprecatedSyntax {
                path: path.clone(),
                msg: deprecation.message,
            },
            LocatedSpan::new(id, deprecation.span),
        ));
    }
    parsed.ast
}

/// Build a semantic model from located items collected across source files.
pub fn build_merged_model(
    program: &ResolvedProgram,
    include_paths: &[PathBuf],
) -> (SemanticModel, Vec<Diagnostic>) {
    let mut all_diags: Vec<Diagnostic> = vec![];
    let mut located_items: Vec<(SourceId, Item)> = Vec::new();
    // (kind, id, module_path) — same id is allowed across different modules.
    let mut seen_ids: HashSet<(String, String, Option<String>)> = HashSet::new();

    let (import_scopes, scope_diags) = crate::import_scope::build_import_scopes(&program.sources);
    for d in scope_diags {
        match d.kind {
            crate::import_scope::ImportScopeDiagKind::UnknownSelect { name, module } => {
                all_diags.push(Diagnostic::error_at(
                    RdraError::UnknownImportName { name, module },
                    LocatedSpan::new(d.source_id, d.span),
                ));
            }
            crate::import_scope::ImportScopeDiagKind::UnknownModule { module } => {
                let similar = similar_modules(program, include_paths, &module);
                let similar = match similar.is_empty() {
                    true => String::new(),
                    false => format!("; similar modules: {}", similar.join(", ")),
                };
                all_diags.push(Diagnostic::error_at(
                    RdraError::UnknownImportModule { module, similar },
                    LocatedSpan::new(d.source_id, d.span),
                ));
            }
            crate::import_scope::ImportScopeDiagKind::DuplicateModule { module } => {
                all_diags.push(Diagnostic::error_at(
                    RdraError::DuplicateModule { module },
                    LocatedSpan::new(d.source_id, d.span),
                ));
            }
            crate::import_scope::ImportScopeDiagKind::DuplicateAlias { alias } => {
                all_diags.push(Diagnostic::error_at(
                    RdraError::DuplicateAlias { alias },
                    LocatedSpan::new(d.source_id, d.span),
                ));
            }
            crate::import_scope::ImportScopeDiagKind::DuplicateVisible { name, .. } => {
                all_diags.push(Diagnostic::error_at(
                    RdraError::DuplicateDefinition { id: name },
                    LocatedSpan::new(d.source_id, d.span),
                ));
            }
        }
    }

    for (source_id, (_path, _src, ast)) in program.sources.iter().enumerate() {
        let module = import_scopes
            .module_for_source(source_id)
            .map(str::to_string);
        for item in &ast.items {
            match item {
                Item::Instance(inst) => {
                    let key = (
                        inst.kind.name().to_string(),
                        inst.id.clone(),
                        module.clone(),
                    );
                    if seen_ids.contains(&key) {
                        all_diags.push(Diagnostic::error_at(
                            RdraError::DuplicateDefinition {
                                id: inst.id.clone(),
                            },
                            LocatedSpan::new(source_id, inst.span.clone()),
                        ));
                    } else {
                        seen_ids.insert(key);
                        located_items.push((source_id, item.clone()));
                    }
                }
                Item::Predicate(_) | Item::Property(_) => {
                    located_items.push((source_id, item.clone()));
                }
                Item::Module(_, _) | Item::Import(_) => {}
            }
        }
    }

    let (model, mut model_diags) =
        crate::analysis::build_model_items_with_scopes(&located_items, import_scopes);
    name_missing_imports(&model, &mut model_diags);
    all_diags.extend(model_diags);

    (model, all_diags)
}

/// Gives the first `UndefinedSymbol` of a file per module the hint `add import <module>`,
/// when one loaded module the file does not import declares the name.
fn name_missing_imports(model: &SemanticModel, diags: &mut [Diagnostic]) {
    let mut hinted: HashSet<(SourceId, String)> = HashSet::new();
    for d in diags {
        let (RdraError::UndefinedSymbol { id, import }, Some(at)) = (&mut d.error, &d.location)
        else {
            continue;
        };
        let name = id.rsplit("::").next().unwrap_or(id);
        let Some(module) = model.import_scopes.unimported_declarer(at.source_id, name) else {
            continue;
        };
        if hinted.insert((at.source_id, module.to_string())) {
            *import = Some(module.to_string());
        }
    }
}

/// Return all `NodeRef`s reachable from a BUC node via RDRA relations (forward BFS only).
/// ER entity-to-entity relations (Relate*) are intentionally excluded so that shared
/// entities don't pull in unrelated BUCs through the ER graph.
pub fn reachable_from_buc(model: &SemanticModel, buc_id: &str) -> HashSet<crate::model::NodeRef> {
    use crate::model::{NodeRef, RelKind};
    let mut visited: HashSet<NodeRef> = HashSet::new();

    let buc_key = model.symbols.get(buc_id).and_then(|nr| {
        if let NodeRef::Buc(k) = nr {
            Some(*k)
        } else {
            None
        }
    });

    let Some(buc_key) = buc_key else {
        return visited;
    };

    let start = NodeRef::Buc(buc_key);
    let mut queue = vec![start.clone()];
    visited.insert(start);

    // 前向き BFS: ER entity-entity 関係を除くすべての forward 関係を辿る
    while let Some(current) = queue.pop() {
        for rel in &model.relations {
            if matches!(
                rel.kind,
                RelKind::RelateOneToOne
                    | RelKind::RelateOneToMany
                    | RelKind::RelateManyToOne
                    | RelKind::RelateManyToMany
            ) {
                continue;
            }
            if rel.from == current && !visited.contains(&rel.to) {
                visited.insert(rel.to.clone());
                queue.push(rel.to.clone());
            }
        }
    }

    // 閉包内のユースケースを起動するイベント（外部システムからの inbound）も閉包に入れる。
    // 前向き BFS は inbound イベントに到達しない。
    let inbound: Vec<NodeRef> = model
        .relations
        .iter()
        .filter(|rel| {
            rel.kind == RelKind::Triggers
                && matches!(rel.from, NodeRef::Event(_))
                && visited.contains(&rel.to)
        })
        .map(|rel| rel.from.clone())
        .collect();
    visited.extend(inbound);

    // 第2パス: 起点 BUC に直接 performs している Actor と、閉包内へ入る Timer（triggers）/
    // 外部システム（raises）/ 条件（applies_to）を追加する（推移的探索はしない — 他 BUC への連鎖を防ぐ）。
    // 条件はそのバリエーションも連れてくる（バリエーションは関係では閉包に入らない）。
    let buc_ref = NodeRef::Buc(buc_key);
    let mut sources = Vec::new();
    for rel in &model.relations {
        let enters = match (&rel.kind, &rel.from) {
            (RelKind::Performs, _) => rel.to == buc_ref,
            (RelKind::Triggers, NodeRef::Timer(_))
            | (RelKind::Raises, NodeRef::ExtSystem(_))
            | (RelKind::AppliesTo | RelKind::Guards, NodeRef::Condition(_)) => {
                visited.contains(&rel.to)
            }
            _ => false,
        };
        if enters {
            if let NodeRef::Condition(condition) = &rel.from {
                for edge in crate::analysis::condition_edges(model, *condition) {
                    if let crate::analysis::ConditionEdge::Variation(variation) = edge {
                        sources.push(NodeRef::Variation(variation));
                    }
                }
            }
            sources.push(rel.from.clone());
        }
    }
    visited.extend(sources);

    visited
}

/// Return the union of `reachable_from_buc` for each BUC id in `buc_ids`.
/// When `buc_ids` is empty the result is an empty set (callers should treat
/// that as "show everything" / `Scope::Whole`).
pub fn reachable_from_bucs(
    model: &SemanticModel,
    buc_ids: &[String],
) -> HashSet<crate::model::NodeRef> {
    let mut union = HashSet::new();
    for id in buc_ids {
        union.extend(reachable_from_buc(model, id));
    }
    union
}

use std::collections::VecDeque;

#[cfg(test)]
mod tests;
