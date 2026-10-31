use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rdra_ish_syntax::ast::{BindingDecl, Kind, QRef, QualifiedColumnRef};

use crate::analysis_diag::{push_error, push_warning};
use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::{format_diagnostic_message, DiagCtxt};
use crate::model::{
    slug, Binding, Entity, Event, ExtSystemKey, ForeignWhat, LookupResult, ModelColumn, NodeRef,
    Origin, Permission, SemanticModel, UseCase,
};
use crate::project::Project;
use crate::surface::{Surface, SurfaceColumn};

/// A loaded dependency as the product's analysis sees it.
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

    /// Every error a dependency's own `check` reports: the semantic errors, else the
    /// lifecycle findings of error severity, as `check` prints them.
    pub fn errors(project: &Project) -> Vec<String> {
        let semantic: Vec<String> = project
            .diags
            .iter()
            .filter(|d| !d.is_warning)
            .map(|d| {
                format_diagnostic_message(
                    Some(&project.program),
                    false,
                    d.location.as_ref(),
                    &d.error.to_string(),
                )
            })
            .collect();
        if !semantic.is_empty() {
            return semantic;
        }
        crate::lifecycle::error_findings(&project.model)
    }
}

/// Which dependency a module path or a source file belongs to.
#[derive(Debug, Default)]
pub struct ForeignModules {
    modules: BTreeMap<String, String>,
    files: Vec<(PathBuf, String)>,
    /// The dependencies that declare no `system`, whose modules cannot be bound yet.
    without_system: Vec<String>,
}

