//! The consumer side of a cross-model reference: what a dependency publishes, and how a
//! product binds it through `extsystem Auth "..." from auth.AuthSystem`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rdra_ish_syntax::ast::{BindingDecl, Kind, QRef};

use crate::analysis_diag::{push_error, push_warning};
use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::{format_diagnostic_message, DiagCtxt};
use crate::model::{
    slug, Binding, Entity, Event, ExtSystemKey, ForeignWhat, LookupResult, ModelColumn, NodeRef,
    Origin, Permission, SemanticModel, UseCase,
};
use crate::project::Project;
use crate::surface::{Surface, SurfaceColumn};

/// A loaded dependency as the product's analysis sees it: its published surface, plus the
/// names it keeps internal (for `NotExposed`) and its modules and files (for `ForeignImport`).
#[derive(Debug, Clone, PartialEq)]
pub struct DependencySurface {
    /// The `[dependencies]` key of the product's manifest.
    pub name: String,
    pub systems: Vec<String>,
    pub surface: Surface,
    /// Every declared id of the dependency with its kind, published or not.
    pub declared: BTreeMap<String, String>,
    pub modules: Vec<String>,
    pub files: Vec<PathBuf>,
}

impl DependencySurface {
    pub fn of(name: &str, project: &Project) -> DependencySurface {
        let model = &project.model;
        let mut systems: Vec<String> = model.systems.values().map(|s| s.id.clone()).collect();
        systems.sort();
        let declared = model
            .decl_sites
            .iter()
            .map(|(kind, id, _)| (id.to_string(), kind.to_string()))
            .collect();
        let mut modules: Vec<String> = model.import_scopes.modules().map(str::to_string).collect();
        modules.sort();
        let files = project
            .program
            .sources
            .iter()
            .map(|(path, _, _)| path.clone())
            .collect();
        DependencySurface {
            name: name.to_string(),
            systems,
            surface: Surface::of(model),
            declared,
            modules,
            files,
        }
    }

    /// The first error of a dependency's own `check`, as the product reports it.
    pub fn first_error(project: &Project) -> Option<String> {
        project.diags.iter().find(|d| !d.is_warning).map(|d| {
            format_diagnostic_message(
                Some(&project.program),
                false,
                d.location.as_ref(),
                &d.error.to_string(),
            )
        })
    }
}

/// Which dependency a module path or a source file belongs to, read where the product
/// resolves its imports.
#[derive(Debug, Default)]
pub struct ForeignModules {
    modules: BTreeMap<String, String>,
    files: Vec<(PathBuf, String)>,
}

impl ForeignModules {
    pub fn of(deps: &[Arc<DependencySurface>]) -> ForeignModules {
        let mut foreign = ForeignModules::default();
        for dep in deps {
            for module in &dep.modules {
                foreign
                    .modules
                    .entry(module.clone())
                    .or_insert_with(|| dep.name.clone());
            }
            for file in &dep.files {
                foreign.files.push((file.clone(), dep.name.clone()));
            }
        }
        foreign
    }

    pub fn dep_of_module(&self, module: &str) -> Option<&str> {
        self.modules.get(module).map(String::as_str)
    }

    pub fn dep_of_file(&self, path: &Path) -> Option<&str> {
        let canon = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.files
            .iter()
            .find(|(file, _)| *file == canon)
            .map(|(_, dep)| dep.as_str())
    }
}

/// What a product does with a foreign element at one predicate position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForeignUse {
    Call,
    React,
    Grant,
    Read,
}

impl ForeignUse {
    pub fn as_str(self) -> &'static str {
        match self {
            ForeignUse::Call => "call",
            ForeignUse::React => "react",
            ForeignUse::Grant => "grant",
            ForeignUse::Read => "read",
        }
    }
}

/// One predicate position that accepts a foreign operand: the predicate, the argument index,
/// the kind the stub must have, and what the product does with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForeignOperand {
    pub predicate: &'static str,
    pub position: usize,
    pub kind: &'static str,
    pub use_: ForeignUse,
}

