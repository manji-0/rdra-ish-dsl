//! Per-file import visibility for module / alias / selective imports.

use crate::location::SourceId;
use rdra_ish_syntax::ast::{ImportDecl, ImportKind, Item, Span};
use std::collections::HashMap;
use std::path::PathBuf;

/// Binding of a flat local name to a canonical id + declaring module (+ kind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatBinding {
    pub canonical_id: String,
    pub module: Option<String>,
    /// Declared kind (`usecase`, `event`, …). `None` only for unrestricted open-world.
    pub kind: Option<String>,
}

/// Import visibility for one source file.
#[derive(Debug, Clone, Default)]
pub struct FileScope {
    /// Flat local name → zero or more kind-qualified bindings.
    /// Same id with different kinds (e.g. `event Capture` + `usecase Capture`) may coexist.
    pub flat: HashMap<String, Vec<FlatBinding>>,
    /// `import M as alias` → module path string (`shared.actors`).
    pub namespaces: HashMap<String, String>,
    /// When true, skip visibility checks (single-file / legacy open scope).
    pub unrestricted: bool,
}

/// id → kinds declared in a source file.
type SourceExports = HashMap<String, Vec<String>>;

/// Workspace-wide import scopes keyed by source id.
#[derive(Debug, Clone, Default)]
pub struct ImportScopes {
    by_source: HashMap<SourceId, FileScope>,
    /// module path → source that declared `module <path>`
    module_sources: HashMap<String, SourceId>,
    /// source → id → kinds declared in that file
    source_exports: HashMap<SourceId, SourceExports>,
    /// source → module path (if declared)
    source_modules: HashMap<SourceId, String>,
}

impl ImportScopes {
    /// Open scope used by single-file `build_model`.
    pub fn unrestricted(source_id: SourceId) -> Self {
        let mut scopes = Self::default();
        scopes.by_source.insert(
            source_id,
            FileScope {
                unrestricted: true,
                ..FileScope::default()
            },
        );
        scopes
    }

    pub fn scope_for(&self, source_id: SourceId) -> FileScope {
        self.by_source
            .get(&source_id)
            .cloned()
            .unwrap_or(FileScope {
                unrestricted: true,
                ..FileScope::default()
            })
    }

    pub fn module_for_source(&self, source_id: SourceId) -> Option<&str> {
        self.source_modules.get(&source_id).map(String::as_str)
    }

    /// Resolve all flat bindings for a local name under the file scope.
    pub fn resolve_flat_bindings(&self, source_id: SourceId, name: &str) -> Vec<FlatBinding> {
        let scope = self.scope_for(source_id);
        if scope.unrestricted {
            return vec![FlatBinding {
                canonical_id: name.to_string(),
                module: None,
                kind: None,
            }];
        }
        scope.flat.get(name).cloned().unwrap_or_default()
    }

    /// Resolve a flat name when exactly one binding is visible.
    /// Prefer [`Self::resolve_flat_bindings`] when kind-qualified refs are involved.
    pub fn resolve_flat(&self, source_id: SourceId, name: &str) -> Option<FlatBinding> {
        let bindings = self.resolve_flat_bindings(source_id, name);
        match bindings.as_slice() {
            [only] => Some(only.clone()),
            _ => None,
        }
    }