impl ForeignModules {
    pub fn of(deps: &[Arc<DependencySurface>]) -> ForeignModules {
        let mut foreign = ForeignModules::default();
        for dep in deps {
            if dep.systems.is_empty() {
                foreign.without_system.push(dep.name.clone());
            }
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

    /// The hint of `ForeignImport` for `dep`: bind its system, or have its owners declare one.
    pub fn import_hint(&self, dep: &str) -> String {
        if self.without_system.iter().any(|d| d == dep) {
            format!("'{dep}' declares no `system` yet; its owners must declare one and `exposes` through it, then bind it with `extsystem X \"...\" from {dep}.<System>`")
        } else {
            format!("bind the system it publishes with `extsystem X \"...\" from {dep}.<System>` and reference its elements as `X.<Name>`")
        }
    }

    pub fn dep_of_file(&self, path: &Path) -> Option<&str> {
        let canon = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.files
            .iter()
            .find(|(file, _)| *file == canon)
            .map(|(_, dep)| dep.as_str())
    }
}

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

/// Every predicate position that accepts a foreign operand. The kind check reads it for a
/// foreign operand in place of the predicate's signature.
pub const FOREIGN_OPERANDS: &[ForeignOperand] = &[
    operand("invokes", 1, "usecase", ForeignUse::Call),
    operand("triggers", 0, "event", ForeignUse::React),
    operand("has_permission", 1, "permission", ForeignUse::Grant),
    operand("reads", 1, "entity", ForeignUse::Read),
    operand("shows", 1, "entity", ForeignUse::Read),
];

const WRITING_PREDICATES: &[&str] = &[
    "writes",
    "creates",
    "updates",
    "deletes",
    "sets",
    "transitions",
    "owns",
];

/// The argument a writing predicate writes: the entity of `writes`, `creates`, `updates`,
/// `deletes`, `sets`, and `owns`, and the column of `transitions`.
fn written_position(predicate: &str) -> Option<usize> {
    match predicate {
        "transitions" => Some(0),
        p if WRITING_PREDICATES.contains(&p) => Some(1),
        _ => None,
    }
}

/// A column operand whose entity path belongs to another model, or whose qualifier is one.
pub(crate) fn is_foreign_path(model: &SemanticModel, qcol: &QualifiedColumnRef) -> bool {
    qcol.entity.parts.len() > 1
        || qcol
            .entity
            .parts
            .first()
            .is_some_and(|alias| bound_ext(model, alias).is_some())
}

fn allowed(predicate: &str, position: usize, kind: &str) -> bool {
    FOREIGN_OPERANDS
        .iter()
        .any(|o| o.predicate == predicate && o.position == position && o.kind == kind)
}

fn uses_of(kind: &str) -> String {
    let mut uses: Vec<String> = FOREIGN_OPERANDS
        .iter()
        .filter(|o| o.kind == kind)
        .map(|o| format!("`{}` (argument {})", o.predicate, o.position + 1))
        .collect();
    uses.dedup();
    uses.join(", ")
}

pub fn bound_ext(model: &SemanticModel, alias: &str) -> Option<ExtSystemKey> {
    model
        .ext_systems
        .iter()
        .find(|(_, x)| x.id == alias && x.binding.is_some())
        .map(|(k, _)| k)
}

fn dependency(model: &SemanticModel, ext: ExtSystemKey) -> Option<&DependencySurface> {
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
        value_labels: Vec::new(),
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
    model.ext_systems[ext].binding = Some(Binding {
        dep: decl.dep.clone(),
        system: decl.system.clone(),
        resolved: false,
    });
    let Some(dep) = model
        .dependencies
        .iter()
        .find(|d| d.name == decl.dep)
        .cloned()
    else {
        let mut known: Vec<&str> = model.dependencies.iter().map(|d| d.name.as_str()).collect();
        known.sort();
        let add = format!(
            "add `{} = {{ path = \"...\" }}` under [dependencies]; a dependency of a dependency cannot be bound",
            decl.dep
        );
        let (known, hint) = if !model.has_manifest {
            (
                "there is no rdra.toml".to_string(),
                format!(
                    "create rdra.toml next to the model with `[model]` `name = \"...\"` and `[dependencies]` `{} = {{ path = \"...\" }}`",
                    decl.dep
                ),
            )
        } else if known.is_empty() {
            ("the manifest has no [dependencies]".to_string(), add)
        } else {
            (known.join(", "), add)
        };
        push_error(
            ctx,
            diags,
            decl.span.clone(),
            RdraError::UnknownDependency {
                dep: decl.dep.clone(),
                known,
                hint,
            },
        );
        return;
    };
    if !dep.systems.contains(&decl.system) {
        let hint = if dep.systems.is_empty() {
            format!(
                "'{}' declares no `system`; its owners must declare one and `exposes` through it",
                decl.dep
            )
        } else {
            format!(
                "its systems: {}",
                closest(&decl.system, dep.systems.iter().map(String::as_str))
            )
        };
        push_error(
            ctx,
            diags,
            decl.span.clone(),
            RdraError::UnknownForeignSystem {
                dep: decl.dep.clone(),
                system: decl.system.clone(),
                hint,
            },
        );
        return;
    }
    let binding = Binding {
        dep: decl.dep.clone(),
        system: decl.system.clone(),
        resolved: true,
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

/// What `Ext.Name` resolves to.
pub(crate) enum Foreign {
    /// `Ext` is not an external system bound with `from`: resolve it as a local reference.
    NotBound,
    /// The stub.
    Node(NodeRef),
    /// There is no such stub; the reason is reported here, or was at the binding.
    Unresolved,
}

/// Resolve `Ext.Name` against the stubs of a bound external system.
pub(crate) fn resolve_foreign(
    model: &SemanticModel,
    qref: &QRef,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Foreign {
    let [alias, name] = qref.parts.as_slice() else {
        return Foreign::NotBound;
    };
    let Some(ext) = bound_ext(model, alias) else {
        return Foreign::NotBound;
    };
    if !model.ext_systems[ext]
        .binding
        .as_ref()
        .is_some_and(|b| b.resolved)
    {
        return Foreign::Unresolved;
    }
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
                return Foreign::Unresolved;
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
                return Foreign::Unresolved;
            }
        }
        return Foreign::Node(node);
    }
    let Some(dep) = dependency(model, ext) else {
        return Foreign::Unresolved;
    };
    let system = &model.ext_systems[ext]
        .binding
        .as_ref()
        .expect("bound")
        .system;
    match dep.declared.get(name.as_str()) {
        Some(kind) => {
            let hint = match kind.as_str() {
                "entity" => {
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
                    if published.is_empty() {
                        format!("{} publishes nothing a product can read or react to; ask its owners to expose what you need", dep.name)
                    } else {
                        published.join(" or ")
                    }
                }
                "usecase" | "event" => format!(
                    "ask {}'s owners to publish it with `exposes({system}, {name})`",
                    dep.name
                ),
                "permission" => format!(
                    "a permission is published through the exposed use case that requires it; ask {}'s owners to expose that use case",
                    dep.name
                ),
                "system" => format!(
                    "a system is bound, not referenced: `extsystem X \"...\" from {}.{name}`",
                    dep.name
                ),
                _ => format!(
                    "a {kind} is never published; reference what {} exposes instead",
                    dep.name
                ),
            };
            let what = if kind == "system" {
                format!("{}'s system '{name}'", dep.name)
            } else {
                let kind = if kind == "usecase" {
                    "use case"
                } else {
                    kind.as_str()
                };
                format!(
                    "{}'s internal {kind} '{name}', which it does not publish",
                    dep.name
                )
            };
            push_error(
                ctx,
                diags,
                qref.span.clone(),
                RdraError::NotExposed {
                    qualified,
                    what,
                    hint,
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
    Foreign::Unresolved
}

/// A column operand whose entity is a dotted path: a comparison on another model's state is
/// `ForeignStateNeedsEnvironment`, and a path that names no entity is reported as the
/// reference resolver sees it. `false` when the entity is local, which the caller resolves
/// by its last segment.
pub(crate) fn foreign_column_operand(
    model: &SemanticModel,
    qcol: &QualifiedColumnRef,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> bool {
    if let [alias] = qcol.entity.parts.as_slice() {
        if bound_ext(model, alias).is_none() {
            return false;
        }
        // `Auth.SessionView` where a column is expected: the path names a foreign element.
        let element = QRef {
            kind_qualifier: None,
            parts: vec![alias.clone(), qcol.column.clone()],
            span: qcol.span.clone(),
        };
        match resolve_foreign(model, &element, ctx, diags) {
            Foreign::Node(node) => push_error(
                ctx,
                diags,
                qcol.span.clone(),
                RdraError::ForeignColumnMissing {
                    id: crate::lint::node_id(model, &node).unwrap_or_default(),
                    kind: crate::model::node_ref_kind(&node).to_string(),
                },
            ),
            Foreign::NotBound | Foreign::Unresolved => {}
        }
        return true;
    }
    match resolve_foreign_kind(model, &qcol.entity, Kind::Entity, ctx, diags) {
        Foreign::NotBound => {
            let (name, prefix) = qcol.entity.parts.split_last().expect("two or more parts");
            let prefix = prefix.join(".");
            if !model.import_scopes.modules().any(|m| m == prefix) {
                push_error(
                    ctx,
                    diags,
                    qcol.entity.span.clone(),
                    RdraError::UndefinedSymbol {
                        id: format!("{prefix}.{name}"),
                    },
                );
                return true;
            }
            let local = crate::analysis::resolve_arg(
                model,
                &rdra_ish_syntax::ast::PredicateArg::Ref(qcol.entity.clone()),
                ctx,
                diags,
            );
            return !matches!(local, Some(NodeRef::Entity(_)));
        }
        Foreign::Node(NodeRef::Entity(entity)) => {
            let known = model.entities[entity]
                .columns
                .iter()
                .any(|c| c.name == qcol.column);
            if known {
                push_error(
                    ctx,
                    diags,
                    qcol.entity.span.clone(),
                    RdraError::ForeignStateNeedsEnvironment {
                        id: model.entities[entity].id.clone(),
                    },
                );
            } else {
                push_error(
                    ctx,
                    diags,
                    qcol.span.clone(),
                    RdraError::UnknownColumn {
                        entity: model.entities[entity].id.clone(),
                        col: qcol.column.clone(),
                    },
                );
            }
        }
        Foreign::Node(_) | Foreign::Unresolved => {}
    }
    true
}

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
        "none".to_string()
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
    let err = if kind == "entity" && written_position(predicate) == Some(position) {
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

fn write_hint(model: &SemanticModel, origin: &Origin, ext: &str) -> String {
    let writers: Vec<String> = dependency(model, origin.ext)
        .map(|dep| {
            dep.surface
                .writers
                .iter()
                .chain(&dep.surface.shared_writers)
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

/// `ForeignSlugClash` between two stubs, and `IdShadowsForeignSlug` for a local id equal to
/// a stub's generated name.
pub(crate) fn shadow_diagnostics(model: &SemanticModel, diags: &mut Vec<Diagnostic>) {
    let stubs: Vec<(&str, &Origin)> = model
        .use_cases
        .foreign()
        .map(|(_, u, o)| (u.id.as_str(), o))
        .chain(model.events.foreign().map(|(_, e, o)| (e.id.as_str(), o)))
        .chain(model.entities.foreign().map(|(_, e, o)| (e.id.as_str(), o)))
        .chain(
            model
                .permissions
                .foreign()
                .map(|(_, p, o)| (p.id.as_str(), o)),
        )
        .collect();
    let mut by_slug: BTreeMap<String, &str> = BTreeMap::new();
    for (id, origin) in &stubs {
        let slug = slug(id);
        match by_slug.get(slug.as_str()) {
            Some(first) if *first != *id => {
                crate::location::push_model_decl_diagnostic(
                    model,
                    diags,
                    "extsystem",
                    &model.ext_systems[origin.ext].id,
                    RdraError::ForeignSlugClash {
                        first: first.to_string(),
                        second: id.to_string(),
                        slug: slug.clone(),
                    },
                    false,
                );
            }
            Some(_) => {}
            None => {
                by_slug.insert(slug, id);
            }
        }
    }
    for (id, _) in stubs {
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

pub(crate) fn resolve_foreign_kind(
    model: &SemanticModel,
    qref: &QRef,
    kind: Kind,
    ctx: DiagCtxt,
    diags: &mut Vec<Diagnostic>,
) -> Foreign {
    match resolve_foreign(model, qref, ctx, diags) {
        Foreign::Node(n) if crate::model::node_ref_kind(&n) != kind.name() => {
            push_error(
                ctx,
                diags,
                qref.span.clone(),
                RdraError::TypeMismatch {
                    pred: "reference".to_string(),
                    id: qref.parts.join("."),
                    actual: crate::model::node_ref_kind(&n).to_string(),
                    expected: kind.name().to_string(),
                },
            );
            Foreign::Unresolved
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIAGNOSTICS: &[&str] = &[
        "UnknownDependency",
        "DuplicateBinding",
        "UnknownForeignName",
        "UnknownForeignSystem",
        "ForeignColumnMissing",
        "NotExposed",
        "ForeignWrite",
        "ForeignContained",
        "ForeignOperand",
        "ForeignImport",
        "ForeignStateNeedsEnvironment",
        "ForeignSlugClash",
        "IdShadowsForeignSlug",
        "DirectEntityReference",
        "BindingOnNonExtSystem",
    ];

    #[test]
    fn the_operand_table_and_every_diagnostic_are_in_the_language_reference() {
        let reference = include_str!("../../../docs/language-reference.md");
        for o in FOREIGN_OPERANDS {
            let row = format!(
                "| `{}` | {} | {} | {} |",
                o.predicate,
                o.position + 1,
                o.kind,
                o.use_.as_str()
            );
            assert!(
                reference.contains(&row),
                "{row} missing from docs/language-reference.md"
            );
        }
        for name in DIAGNOSTICS {
            assert!(
                reference.contains(&format!("`{name}`")),
                "{name} missing from docs/language-reference.md"
            );
        }
    }

    #[test]
    fn closest_names_are_nearest_first_and_at_most_three() {
        assert_eq!(
            closest(
                "Logon",
                ["Login", "CanLogin", "SessionView", "Revoke"].into_iter()
            ),
            "`Login`, `CanLogin`, `Revoke`"
        );
        assert_eq!(closest("X", std::iter::empty()), "none");
    }
}