const fn operand(
    predicate: &'static str,
    position: usize,
    kind: &'static str,
    use_: ForeignUse,
) -> ForeignOperand {
    ForeignOperand {
        predicate,
        position,
        kind,
        use_,
    }
}

/// The one table of every predicate position that accepts a foreign operand. The kind check
/// reads it for a foreign operand in place of the predicate's signature, and the language
/// reference lists it. A guard (`holds` on `Ext.Entity.column`) joins it with the
/// environment; until then it is `ForeignStateNeedsEnvironment`.
pub const FOREIGN_OPERANDS: &[ForeignOperand] = &[
    operand("invokes", 1, "usecase", ForeignUse::Call),
    operand("triggers", 0, "event", ForeignUse::React),
    operand("has_permission", 1, "permission", ForeignUse::Grant),
    operand("reads", 1, "entity", ForeignUse::Read),
    operand("shows", 1, "entity", ForeignUse::Read),
];

/// Predicates that write their entity operand: a foreign one is `ForeignWrite`.
const WRITES: &[&str] = &[
    "writes",
    "creates",
    "updates",
    "deletes",
    "sets",
    "transitions",
    "owns",
];

fn allowed(predicate: &str, position: usize, kind: &str) -> bool {
    FOREIGN_OPERANDS
        .iter()
        .any(|o| o.predicate == predicate && o.position == position && o.kind == kind)
}

/// The uses the table allows for stubs of `kind`, for a hint.
fn uses_of(kind: &str) -> String {
    let mut uses: Vec<String> = FOREIGN_OPERANDS
        .iter()
        .filter(|o| o.kind == kind)
        .map(|o| format!("`{}` (argument {})", o.predicate, o.position + 1))
        .collect();
    uses.dedup();
    uses.join(", ")
}

/// The bound external system `alias` names, if any.
pub(crate) fn bound_ext(model: &SemanticModel, alias: &str) -> Option<ExtSystemKey> {
    model
        .ext_systems
        .iter()
        .find(|(_, x)| x.id == alias && x.binding.is_some())
        .map(|(k, _)| k)
}

fn dependency<'m>(model: &'m SemanticModel, ext: ExtSystemKey) -> Option<&'m DependencySurface> {
    let binding = model.ext_systems[ext].binding.as_ref()?;
    model
        .dependencies
        .iter()
        .find(|d| d.name == binding.dep)
        .map(Arc::as_ref)
}

fn model_column(c: &SurfaceColumn) -> ModelColumn {
    ModelColumn {
        name: c.name.clone(),
        col_type: c.col_type.clone(),
        is_pk: c.is_pk,
        is_unique: c.is_unique,
        is_indexed: false,
        is_nullable: c.is_nullable,
        default_val: None,
        label: None,
        check_constraints: Vec::new(),
        is_soft_delete: false,
        is_history: false,
        is_tenant_scope: false,
        derived_expr: None,
    }
}

