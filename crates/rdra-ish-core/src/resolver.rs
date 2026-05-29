use crate::analysis::build_model;
use crate::diagnostics::{Diagnostic, RdraError};
use crate::model::SemanticModel;
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use rdra_ish_syntax::{ast::*, parse};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// SourceId identifies a single source file by its index in `sources`.
pub type SourceId = usize;

/// A fully-resolved multi-file program.
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
    let mut diags: Vec<Diagnostic> = vec![];
    let mut sources: Vec<(PathBuf, String, Ast)> = vec![];
    let mut path_to_id: HashMap<PathBuf, SourceId> = HashMap::new();
    let mut graph: DiGraph<SourceId, ()> = DiGraph::new();
    let mut node_indices: Vec<NodeIndex> = vec![];

    // BFS: collect all reachable files.
    let mut queue: VecDeque<PathBuf> = entry_paths.iter().cloned().collect();
    let mut queued: HashSet<PathBuf> = HashSet::new();
    for p in entry_paths {
        if let Ok(c) = std::fs::canonicalize(p) {
            queued.insert(c);
        }
    }

    while let Some(path) = queue.pop_front() {
        let canon = match std::fs::canonicalize(&path) {
            Ok(p) => p,
            Err(e) => {
                diags.push(Diagnostic::error(RdraError::IoError {
                    path: path.display().to_string(),
                    msg: e.to_string(),
                }));
                continue;
            }
        };

        if path_to_id.contains_key(&canon) {
            continue;
        }

        let src = match std::fs::read_to_string(&canon) {
            Ok(s) => s,
            Err(e) => {
                diags.push(Diagnostic::error(RdraError::IoError {
                    path: canon.display().to_string(),
                    msg: e.to_string(),
                }));
                continue;
            }
        };

        let (ast, _parse_errs) = parse(&src);
        let id: SourceId = sources.len();
        let ni = graph.add_node(id);
        path_to_id.insert(canon.clone(), id);
        node_indices.push(ni);

        // Collect imports before moving ast into sources.
        let imports: Vec<(DottedName, ImportKind)> = ast
            .items
            .iter()
            .filter_map(|item| {
                if let Item::Import(imp) = item {
                    Some((imp.path.clone(), imp.kind.clone()))
                } else {
                    None
                }
            })
            .collect();

        sources.push((canon.clone(), src, ast));

        // Enqueue unvisited dependencies.
        for (dotted, _kind) in imports {
            if let Some(dep_path) = resolve_import_path(&dotted, include_paths) {
                let dep_canon = std::fs::canonicalize(&dep_path).unwrap_or(dep_path.clone());
                if !queued.contains(&dep_canon) {
                    queued.insert(dep_canon);
                    queue.push_back(dep_path);
                }
            } else {
                diags.push(Diagnostic::error(RdraError::IoError {
                    path: dotted.0.join("/") + ".rdra",
                    msg: "module file not found in include paths".to_string(),
                }));
            }
        }
    }

    // Second pass: add edges now that all SourceIds are assigned.
    let edge_info: Vec<(PathBuf, Vec<DottedName>)> = sources
        .iter()
        .map(|(p, _, ast)| {
            let imports = ast
                .items
                .iter()
                .filter_map(|item| {
                    if let Item::Import(imp) = item {
                        Some(imp.path.clone())
                    } else {
                        None
                    }
                })
                .collect();
            (p.clone(), imports)
        })
        .collect();

    for (from_path, imports) in &edge_info {
        let from_id = *path_to_id.get(from_path).unwrap();
        let from_ni = node_indices[from_id];
        for dotted in imports {
            if let Some(dep_path) = resolve_import_path(dotted, include_paths) {
                if let Ok(dep_canon) = std::fs::canonicalize(&dep_path) {
                    if let Some(&to_id) = path_to_id.get(&dep_canon) {
                        let to_ni = node_indices[to_id];
                        graph.add_edge(from_ni, to_ni, ());
                    }
                }
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
            diags.push(Diagnostic::warning(RdraError::CircularImport { files }));
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

/// Merge all ASTs from `program` into a single flat AST and build a `SemanticModel`.
///
/// Duplicate instance IDs across files are reported as errors.
/// `Module` and `Import` items are skipped (already handled during discovery).
pub fn build_merged_model(
    program: &ResolvedProgram,
    _include_paths: &[PathBuf],
) -> (SemanticModel, Vec<Diagnostic>) {
    let mut all_diags: Vec<Diagnostic> = vec![];
    let mut merged_items: Vec<Item> = vec![];
    // Key is (kind_name, id) — same id across different kinds is allowed.
    let mut seen_ids: HashSet<(String, String)> = HashSet::new();

    for (_path, _src, ast) in &program.sources {
        for item in &ast.items {
            match item {
                Item::Instance(inst) => {
                    let key = (inst.kind.name().to_string(), inst.id.clone());
                    if seen_ids.contains(&key) {
                        all_diags.push(Diagnostic::error(RdraError::DuplicateDefinition {
                            id: inst.id.clone(),
                        }));
                    } else {
                        seen_ids.insert(key);
                        merged_items.push(item.clone());
                    }
                }
                Item::Predicate(_) => {
                    merged_items.push(item.clone());
                }
                // Already processed during discovery.
                Item::Module(_, _) | Item::Import(_) => {}
            }
        }
    }

    let merged_ast = Ast {
        items: merged_items,
        source: String::new(),
    };

    let (model, model_diags) = build_model(&merged_ast);
    all_diags.extend(model_diags);

    (model, all_diags)
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

    // 第2パス: 起点 BUC に直接 performs / uses している Actor / ExternalSystem のみ追加
    // （推移的探索はしない — 他 BUC への連鎖を防ぐ）
    let buc_ref = NodeRef::Buc(buc_key);
    for rel in &model.relations {
        if matches!(rel.kind, RelKind::Performs | RelKind::Uses) && rel.to == buc_ref {
            visited.insert(rel.from.clone());
        }
    }

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
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// Write a file, creating parent directories as needed.
    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    /// Create a unique temp directory under the system temp root.
    fn make_temp_dir(prefix: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "rdra_test_{}_{}",
            prefix,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn test_resolve_single_file() {
        let dir = make_temp_dir("single");
        let entry = dir.join("main.rdra");
        write_file(
            &entry,
            r#"
actor Customer "顧客"
usecase Browse "商品を探す"
performs(Customer, Browse)
"#,
        );

        let (program, diags) = resolve(&[entry], std::slice::from_ref(&dir));
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors.is_empty(),
            "unexpected errors: {:?}",
            errors
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(program.sources.len(), 1);
    }

    #[test]
    fn test_resolve_two_files_with_import() {
        let dir = make_temp_dir("two_files");
        let shared_dir = dir.join("shared");
        fs::create_dir_all(&shared_dir).unwrap();

        write_file(
            &shared_dir.join("actors.rdra"),
            r#"
module shared.actors
actor Customer "顧客"
"#,
        );
        write_file(
            &dir.join("main.rdra"),
            r#"
import shared.actors
usecase Browse "商品を探す"
performs(Customer, Browse)
"#,
        );

        let (program, diags) = resolve(&[dir.join("main.rdra")], std::slice::from_ref(&dir));
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors.is_empty(),
            "unexpected errors: {:?}",
            errors
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(program.sources.len(), 2, "expected 2 source files");

        // Verify edge exists in import graph.
        assert_eq!(program.import_graph.edge_count(), 1);

        // Verify model builds correctly.
        let (model, model_diags) = build_merged_model(&program, &[dir]);
        let model_errors: Vec<_> = model_diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            model_errors.is_empty(),
            "model errors: {:?}",
            model_errors
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(model.actors.len(), 1);
        assert_eq!(model.use_cases.len(), 1);
    }

    #[test]
    fn test_circular_import_warning() {
        let dir = make_temp_dir("circular");
        let a = dir.join("a.rdra");
        let b_dir = dir.join("b");
        fs::create_dir_all(&b_dir).unwrap();
        let b = b_dir.join("mod.rdra");

        // a imports b.mod, b.mod imports... but b.mod can't import "a" (dotted name would be "a"),
        // so simulate the simplest possible cycle: two files that each import the other.
        // We use a directory structure: dir/a.rdra imports dir/b/mod.rdra,
        // and dir/b/mod.rdra imports dir/a.rdra (via dotted name "a").
        write_file(
            &a,
            r#"
module a
import b.mod
actor Customer "顧客"
"#,
        );
        write_file(
            &b,
            r#"
module b.mod
import a
actor Staff "スタッフ"
"#,
        );

        let (program, diags) = resolve(&[a], std::slice::from_ref(&dir));
        let warnings: Vec<_> = diags.iter().filter(|d| d.is_warning).collect();
        assert!(
            warnings
                .iter()
                .any(|d| matches!(&d.error, RdraError::CircularImport { .. })),
            "expected CircularImport warning, got: {:?}",
            diags
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );
        // Sources are still collected (2 files).
        assert_eq!(program.sources.len(), 2);
    }

    #[test]
    fn test_duplicate_definition_across_files() {
        let dir = make_temp_dir("dup_def");
        let shared_dir = dir.join("shared");
        fs::create_dir_all(&shared_dir).unwrap();

        write_file(
            &shared_dir.join("actors.rdra"),
            r#"
module shared.actors
actor Customer "顧客"
"#,
        );
        write_file(
            &dir.join("main.rdra"),
            r#"
import shared.actors
actor Customer "重複定義"
"#,
        );

        let (program, resolve_diags) =
            resolve(&[dir.join("main.rdra")], std::slice::from_ref(&dir));
        let (_, model_diags) = build_merged_model(&program, &[dir]);

        let all_diags: Vec<_> = resolve_diags.iter().chain(model_diags.iter()).collect();
        let dup_errors: Vec<_> = all_diags
            .iter()
            .filter(|d| matches!(&d.error, RdraError::DuplicateDefinition { .. }))
            .collect();
        assert!(!dup_errors.is_empty(), "expected DuplicateDefinition error");
    }

    #[test]
    fn test_resolve_fixture_purchase() {
        // Tests the full purchase fixture under tests/fixtures/purchase/.
        let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("tests/fixtures/purchase");

        if !fixture_root.exists() {
            // Skip if fixtures haven't been created yet.
            return;
        }

        let entry = fixture_root.join("buc/buc_purchase.rdra");
        let (program, diags) = resolve(&[entry], std::slice::from_ref(&fixture_root));

        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors.is_empty(),
            "resolve errors: {:?}",
            errors
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(program.sources.len(), 3, "expected 3 source files");

        let (model, model_diags) = build_merged_model(&program, &[fixture_root]);
        let model_errors: Vec<_> = model_diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            model_errors.is_empty(),
            "model errors: {:?}",
            model_errors
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );
        assert!(model.actors.len() >= 2);
        assert!(model.entities.len() >= 4);
        assert!(!model.bucs.is_empty());
    }
}