    /// Resolve `alias.Name` under a namespaced import (all kinds exported under `Name`).
    pub fn resolve_namespaced(
        &self,
        source_id: SourceId,
        alias: &str,
        name: &str,
    ) -> Result<Vec<FlatBinding>, NamespacedResolveError> {
        let scope = self.scope_for(source_id);
        if scope.unrestricted {
            return Ok(vec![FlatBinding {
                canonical_id: name.to_string(),
                module: None,
                kind: None,
            }]);
        }
        let Some(module_path) = scope.namespaces.get(alias) else {
            return Err(NamespacedResolveError::UnknownAlias {
                alias: alias.to_string(),
            });
        };
        let Some(&mod_source) = self.module_sources.get(module_path) else {
            return Err(NamespacedResolveError::UnknownModule {
                module: module_path.clone(),
            });
        };
        let Some(exports) = self.source_exports.get(&mod_source) else {
            return Err(NamespacedResolveError::NotExported {
                module: module_path.clone(),
                name: name.to_string(),
            });
        };
        let Some(kinds) = exports.get(name) else {
            return Err(NamespacedResolveError::NotExported {
                module: module_path.clone(),
                name: name.to_string(),
            });
        };
        Ok(kinds
            .iter()
            .map(|kind| FlatBinding {
                canonical_id: name.to_string(),
                module: Some(module_path.clone()),
                kind: Some(kind.clone()),
            })
            .collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamespacedResolveError {
    UnknownAlias { alias: String },
    UnknownModule { module: String },
    NotExported { module: String, name: String },
}

/// Diagnostics produced while building scopes (unknown selective imports, etc.).
#[derive(Debug, Clone)]
pub struct ImportScopeDiagnostic {
    pub source_id: SourceId,
    pub span: Span,
    pub message_id: String,
    pub kind: ImportScopeDiagKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportScopeDiagKind {
    UnknownSelect {
        name: String,
        module: String,
    },
    UnknownModule {
        module: String,
    },
    DuplicateModule {
        module: String,
    },
    DuplicateAlias {
        alias: String,
    },
    /// Local def or prior import already binds this flat name for the same kind.
    DuplicateVisible {
        name: String,
        kind: String,
        module: String,
    },
}

fn insert_flat(
    scope: &mut FileScope,
    diags: &mut Vec<ImportScopeDiagnostic>,
    source_id: SourceId,
    span: Span,
    local: String,
    binding: FlatBinding,
) {
    let entries = scope.flat.entry(local.clone()).or_default();
    if let Some(existing) = entries
        .iter()
        .find(|e| e.kind == binding.kind && e.kind.is_some())
    {
        if existing.module != binding.module || existing.canonical_id != binding.canonical_id {
            diags.push(ImportScopeDiagnostic {
                source_id,
                span: span.clone(),
                message_id: local.clone(),
                kind: ImportScopeDiagKind::DuplicateVisible {
                    name: local,
                    kind: binding.kind.clone().unwrap_or_default(),
                    module: binding.module.clone().unwrap_or_else(|| "<import>".into()),
                },
            });
        }
        return;
    }
    // Identical re-insert (same kind/module/id) is a no-op.
    if entries.iter().any(|e| e == &binding) {
        return;
    }
    entries.push(binding);
}

fn span_for_local_id(ast: &rdra_ish_syntax::ast::Ast, id: &str) -> Span {
    ast.items
        .iter()
        .find_map(|item| match item {
            Item::Instance(inst) if inst.id == id => Some(inst.span.clone()),
            Item::Property(prop) if prop.id == id => Some(prop.span.clone()),
            _ => None,
        })
        .unwrap_or(0..0)
}

fn push_export(exports: &mut SourceExports, id: String, kind: String) {
    let kinds = exports.entry(id).or_default();
    if !kinds.contains(&kind) {
        kinds.push(kind);
    }
}

/// Build import scopes from a resolved program's ASTs.
pub fn build_import_scopes(
    sources: &[(PathBuf, String, rdra_ish_syntax::ast::Ast)],
) -> (ImportScopes, Vec<ImportScopeDiagnostic>) {
    let mut scopes = ImportScopes::default();
    let mut diags = Vec::new();

    for (source_id, (_path, _src, ast)) in sources.iter().enumerate() {
        let mut exports = SourceExports::new();
        let mut module_path: Option<(String, Span)> = None;
        for item in &ast.items {
            match item {
                Item::Module(path, span) => {
                    module_path = Some((path.0.join("."), span.clone()));
                }
                Item::Instance(inst) => {
                    push_export(&mut exports, inst.id.clone(), inst.kind.name().to_string());
                }
                _ => {}
            }
        }
        if let Some((mp, span)) = &module_path {
            if let Some(&prev) = scopes.module_sources.get(mp) {
                if prev != source_id {
                    diags.push(ImportScopeDiagnostic {
                        source_id,
                        span: span.clone(),
                        message_id: mp.clone(),
                        kind: ImportScopeDiagKind::DuplicateModule { module: mp.clone() },
                    });
                }
            } else {
                scopes.module_sources.insert(mp.clone(), source_id);
            }
            scopes.source_modules.insert(source_id, mp.clone());
        }
        scopes.source_exports.insert(source_id, exports);
    }

    for (source_id, (_path, _src, ast)) in sources.iter().enumerate() {
        let imports: Vec<&ImportDecl> = ast
            .items
            .iter()
            .filter_map(|item| {
                if let Item::Import(imp) = item {
                    Some(imp)
                } else {
                    None
                }
            })
            .collect();

        let mut scope = FileScope::default();
        let local_module = scopes.source_modules.get(&source_id).cloned();
        let local_exports = scopes
            .source_exports
            .get(&source_id)
            .cloned()
            .unwrap_or_default();

        // Per-file latch: only files that import become closed scope.
        // Sibling files without imports keep legacy open-world resolution.
        if imports.is_empty() {
            for (id, kinds) in &local_exports {
                for kind in kinds {
                    scope.flat.entry(id.clone()).or_default().push(FlatBinding {
                        canonical_id: id.clone(),
                        module: local_module.clone(),
                        kind: Some(kind.clone()),
                    });
                }
            }
            scope.unrestricted = true;
            scopes.by_source.insert(source_id, scope);
            continue;
        }

        // Imports first, then locals — redefinition of an imported name errors on the local decl.
        for imp in imports {
            let module_path = imp.path.0.join(".");
            let Some(&mod_source) = scopes.module_sources.get(&module_path) else {
                diags.push(ImportScopeDiagnostic {
                    source_id,
                    span: imp.span.clone(),
                    message_id: module_path.clone(),
                    kind: ImportScopeDiagKind::UnknownModule {
                        module: module_path,
                    },
                });
                continue;
            };
            let exports = scopes
                .source_exports
                .get(&mod_source)
                .cloned()
                .unwrap_or_default();

            match &imp.kind {
                ImportKind::All => {
                    for (id, kinds) in &exports {
                        for kind in kinds {
                            insert_flat(
                                &mut scope,
                                &mut diags,
                                source_id,
                                imp.span.clone(),
                                id.clone(),
                                FlatBinding {
                                    canonical_id: id.clone(),
                                    module: Some(module_path.clone()),
                                    kind: Some(kind.clone()),
                                },
                            );
                        }
                    }
                }
                ImportKind::Alias(alias) => {
                    if let Some(existing) = scope.namespaces.get(alias) {
                        if existing != &module_path {
                            diags.push(ImportScopeDiagnostic {
                                source_id,
                                span: imp.span.clone(),
                                message_id: alias.clone(),
                                kind: ImportScopeDiagKind::DuplicateAlias {
                                    alias: alias.clone(),
                                },
                            });
                        }
                    }
                    scope.namespaces.insert(alias.clone(), module_path.clone());
                }
                ImportKind::Select(items) => {
                    for item in items {
                        if let Some(kinds) = exports.get(&item.name) {
                            let local = item.alias.clone().unwrap_or_else(|| item.name.clone());
                            for kind in kinds {
                                insert_flat(
                                    &mut scope,
                                    &mut diags,
                                    source_id,
                                    item.span.clone(),
                                    local.clone(),
                                    FlatBinding {
                                        canonical_id: item.name.clone(),
                                        module: Some(module_path.clone()),
                                        kind: Some(kind.clone()),
                                    },
                                );
                            }
                        } else {
                            diags.push(ImportScopeDiagnostic {
                                source_id,
                                span: item.span.clone(),
                                message_id: item.name.clone(),
                                kind: ImportScopeDiagKind::UnknownSelect {
                                    name: item.name.clone(),
                                    module: module_path.clone(),
                                },
                            });
                        }
                    }
                }
            }
        }

        for (id, kinds) in &local_exports {
            let span = span_for_local_id(ast, id);
            for kind in kinds {
                insert_flat(
                    &mut scope,
                    &mut diags,
                    source_id,
                    span.clone(),
                    id.clone(),
                    FlatBinding {
                        canonical_id: id.clone(),
                        module: local_module.clone(),
                        kind: Some(kind.clone()),
                    },
                );
            }
        }

        scopes.by_source.insert(source_id, scope);
    }

    (scopes, diags)
}