/// Bind `ext` to `decl.dep`'s system `decl.system` and insert a stub for everything that
/// system publishes. A stub's id is `Ext.Name`, which no local id can equal, and its label is
/// that id too, so no output prints the owner's text.
pub(crate) fn bind(
    model: &mut SemanticModel,
    ext: ExtSystemKey,
    decl: &BindingDecl,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) {
    let Some(dep) = model
        .dependencies
        .iter()
        .find(|d| d.name == decl.dep)
        .cloned()
    else {
        let mut known: Vec<&str> = model.dependencies.iter().map(|d| d.name.as_str()).collect();
        known.sort();
        push_error(
            ctx,
            diags,
            decl.span.clone(),
            RdraError::UnknownDependency {
                dep: decl.dep.clone(),
                known: if known.is_empty() {
                    "the manifest has no [dependencies]".to_string()
                } else {
                    known.join(", ")
                },
            },
        );
        return;
    };
    if !dep.systems.iter().any(|s| *s == decl.system) {
        push_error(
            ctx,
            diags,
            decl.span.clone(),
            RdraError::UnknownForeignName {
                qualifier: decl.dep.clone(),
                name: decl.system.clone(),
                closest: closest(&decl.system, dep.systems.iter().map(String::as_str)),
            },
        );
        return;
    }
    let binding = Binding {
        dep: decl.dep.clone(),
        system: decl.system.clone(),
    };
    if let Some((_, other)) = model
        .ext_systems
        .iter()
        .find(|(k, x)| *k != ext && x.binding.as_ref() == Some(&binding))
    {
        push_error(
            ctx,
            diags,
            decl.span.clone(),
            RdraError::DuplicateBinding {
                dep: decl.dep.clone(),
                system: decl.system.clone(),
                ext: other.id.clone(),
            },
        );
        return;
    }
    model.ext_systems[ext].binding = Some(binding);

    let alias = model.ext_systems[ext].id.clone();
    let origin = |foreign_id: &str, what: ForeignWhat| Origin {
        ext,
        dep: decl.dep.clone(),
        foreign_id: foreign_id.to_string(),
        what,
    };
    let surface = &dep.surface;
    let mut permissions: Vec<&str> = Vec::new();
    for uc in surface.use_cases.iter().filter(|u| u.system == decl.system) {
        let id = format!("{alias}.{}", uc.id);
        let key = model.use_cases.insert_foreign(
            UseCase {
                id: id.clone(),
                label: id.clone(),
                description: None,
                preconditions: Vec::new(),
                postconditions: Vec::new(),
                guards: Vec::new(),
                alternatives: Vec::new(),
                errors: Vec::new(),
            },
            origin(&uc.id, ForeignWhat::UseCase),
        );
        model.symbols.insert(id, NodeRef::UseCase(key));
        for p in &uc.requires {
            if !permissions.contains(&p.as_str()) {
                permissions.push(p);
            }
        }
    }
    for p in permissions {
        let id = format!("{alias}.{p}");
        let key = model.permissions.insert_foreign(
            Permission {
                id: id.clone(),
                label: id.clone(),
                description: None,
            },
            origin(p, ForeignWhat::Permission),
        );
        model.symbols.insert(id, NodeRef::Permission(key));
    }
    for ev in surface.events.iter().filter(|e| e.system == decl.system) {
        let id = format!("{alias}.{}", ev.id);
        let key = model.events.insert_foreign(
            Event {
                id: id.clone(),
                label: id.clone(),
                description: None,
            },
            origin(&ev.id, ForeignWhat::Event),
        );
        model.symbols.insert(id, NodeRef::Event(key));
    }
    for e in surface
        .entities
        .iter()
        .chain(&surface.shared)
        .filter(|e| e.system == decl.system)
    {
        let id = format!("{alias}.{}", e.id);
        let what = match e.mode {
            Some(mode) => ForeignWhat::PublishedEntity(mode),
            None => ForeignWhat::SharedEntity,
        };
        let columns: Vec<ModelColumn> = e.columns.iter().map(model_column).collect();
        let primary_key = columns
            .iter()
            .filter(|c| c.is_pk)
            .map(|c| c.name.clone())
            .collect();
        let key = model.entities.insert_foreign(
            Entity {
                id: id.clone(),
                label: id.clone(),
                description: None,
                columns,
                primary_key,
                unique_constraints: Vec::new(),
                indexes: Vec::new(),
            },
            origin(&e.id, what),
        );
        model.symbols.insert(id, NodeRef::Entity(key));
    }
}

