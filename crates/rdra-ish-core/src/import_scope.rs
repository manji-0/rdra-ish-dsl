//! Per-file import visibility for module / alias / selective imports.

use crate::location::SourceId;
use rdra_ish_syntax::ast::{ImportDecl, ImportKind, Item, Span};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Binding of a flat local name to a canonical id + declaring module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatBinding {
    pub canonical_id: String,
    pub module: Option<String>,
}

/// Import visibility for one source file.
#[derive(Debug, Clone, Default)]
pub struct FileScope {
    /// Flat local name → canonical declaration (+ module for disambiguation).
    pub flat: HashMap<String, FlatBinding>,
    /// `import M as alias` → module path string (`shared.actors`).
    pub namespaces: HashMap<String, String>,
    /// When true, skip visibility checks (single-file / legacy open scope).
    pub unrestricted: bool,
}

/// Workspace-wide import scopes keyed by source id.
#[derive(Debug, Clone, Default)]
pub struct ImportScopes {
    by_source: HashMap<SourceId, FileScope>,
    /// module path → source that declared `module <path>`
    module_sources: HashMap<String, SourceId>,
    /// source → ids declared in that file
    source_exports: HashMap<SourceId, HashSet<String>>,
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

    /// Resolve a flat name under the file scope.
    pub fn resolve_flat(&self, source_id: SourceId, name: &str) -> Option<FlatBinding> {
        let scope = self.scope_for(source_id);
        if scope.unrestricted {
            // Open world: do not constrain lookup to this file's module.
            return Some(FlatBinding {
                canonical_id: name.to_string(),
                module: None,
            });
        }
        scope.flat.get(name).cloned()
    }

    /// Resolve `alias.Name` under a namespaced import.
    pub fn resolve_namespaced(
        &self,
        source_id: SourceId,
        alias: &str,
        name: &str,
    ) -> Result<FlatBinding, NamespacedResolveError> {
        let scope = self.scope_for(source_id);
        if scope.unrestricted {
            return Ok(FlatBinding {
                canonical_id: name.to_string(),
                module: None,
            });
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
        let exports = self.source_exports.get(&mod_source);
        if exports.is_some_and(|e| e.contains(name)) {
            Ok(FlatBinding {
                canonical_id: name.to_string(),
                module: Some(module_path.clone()),
            })
        } else {
            Err(NamespacedResolveError::NotExported {
                module: module_path.clone(),
                name: name.to_string(),
            })
        }
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
    /// Local def or prior import already binds this flat name.
    DuplicateVisible {
        name: String,
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
    if let Some(existing) = scope.flat.get(&local) {
        if existing.module != binding.module || existing.canonical_id != binding.canonical_id {
            diags.push(ImportScopeDiagnostic {
                source_id,
                span: span.clone(),
                message_id: local.clone(),
                kind: ImportScopeDiagKind::DuplicateVisible {
                    name: local.clone(),
                    module: binding.module.clone().unwrap_or_else(|| "<import>".into()),
                },
            });
        }
    }
    scope.flat.insert(local, binding);
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

/// Build import scopes from a resolved program's ASTs.
pub fn build_import_scopes(
    sources: &[(PathBuf, String, rdra_ish_syntax::ast::Ast)],
) -> (ImportScopes, Vec<ImportScopeDiagnostic>) {
    let mut scopes = ImportScopes::default();
    let mut diags = Vec::new();

    for (source_id, (_path, _src, ast)) in sources.iter().enumerate() {
        let mut exports = HashSet::new();
        let mut module_path: Option<(String, Span)> = None;
        for item in &ast.items {
            match item {
                Item::Module(path, span) => {
                    module_path = Some((path.0.join("."), span.clone()));
                }
                Item::Instance(inst) => {
                    exports.insert(inst.id.clone());
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
            for id in &local_exports {
                scope.flat.insert(
                    id.clone(),
                    FlatBinding {
                        canonical_id: id.clone(),
                        module: local_module.clone(),
                    },
                );
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
                    for id in &exports {
                        insert_flat(
                            &mut scope,
                            &mut diags,
                            source_id,
                            imp.span.clone(),
                            id.clone(),
                            FlatBinding {
                                canonical_id: id.clone(),
                                module: Some(module_path.clone()),
                            },
                        );
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
                        if exports.contains(&item.name) {
                            let local = item.alias.clone().unwrap_or_else(|| item.name.clone());
                            insert_flat(
                                &mut scope,
                                &mut diags,
                                source_id,
                                item.span.clone(),
                                local,
                                FlatBinding {
                                    canonical_id: item.name.clone(),
                                    module: Some(module_path.clone()),
                                },
                            );
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

        for id in &local_exports {
            let span = span_for_local_id(ast, id);
            insert_flat(
                &mut scope,
                &mut diags,
                source_id,
                span,
                id.clone(),
                FlatBinding {
                    canonical_id: id.clone(),
                    module: local_module.clone(),
                },
            );
        }

        scopes.by_source.insert(source_id, scope);
    }

    (scopes, diags)
}