/// Resolve `Ext.Name` against the stubs of a bound external system. `None` when `ext` is
/// not bound; otherwise the stub, or `None` after reporting why there is none.
pub(crate) fn resolve_foreign(
    model: &SemanticModel,
    qref: &QRef,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<Option<NodeRef>> {
    let [alias, name] = qref.parts.as_slice() else {
        return None;
    };
    let ext = bound_ext(model, alias)?;
    let qualified = format!("{alias}.{name}");
    let found = match model.symbols.lookup(&qualified) {
        LookupResult::Found(node) => Some(node.clone()),
        LookupResult::Ambiguous(kinds) => {
            let Some(kind) = &qref.kind_qualifier else {
                push_error(
                    ctx,
                    diags,
                    qref.span.clone(),
                    RdraError::AmbiguousReference {
                        id: qualified,
                        kinds: kinds.join(", "),
                    },
                );
                return Some(None);
            };
            model.symbols.lookup_qualified(kind, &qualified).cloned()
        }
        LookupResult::NotFound => None,
    };
    if let Some(node) = found {
        if let Some(kind) = &qref.kind_qualifier {
            let actual = crate::model::node_ref_kind(&node);
            if actual != kind.name() {
                push_error(
                    ctx,
                    diags,
                    qref.span.clone(),
                    RdraError::TypeMismatch {
                        pred: "reference".to_string(),
                        id: format!("{}::{qualified}", kind.name()),
                        actual: actual.to_string(),
                        expected: kind.name().to_string(),
                    },
                );
                return Some(None);
            }
        }
        return Some(Some(node));
    }
    let Some(dep) = dependency(model, ext) else {
        return Some(None);
    };
    let system = &model.ext_systems[ext]
        .binding
        .as_ref()
        .expect("bound")
        .system;
    match dep.declared.get(name.as_str()) {
        Some(kind) => {
            let published: Vec<String> = dep
                .surface
                .entities
                .iter()
                .filter(|e| e.system == *system)
                .map(|e| format!("read `{alias}.{}`", e.id))
                .chain(
                    dep.surface
                        .events
                        .iter()
                        .filter(|e| e.system == *system)
                        .map(|e| format!("react to `{alias}.{}`", e.id)),
                )
                .collect();
            push_error(
                ctx,
                diags,
                qref.span.clone(),
                RdraError::NotExposed {
                    qualified,
                    dep: dep.name.clone(),
                    kind: kind.clone(),
                    name: name.clone(),
                    hint: if published.is_empty() {
                        format!("{} publishes nothing a product can read or react to; ask its owners to expose what you need", dep.name)
                    } else {
                        published.join(" or ")
                    },
                },
            );
        }
        None => {
            let names = model
                .use_cases
                .foreign()
                .map(|(_, _, o)| o)
                .chain(model.events.foreign().map(|(_, _, o)| o))
                .chain(model.entities.foreign().map(|(_, _, o)| o))
                .chain(model.permissions.foreign().map(|(_, _, o)| o))
                .filter(|o| o.ext == ext)
                .map(|o| o.foreign_id.as_str());
            push_error(
                ctx,
                diags,
                qref.span.clone(),
                RdraError::UnknownForeignName {
                    qualifier: alias.clone(),
                    name: name.clone(),
                    closest: closest(name, names),
                },
            );
        }
    }
    Some(None)
}

/// Up to three published names nearest to `name`, nearest first.
fn closest<'a>(name: &str, candidates: impl Iterator<Item = &'a str>) -> String {
    let mut scored: Vec<(usize, &str)> = candidates.map(|c| (edit_distance(name, c), c)).collect();
    scored.sort();
    scored.dedup();
    let names: Vec<String> = scored
        .iter()
        .take(3)
        .map(|(_, c)| format!("`{c}`"))
        .collect();
    if names.is_empty() {
        "nothing is published".to_string()
    } else {
        names.join(", ")
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Check one resolved foreign operand against `FOREIGN_OPERANDS`. `true` when the position
/// accepts it; a shared entity also gets its `DirectEntityReference` warning.
pub(crate) fn check_foreign_operand(
    model: &SemanticModel,
    predicate: &str,
    position: usize,
    node: &NodeRef,
    span: rdra_ish_syntax::ast::Span,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    let Some(origin) = model.origin(node) else {
        return true;
    };
    let kind = origin.what.kind();
    let id = crate::lint::node_id(model, node).unwrap_or_default();
    let ext = &model.ext_systems[origin.ext].id;
    if allowed(predicate, position, kind) {
        if origin.what == ForeignWhat::SharedEntity {
            push_warning(
                ctx,
                diags,
                span,
                RdraError::DirectEntityReference {
                    id: id.clone(),
                    dep: origin.dep.clone(),
                    predicate: predicate.to_string(),
                },
            );
        }
        return true;
    }
    let err = if WRITES.contains(&predicate) {
        RdraError::ForeignWrite {
            id: id.clone(),
            dep: origin.dep.clone(),
            predicate: predicate.to_string(),
            hint: write_hint(model, origin, ext),
        }
    } else if predicate == "contains" {
        RdraError::ForeignContained {
            id: id.clone(),
            ext: ext.clone(),
        }
    } else {
        RdraError::ForeignOperand {
            id: id.clone(),
            predicate: predicate.to_string(),
            position: position + 1,
            kind: kind.to_string(),
            uses: uses_of(kind),
        }
    };
    push_error(ctx, diags, span, err);
    false
}

/// The exposed use cases of the owner that write a published entity, for `ForeignWrite`.
fn write_hint(model: &SemanticModel, origin: &Origin, ext: &str) -> String {
    let writers: Vec<String> = dependency(model, origin.ext)
        .map(|dep| {
            dep.surface
                .writers
                .iter()
                .filter(|w| w.entity == origin.foreign_id)
                .filter_map(|w| w.usecase.as_deref())
                .map(|uc| format!("`invokes(<UseCase>, {ext}.{uc})`"))
                .collect()
        })
        .unwrap_or_default();
    if writers.is_empty() {
        format!(
            "only {} writes it; no exposed use case of {} writes it, so ask its owners",
            origin.dep, origin.dep
        )
    } else {
        format!(
            "only {} writes it; ask it with {}",
            origin.dep,
            writers.join(" or ")
        )
    }
}

/// `IdShadowsForeignSlug`: a local id equal to the generated name of a stub.
pub(crate) fn shadow_diagnostics(model: &SemanticModel, diags: &mut Vec<Diagnostic>) {
    let stubs = model
        .use_cases
        .foreign()
        .map(|(_, u, _)| u.id.as_str())
        .chain(model.events.foreign().map(|(_, e, _)| e.id.as_str()))
        .chain(model.entities.foreign().map(|(_, e, _)| e.id.as_str()))
        .chain(model.permissions.foreign().map(|(_, p, _)| p.id.as_str()));
    for id in stubs {
        let slug = slug(id);
        let kinds: Vec<&str> = match model.symbols.lookup(&slug) {
            LookupResult::Found(node) => vec![crate::model::node_ref_kind(node)],
            LookupResult::Ambiguous(kinds) => kinds,
            LookupResult::NotFound => continue,
        };
        for kind in kinds {
            crate::location::push_model_decl_diagnostic(
                model,
                diags,
                kind,
                &slug,
                RdraError::IdShadowsForeignSlug {
                    id: slug.clone(),
                    qualified: id.to_string(),
                },
                false,
            );
        }
    }
}

/// `Ext.Name` as a `kind` reference, for the constraint resolvers that take an entity.
pub(crate) fn resolve_foreign_kind(
    model: &SemanticModel,
    qref: &QRef,
    kind: Kind,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Option<Option<NodeRef>> {
    let node = resolve_foreign(model, qref, ctx, diags)?;
    Some(node.filter(|n| {
        if crate::model::node_ref_kind(n) == kind.name() {
            true
        } else {
            push_error(
                ctx,
                diags,
                qref.span.clone(),
                RdraError::TypeMismatch {
                    pred: "reference".to_string(),
                    id: qref.parts.join("."),
                    actual: crate::model::node_ref_kind(n).to_string(),
                    expected: kind.name().to_string(),
                },
            );
            false
        }
    }))
}
