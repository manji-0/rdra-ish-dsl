//! `rdra-ish codegen`: the model as domain code an agent finishes, in TypeScript, Rust, or
//! Go. See docs/design/algebraic-codegen.md. docs/compatibility.md covers the TypeScript
//! types (tests/codegen_contract.rs in the CLI pins them); the rest is experimental.
//!
//! [`build`] makes every choice once into a [`DomainModel`]; the printers only print it.
//! It prints the data layer and the states: variations and inline enums, an id type per
//! entity with a one-column primary key, entity and DTO records whose key columns take the
//! parent's id type, a sum per entity with a lifecycle column, and the obligations the data
//! layer's invariants put on the code.

mod actions;
pub mod adapter;
pub mod conformance;
mod go;
mod go_port;
/// The test the Go probe holds when bound to traces: `check --code=run` runs it.
pub use go_port::CONFORMANCE_TEST as GO_CONFORMANCE_TEST;
pub mod ir;
pub mod json;
pub mod probe;
mod rust;
mod rust_port;
mod states;
mod ts;
mod ts_port;
mod validators;
pub mod verus;
mod writes;

use rdra_ish_syntax::ast::Kind;
use std::collections::BTreeMap;

use rdra_ish_core::lifecycle::{ActionOrigin, Caps};
use rdra_ish_core::location::LocatedSpan;
use rdra_ish_core::model::{ColumnType, Entity, EntityKey, Max, Min, ModelColumn, RefAction};
use rdra_ish_core::SemanticModel;

use ir::{
    DomainModel, EnumDecl, EnumValue, Field, FieldType, IdDecl, Obligation, ObligationSubject,
    RecordDecl, RecordKind, Scalar, SumDecl, UseCaseDecl, VariantDecl,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    TypeScript,
    Rust,
    Go,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub lang: Lang,
    /// The Go package name.
    pub package: String,
    /// The input paths as the user gave them, for the header.
    pub model_path: String,
    /// The rdra-ish version, for the header.
    pub version: String,
    /// What `check` said of the model: the state sums refine only a model it passed.
    pub check: CheckStatus,
    /// Print the conformance port and harness, and write the traces of a BFS
    /// that stops at this many states.
    pub conformance: Option<usize>,
    /// TypeScript: brand the state types with a symbol only the module can name, so only its
    /// functions make a state (decision 9 of docs/design/code-check.md: opt-in).
    pub seal: bool,
    /// Rust: derive serde's `Serialize` and `Deserialize` on the data types (`[code.rust]
    /// serde`): the scalars and value sets in their wire spelling, the ids, the rows, the
    /// entities with states through their rows, the records, events, and run marks.
    pub serde: bool,
}

/// The result of the `check` the generator ran before printing.
#[derive(Clone, Debug)]
pub enum CheckStatus {
    /// `check` reported no error, within these bounds (its `bounds:` line), exploring at
    /// `caps`. The state refinements explore the model again at the same caps.
    Passed { bounds: String, caps: Caps },
    /// `check` reported these errors (`name: message`), and the caller asked for the code
    /// anyway: the state sums refine nothing, and each error is an unmet obligation.
    Failed { errors: Vec<String> },
}

#[derive(Debug, thiserror::Error)]
pub enum CodegenError {
    #[error("codegen: two declarations generate the type name `{name}`: {first} and {second}")]
    NameCollision {
        name: String,
        first: String,
        second: String,
    },
    #[error("codegen: no conformance harness for this model: {0}")]
    Conformance(String),
}

/// The code, and the traces when the options ask for the conformance harness.
pub struct Generated {
    pub code: String,
    pub traces: Option<conformance::Traces>,
    /// The domain model the code was printed from: its contracts, for the probe and the
    /// mapping of compiler errors (`check --code=build`).
    pub domain: DomainModel,
}

/// The generated code for `options.lang`. `site` formats a source location as `path:line:col`.
pub fn generate(
    model: &SemanticModel,
    options: &Options,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> Result<String, CodegenError> {
    generate_all(model, options, site).map(|g| g.code)
}

/// The generated code, with the conformance traces when `options.conformance` asks for them.
pub fn generate_all(
    model: &SemanticModel,
    options: &Options,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> Result<Generated, CodegenError> {
    let refine = match options.check {
        CheckStatus::Passed { caps, .. } => Some(caps),
        CheckStatus::Failed { .. } => None,
    };
    if options.conformance.is_some() && refine.is_none() {
        return Err(CodegenError::Conformance(
            "`check` rejects the model, so there are no transition functions to drive".to_string(),
        ));
    }
    let domain = domain_of(model, options, options.conformance.is_some(), site)?;
    let traces = options
        .conformance
        .map(|cap| {
            let mut header = vec![format!("model: {}", options.model_path)];
            if let CheckStatus::Passed { bounds, .. } = &options.check {
                header.push(format!("bounds: {bounds}"));
            }
            conformance::traces(model, &domain, cap, &header)
        })
        .transpose()?;
    let code = match options.lang {
        Lang::TypeScript => ts::print(&domain, options, traces.as_ref()),
        Lang::Rust => rust::print(&domain, options, traces.as_ref()),
        Lang::Go => go::print(&domain, options, traces.as_ref()),
    };
    Ok(Generated {
        code,
        traces,
        domain,
    })
}

/// snake_case of a model identifier, as the printers spell function and method names:
/// `CustomerAccount` is `customer_account`.
pub fn snake(id: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for ch in id.chars() {
        if ch == '-' || ch == ' ' {
            out.push('_');
            prev_lower = false;
            continue;
        }
        if ch.is_uppercase() {
            if prev_lower {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
            prev_lower = false;
        } else {
            out.push(ch);
            prev_lower = ch.is_lowercase() || ch.is_ascii_digit();
        }
    }
    out
}

/// The names of a port method's two calls in `lang`, `[prepare, commit]`: Rust
/// `prepare_pay`, Go `PreparePay`, TypeScript `preparePay`.
pub fn method_names(lang: Lang, method: &ir::PortMethod) -> [String; 2] {
    match lang {
        Lang::Rust => rust_port::method_names(method),
        Lang::Go => go_port::method_names(method),
        Lang::TypeScript => ts_port::method_names(method),
    }
}

/// The name of an entry point's method in `lang` (#335), by the entry's PascalCase name:
/// Rust `run_create_charge`, Go `RunCreateCharge`, TypeScript `runCreateCharge`.
pub fn entry_name(lang: Lang, name: &str) -> String {
    match lang {
        Lang::Rust => rust_port::entry_name(name),
        Lang::Go => go_port::entry_name(name),
        Lang::TypeScript => ts_port::entry_name(name),
    }
}

/// The name of a transition function or a dispatcher in `lang`, by its PascalCase name in the
/// IR: Rust `run_pay`, Go `RunPay`, TypeScript `runPay`.
pub fn function_name(lang: Lang, pascal: &str) -> String {
    match lang {
        Lang::Rust => rust::fn_ident(pascal),
        Lang::Go => pascal.to_string(),
        Lang::TypeScript => ts::fn_name(pascal),
    }
}

/// What the probe binds (`[code.<lang>]` in rdra.toml): the service type, the path of the
/// generated module, and for the conformance test, the expression that makes a fresh service
/// and the traces' path from the build directory.
#[derive(Clone, Debug, Default)]
pub struct ProbeBinding {
    pub service: String,
    pub module: String,
    pub conformance: Option<(String, String)>,
    /// Go: the name in the domain package's `package` clause (`[code.go] package`), which the
    /// probe names it by; `None` names it by the last element of `module`.
    pub package: Option<String>,
}

/// The probe of `domain` in `lang`: one line per contract stating the service implements it,
/// and the conformance test when the binding has one. `None` for a language with no probe
/// printer yet, or a model with no contracts.
pub fn probe_for(lang: Lang, domain: &DomainModel, binding: &ProbeBinding) -> Option<probe::Probe> {
    match lang {
        Lang::Rust => rust_port::probe(domain, binding),
        Lang::TypeScript => ts_port::probe(domain, binding),
        Lang::Go => go_port::probe(domain, binding),
    }
}

/// The contract as JSON (`codegen --format json`): the same choices the printers print, with
/// model locations. `options.lang` does not change it.
pub fn generate_json(
    model: &SemanticModel,
    options: &Options,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> Result<String, CodegenError> {
    let domain = domain_of(model, options, false, site)?;
    Ok(json::print(model, &domain, options, site))
}

/// The domain model the options ask for: refined when `check` passed, with an unmet
/// obligation per error when it did not.
fn domain_of(
    model: &SemanticModel,
    options: &Options,
    port: bool,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> Result<DomainModel, CodegenError> {
    let refine = match options.check {
        CheckStatus::Passed { caps, .. } => Some(caps),
        CheckStatus::Failed { .. } => None,
    };
    let mut domain = build_for(model, refine, port, Some(options.lang), site)?;
    if let CheckStatus::Failed { errors } = &options.check {
        for error in errors {
            let name = error.split_once(':').map_or(error.as_str(), |(n, _)| n);
            domain.obligations.push(Obligation {
                name: format!("NotMet_{}", name.trim()),
                subject: ObligationSubject::of("not-met"),
                checked: false,
                text: format!("NOT MET by the model, which `check` rejects: {error}"),
                site: None,
            });
        }
        unique_names(&mut domain.obligations);
    }
    Ok(domain)
}

/// The header every language prints as comment lines. Compatibility covers the TypeScript
/// types of a model `check` passes, so only that output says so.
fn header_lines(options: &Options) -> Vec<String> {
    let covered =
        options.lang == Lang::TypeScript && matches!(options.check, CheckStatus::Passed { .. });
    let mut lines = if covered {
        vec![
            format!(
                "Generated by rdra-ish {} codegen from {}.",
                options.version, options.model_path
            ),
            "rdra-ish's compatibility contract covers `Result`, `InvalidValue`, `InvalidRow`, the"
                .to_string(),
            "scalars, ids, value sets, state types, `<Entity>Kind` and `<Entity>Row` types below,"
                .to_string(),
            "and the signatures of their companions and `parse<Entity>Row` (docs/compatibility.md)."
                .to_string(),
            "The rest is experimental: a later release may generate different names or shapes"
                .to_string(),
            "from the same model.".to_string(),
        ]
    } else {
        vec![
            format!(
                "Generated by rdra-ish {} codegen (experimental) from {}.",
                options.version, options.model_path
            ),
            "Code generation is outside rdra-ish's compatibility contract: a later release may"
                .to_string(),
            "generate different names or shapes from the same model. Generate again after an"
                .to_string(),
            "upgrade and compare.".to_string(),
        ]
    };
    lines.extend([
        String::new(),
        "The types below are the model's data layer and its states. The code that finishes"
            .to_string(),
        "this output must keep every obligation listed: `check` and `verify` assume them."
            .to_string(),
        String::new(),
    ]);
    match &options.check {
        CheckStatus::Passed { bounds, .. } => {
            lines.push(
                "`check` passed. A state type narrows a column only where a rule says so, and"
                    .to_string(),
            );
            lines.push(format!(
                "`check` verified those rules within these bounds: {bounds}."
            ));
            lines.push("Run `rdra-ish verify` to check them at larger bounds.".to_string());
        }
        CheckStatus::Failed { .. } => {
            lines.push(
                "`check` FAILED on this model, and the output was asked for anyway: the state"
                    .to_string(),
            );
            lines.push("types narrow no column, and the obligations list each error.".to_string());
        }
    }
    lines
}

/// The obligations as comment lines, each wrapped under its id.
fn obligation_lines(domain: &DomainModel) -> Vec<String> {
    let mut lines = obligations_only(domain);
    if !domain.candidates.is_empty() {
        lines.push(String::new());
        lines.push(
            "Candidate rules: every reachable row of the state agrees, and no rule says so. A rule"
                .to_string(),
        );
        lines.push(
            "the business holds narrows the state type; one that is an accident of the model is a gap:"
                .to_string(),
        );
        for c in &domain.candidates {
            lines.push(format!("- {c}"));
        }
    }
    if !domain.skipped.is_empty() {
        lines.push(String::new());
        lines.push(
            "Use cases with no function yet (the code that finishes this output writes them):"
                .to_string(),
        );
        for s in &domain.skipped {
            lines.push(format!("- {}: {}", s.action, s.reason));
        }
    }
    lines
}

/// What the conformance harness checks, as comment lines; none without the harness.
fn conformance_lines(
    domain: &DomainModel,
    traces: Option<&conformance::Traces>,
    conform: &str,
    port: &str,
) -> Vec<String> {
    let (Some(decl), Some(t)) = (domain.harness(), traces) else {
        return Vec::new();
    };
    let mut lines = vec![
        String::new(),
        format!("Conformance: the service that finishes this output implements {port}: it seeds"),
        "an empty store, reads the store back, and runs each use case by the ids of its rows in"
        .to_string(),
        "two calls, a prepare that reads and a commit that writes in one transaction. It must pass"
        .to_string(),
        format!("{conform} with the traces written with this file."),
        format!(
            "The traces take {} of the {} moves of the printed use cases between the {} states",
            t.covered, t.moves, t.states
        ),
        format!(
            "they reach at `check`'s bounds, in {} traces of {} calls, and try {} calls the model refuses",
            t.traces, t.calls, t.refusals
        ),
        format!(
            "and {} commits after another call changed a row they write.",
            t.interleavings
        ),
    ];
    if !t.complete {
        lines.push(
            "BFS stopped at its state cap: moves past it are not in the traces, and no call is"
                .to_string(),
        );
        lines.push("tried as refused.".to_string());
    }
    if t.left_out > 0 {
        lines.push(format!(
            "{} moves of use cases with no function, or of no use case, are left out.",
            t.left_out
        ));
    }
    if !decl.unchecked.is_empty() {
        lines.push("The harness does not compare:".to_string());
        for u in &decl.unchecked {
            lines.push(format!("- {u}"));
        }
    }
    lines
}

fn obligations_only(domain: &DomainModel) -> Vec<String> {
    if domain.obligations.is_empty() {
        return vec!["Obligations: none.".to_string()];
    }
    let mut lines = vec![
        "Obligations, each by a name that stays when the model changes elsewhere (`check` and"
            .to_string(),
        "`verify` report the ones marked [checked] by the same name):".to_string(),
    ];
    for o in &domain.obligations {
        let checked = if o.checked { " [checked]" } else { "" };
        lines.push(format!("{}{checked}: {}", o.name, o.text));
        if let Some(site) = &o.site {
            lines.push(format!("    at {site}"));
        }
    }
    lines
}

/// Builds the domain model of the data layer. `refine` is the caps of the `check` the model
/// passed: with them the state types narrow by the rules and the reachable states, explored
/// at those caps; with `None` they narrow nothing.
pub fn build(
    model: &SemanticModel,
    refine: Option<Caps>,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> Result<DomainModel, CodegenError> {
    build_with(model, refine, false, site)
}

/// [`build`], with the conformance port when `port` asks for it.
pub fn build_with(
    model: &SemanticModel,
    refine: Option<Caps>,
    port: bool,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> Result<DomainModel, CodegenError> {
    build_for(model, refine, port, None, site)
}

/// The names a language's printer writes unqualified beside the generated types, so a
/// model type of that name is a collision ([`build_for`]).
pub fn reserved_names(lang: Lang) -> &'static [&'static str] {
    match lang {
        // `Symbol`: the sealed state types need the global one (`--seal`).
        Lang::TypeScript => &["Result", "assertNever", "Symbol"],
        // The sentinels errors.Is matches each case of a `<UseCase>Error` with.
        Lang::Go => &[
            "ErrNotEnabled",
            "ErrNotFound",
            "ErrRejected",
            "ErrInvalidArgument",
            "ErrStoreFailed",
        ],
        Lang::Rust => &[],
    }
}

/// A name a language's printer derives from a model name (Go `<Entity>Visitor`), which a model
/// type of the same name would break: [`build_for`] claims it after every name of the IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedName {
    /// What the printer looks it up by, with [`DomainModel::derived`]: `<lang>:<what>:<from>`,
    /// such as `go:visitor:Order`.
    pub key: String,
    /// The name the printer would like: the claimed name is this, or this with `_` appended
    /// when a model type has it already.
    pub name: String,
    /// What it is, for a collision's message.
    pub by: String,
}

/// The names `lang`'s printer derives from the IR's names, beyond the IR's own fields.
/// Each language's pull request fills in its arm.
pub fn derived_names(lang: Lang, domain: &DomainModel) -> Vec<DerivedName> {
    let mut names = lang_derived_names(lang, domain);
    // The commit part's types and helper, in every language.
    names.extend(writes::derived_names());
    names
}

fn lang_derived_names(lang: Lang, domain: &DomainModel) -> Vec<DerivedName> {
    match lang {
        // Every name the Rust output declares in the module scope is a field of the IR:
        // the Rust printer derives none.
        Lang::Rust => Vec::new(),
        Lang::Go => go::derived_names(domain),
        // The boundary parser of each row type, `parse<Row>`, claimed in PascalCase as the
        // functions are (the printer writes it in camelCase).
        Lang::TypeScript => {
            let rows = domain
                .records
                .iter()
                .map(|r| r.name.as_str())
                .chain(domain.sums.iter().map(|s| s.row.as_str()));
            rows.map(|row| DerivedName {
                key: format!("ts:parse:{row}"),
                name: format!("Parse{row}"),
                by: format!("the parser of {row}"),
            })
            .collect()
        }
    }
}

/// [`build_with`] for the printer of `lang`: the names it writes unqualified
/// ([`reserved_names`]) are claimed first, and the names it derives ([`derived_names`]) last,
/// each as it is or with `_` appended, into [`DomainModel::derived`].
pub fn build_for(
    model: &SemanticModel,
    refine: Option<Caps>,
    port: bool,
    lang: Option<Lang>,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> Result<DomainModel, CodegenError> {
    let mut names = Names::default();
    for name in lang.map_or(&[][..], reserved_names) {
        names.claim(
            name.to_string(),
            "a name the printer of this language writes".to_string(),
        )?;
    }
    let mut domain = DomainModel::default();
    // The prelude's names, and `Self`, which no language takes as a type name.
    for scalar in Scalar::PRELUDE {
        names.claim(scalar.name().to_string(), "the prelude".to_string())?;
    }
    // Names a language reserves where the printers write them unqualified.
    for reserved in [
        "Self",
        "Some",
        "None",
        "Ok",
        "Err",
        "Refused",
        ir::INVALID_ROW,
        "InvalidValue",
    ]
    .into_iter()
    .chain(ir::ERROR_CASES)
    {
        names.claim(reserved.to_string(), "a reserved word".to_string())?;
    }

    for (_, variation) in &model.variations {
        let name = names.claim(
            type_ident(&variation.id),
            format!("variation {}", variation.id),
        )?;
        domain.enums.push(EnumDecl {
            name,
            doc: doc_of(&variation.label, variation.description.as_deref()),
            values: distinct(
                variation
                    .values
                    .iter()
                    .map(|v| enum_value(&v.value, v.label.clone()))
                    .collect(),
                |v| &mut v.ident,
            ),
        });
    }

    // Ids first, so a key column can name its parent's id whatever the entity order.
    let mut id_of: BTreeMap<EntityKey, String> = BTreeMap::new();
    for (key, entity) in &model.entities {
        let [pk] = entity.primary_key.as_slice() else {
            continue;
        };
        let Some(column) = entity.columns.iter().find(|c| c.name == *pk) else {
            continue;
        };
        let Some(repr) = scalar_of(&column.col_type) else {
            continue;
        };
        let name = names.claim(
            format!("{}Id", type_ident(&entity.id)),
            format!("the id of entity {}", entity.id),
        )?;
        id_of.insert(key, name.clone());
        domain.ids.push(IdDecl {
            name,
            entity: entity.id.clone(),
            repr,
        });
    }

    // Each state type waits for its discriminant's name: (type name, entity, the rest).
    let mut sums = Vec::new();
    for (key, entity) in &model.entities {
        let name = names.claim(type_ident(&entity.id), format!("entity {}", entity.id))?;
        let mut fields = Vec::new();
        for column in &entity.columns {
            let ty = column_type(model, key, entity, column, &id_of, &mut names, &mut domain)?;
            fields.push(field(column, ty));
        }
        let fields = distinct(fields, |f| &mut f.pascal);
        let doc = doc_of(&entity.label, entity.description.as_deref());
        match states::tag_column(model, key, entity) {
            Some((tag, values)) => {
                let refinements = if refine.is_some() {
                    states::refinements(model, key, entity, tag, values)
                } else {
                    states::Refinements::new()
                };
                let sum = sum_decl(
                    name.clone(),
                    entity,
                    doc,
                    fields,
                    (tag, values),
                    &refinements,
                    &mut names,
                    site,
                )?;
                sums.push((name, &entity.id, sum));
            }
            None => domain.records.push(RecordDecl {
                name,
                id: entity.id.clone(),
                kind: RecordKind::Entity,
                doc,
                fields,
            }),
        }
    }

    for (_, dto) in &model.dtos {
        let name = names.claim(type_ident(&dto.id), format!("dto {}", dto.id))?;
        let mut fields = Vec::new();
        for column in &dto.fields {
            let ty = match &column.col_type {
                ColumnType::Enum(values) => {
                    inline_enum_type(&mut names, &mut domain, &name, &dto.id, column, values)?
                }
                other => value_type(model, other),
            };
            fields.push(field(column, ty));
        }
        domain.records.push(RecordDecl {
            name,
            id: dto.id.clone(),
            kind: RecordKind::Dto,
            doc: doc_of(&dto.label, dto.description.as_deref()),
            fields: distinct(fields, |f| &mut f.pascal),
        });
    }

    // The discriminant types, once every type of the data layer has its name: the one name
    // a column's enum or a state may already have is `<Entity>Kind`.
    for (name, entity, sum) in sums {
        let kind = claim_first(
            &mut names,
            [format!("{name}Kind"), format!("{name}StateKind")],
            format!("the kind of {entity}"),
        )?;
        domain.sums.push(sum(kind));
    }

    domain.obligations = obligations(model, site);
    domain.usecases = usecases(model, site);

    // Transition functions and rule checks rest on the verified system: only a model `check`
    // passed gets them.
    if let Some(caps) = refine {
        let built = rdra_ish_core::lifecycle::TransitionSystem::build(
            model,
            rdra_ish_core::lifecycle::Bounds::STATES,
            &rdra_ish_core::lifecycle::Scope::ALL,
        );
        actions::add_functions(model, &built.ts, &mut domain, &mut names, site)?;
        validators::add_validators(&built.ts, &mut domain, &mut names, site)?;
        domain.candidates = states::candidates(model, &domain.sums, caps);
        // The port's contracts are printed with or without the harness, so the probe has
        // them; a model the harness cannot name the rows of has none, and is an error only
        // when the harness is asked for.
        let mut trial = names.clone();
        match conformance::port(model, &built.ts, &domain, &mut trial) {
            Ok(mut decl) => {
                names = trial;
                decl.harness = port;
                domain.port = Some(decl);
                conformance::add_entries(&built.ts, &mut domain, &mut names)?;
                conformance::add_callers(model, &mut domain, &mut names)?;
                conformance::claim_unimplemented(&domain, &mut names)?;
            }
            Err(e) if port => return Err(e),
            Err(_) => {}
        }
        for uc in &mut domain.usecases {
            uc.actions = built
                .ts
                .actions()
                .iter()
                .filter(|a| {
                    matches!(&a.origin, ActionOrigin::UseCase { uc: k, .. }
                        if model.use_cases[*k].id == uc.id)
                })
                .map(|a| a.name.clone())
                .collect();
            uc.error = domain
                .errors
                .iter()
                .find(|e| e.usecase == uc.id)
                .map(|e| e.name.clone());
        }
    }
    unique_names(&mut domain.obligations);

    // Go declares `<Enum>Values` and one `<Enum><Value>` constant per value in the package
    // scope the types share, so those names are claimed too.
    for decl in &domain.enums {
        names.claim(
            format!("{}Values", decl.name),
            format!("the values of {}", decl.name),
        )?;
        names.claim(
            format!("Parse{}", decl.name),
            format!("the parser of {}", decl.name),
        )?;
        for value in &decl.values {
            names.claim(
                format!("{}{}", decl.name, value.ident),
                format!("the value {} of {}", value.wire, decl.name),
            )?;
        }
    }
    if let Some(lang) = lang {
        let derived = derived_names(lang, &domain);
        claim_derived(&mut names, &mut domain, derived)?;
    }
    Ok(domain)
}

/// Claims each derived name as it is, or with `_` appended when a model type has it, into
/// `domain.derived`; a collision of both spellings is an error naming both claimants.
fn claim_derived(
    names: &mut Names,
    domain: &mut DomainModel,
    derived: Vec<DerivedName>,
) -> Result<(), CodegenError> {
    for d in derived {
        let name = claim_first(names, [d.name.clone(), format!("{}_", d.name)], d.by)?;
        domain.derived.insert(d.key, name);
    }
    Ok(())
}

/// The sum of an entity with a lifecycle column: one variant per value of its tag, each
/// with the columns the refinements narrow. It is complete once its discriminant type has
/// a name, which is claimed after every type of the data layer.
#[allow(clippy::too_many_arguments)]
fn sum_decl(
    name: String,
    entity: &Entity,
    doc: Vec<String>,
    fields: Vec<Field>,
    (tag, values): (&str, &[String]),
    refinements: &states::Refinements,
    names: &mut Names,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> Result<impl FnOnce(String) -> SumDecl, CodegenError> {
    let row_fields = fields.clone();
    let row = names.claim(
        format!("{name}Row"),
        format!("the row type of {}", entity.id),
    )?;
    let (tag_field, rest): (Vec<Field>, Vec<Field>) =
        fields.into_iter().partition(|f| f.wire == tag);
    let tag_field = tag_field
        .into_iter()
        .next()
        .expect("the tag column is a column of the entity");
    let mut variants = Vec::new();
    let values = values.iter().map(|v| enum_value(v, None)).collect();
    for value in distinct(values, |v| &mut v.ident) {
        let variant_name = names.claim(
            format!("{name}{}", value.ident),
            format!("the state {} of {}.{tag}", value.wire, entity.id),
        )?;
        let mut fields = Vec::new();
        let mut always_null = Vec::new();
        for field in &rest {
            let refinement = refinements.get(&(value.wire.clone(), field.wire.clone()));
            let mut field = field.clone();
            if field.wire == "kind" {
                // `kind` is the discriminant of a state type; the column keeps its name in the row.
                field.name = "kind_".to_string();
            }
            let at = refinement
                .and_then(|r| r.site.as_ref())
                .and_then(site)
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            match refinement.map(|r| r.refined) {
                Some(states::Refined::Present) => {
                    field.nullable = false;
                    field
                        .doc
                        .push(format!("Present in this state by a rule{at}."));
                    fields.push(field);
                }
                Some(states::Refined::Null) => {
                    field.doc.push(format!("Null in this state by a rule{at}."));
                    always_null.push(field);
                }
                None => fields.push(field),
            }
        }
        variants.push(VariantDecl {
            name: variant_name,
            doc: vec![format!(
                "{} in the state `{}` of {tag}.",
                entity.id, value.wire
            )],
            value,
            fields,
            always_null,
        });
    }
    let entity = entity.id.clone();
    Ok(move |kind| SumDecl {
        name,
        entity,
        doc,
        tag: tag_field,
        variants,
        row,
        row_fields,
        kind,
    })
}

fn column_type(
    model: &SemanticModel,
    key: EntityKey,
    entity: &Entity,
    column: &ModelColumn,
    id_of: &BTreeMap<EntityKey, String>,
    names: &mut Names,
    domain: &mut DomainModel,
) -> Result<FieldType, CodegenError> {
    // The entity's own one-column primary key.
    if entity.primary_key.len() == 1 && entity.primary_key[0] == column.name {
        if let Some(id) = id_of.get(&key) {
            return Ok(FieldType::Id(id.clone()));
        }
    }
    // A one-column key to a parent's id (not a `.references` to another unique column).
    if let Some((_, fk)) = model.column_fk(key, &column.name) {
        let parent_pk = &model.entities[fk.parent].primary_key;
        if matches!(fk.columns.as_slice(), [(_, p)] if parent_pk.as_slice() == [p.clone()]) {
            if let Some(id) = id_of.get(&fk.parent) {
                return Ok(FieldType::Id(id.clone()));
            }
        }
    }
    Ok(match &column.col_type {
        ColumnType::Enum(values) => inline_enum_type(
            names,
            domain,
            &type_ident(&entity.id),
            &entity.id,
            column,
            values,
        )?,
        other => value_type(model, other),
    })
}

fn value_type(model: &SemanticModel, col_type: &ColumnType) -> FieldType {
    match col_type {
        ColumnType::Variation(v) => FieldType::Enum(type_ident(&model.variations[*v].id)),
        other => FieldType::Scalar(scalar_of(other).unwrap_or(Scalar::String)),
    }
}

fn scalar_of(col_type: &ColumnType) -> Option<Scalar> {
    Some(match col_type {
        ColumnType::Int => Scalar::Int,
        ColumnType::String => Scalar::String,
        ColumnType::Bool => Scalar::Bool,
        ColumnType::Money => Scalar::Money,
        ColumnType::Decimal => Scalar::Decimal,
        ColumnType::Date => Scalar::Date,
        ColumnType::DateTime => Scalar::DateTime,
        ColumnType::Enum(_) | ColumnType::Variation(_) => return None,
    })
}

/// The type of an Enum column that has no variation: a new enum named after its owner's type
/// and the column.
fn inline_enum_type(
    names: &mut Names,
    domain: &mut DomainModel,
    owner_type: &str,
    owner: &str,
    column: &ModelColumn,
    values: &[String],
) -> Result<FieldType, CodegenError> {
    let name = names.claim(
        format!("{owner_type}{}", type_ident(&column.name)),
        format!("the Enum of {owner}.{}", column.name),
    )?;
    domain
        .enums
        .push(inline_enum(name.clone(), owner, column, values));
    Ok(FieldType::Enum(name))
}

fn inline_enum(name: String, owner: &str, column: &ModelColumn, values: &[String]) -> EnumDecl {
    EnumDecl {
        name,
        doc: vec![format!("The values of {owner}.{}.", column.name)],
        values: distinct(
            values
                .iter()
                .enumerate()
                .map(|(i, v)| enum_value(v, column.value_labels.get(i).cloned().flatten()))
                .collect(),
            |v| &mut v.ident,
        ),
    }
}

/// Items whose names coincide (`paid` and `Paid`, `due_at` and `dueAt`) keep the first as it is
/// and number the others, `Paid2`, so every name among them is distinct.
fn distinct<T>(mut items: Vec<T>, name: fn(&mut T) -> &mut String) -> Vec<T> {
    let mut seen: Vec<String> = Vec::new();
    for item in &mut items {
        let base = name(item).clone();
        let mut candidate = base.clone();
        let mut n = 2;
        while seen.contains(&candidate) {
            candidate = format!("{base}{n}");
            n += 1;
        }
        seen.push(candidate.clone());
        *name(item) = candidate;
    }
    items
}

fn enum_value(wire: &str, label: Option<String>) -> EnumValue {
    EnumValue {
        wire: wire.to_string(),
        ident: type_ident(wire),
        label,
    }
}

fn field(column: &ModelColumn, ty: FieldType) -> Field {
    let mut doc = Vec::new();
    if let Some(label) = &column.label {
        doc.push(label.clone());
    }
    if column.is_pk {
        doc.push("Primary key.".to_string());
    }
    if let Some(default) = &column.default_val {
        doc.push(format!("Default: {default}."));
    }
    if let Some(expr) = &column.derived_expr {
        doc.push(format!(
            "Derived: {}. Compute it; never set it on its own (see the obligations).",
            expr.display()
        ));
    }
    Field {
        wire: column.name.clone(),
        name: column.name.clone(),
        pascal: type_ident(&column.name),
        ty,
        nullable: column.is_nullable,
        doc,
    }
}

fn doc_of(label: &str, description: Option<&str>) -> Vec<String> {
    let mut doc = vec![label.to_string()];
    if let Some(d) = description.map(str::trim).filter(|d| !d.is_empty()) {
        doc.push(d.to_string());
    }
    doc
}

/// What the data layer's invariants and constraints ask of the code, in entity order. Each
/// name is built from the model's identifiers, never from a position, so adding an unrelated
/// declaration moves none.
fn obligations(
    model: &SemanticModel,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> Vec<Obligation> {
    let mut out: Vec<Obligation> = Vec::new();
    let mut push = |name: String,
                    checked: bool,
                    text: String,
                    at: Option<&LocatedSpan>,
                    subject: ObligationSubject| {
        out.push(Obligation {
            name,
            checked,
            text,
            subject,
            site: at.and_then(site),
        });
    };
    for (key, entity) in &model.entities {
        // An obligation with no declaration of its own is at its entity's.
        let entity_at = model.decl_sites.located(Kind::Entity, &entity.id);
        let on = |kind: &'static str, columns: &[&str]| ObligationSubject {
            kind,
            entity: Some(entity.id.clone()),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            parent: None,
        };
        for (_, fk) in model.foreign_keys().filter(|(_, fk)| fk.child == key) {
            if fk.columns.is_empty() {
                continue;
            }
            let parent = &model.entities[fk.parent];
            let role = model.key_role(fk);
            let cols = fk
                .columns
                .iter()
                .map(|(c, _)| format!("{}.{c}", entity.id))
                .collect::<Vec<_>>()
                .join(", ");
            let col_names = fk
                .columns
                .iter()
                .map(|(c, _)| c.as_str())
                .collect::<Vec<_>>()
                .join("_");
            let nullable = if fk.optional() { " when set" } else { "" };
            let keyed = |kind: &'static str| ObligationSubject {
                kind,
                entity: Some(entity.id.clone()),
                columns: fk.columns.iter().map(|(c, _)| c.clone()).collect(),
                parent: Some(parent.id.clone()),
            };
            push(
                format!("Ref_{}_{col_names}", entity.id),
                false,
                format!("{cols} names a present {}{nullable}.", parent.id),
                fk.site.as_ref(),
                keyed("reference"),
            );
            if fk.children.max == Max::One {
                push(
                    format!("Unique_{}_{role}", entity.id),
                    true,
                    format!(
                        "no two {} rows share one {} through {cols}.",
                        entity.id, parent.id
                    ),
                    fk.site.as_ref(),
                    keyed("unique"),
                );
            }
            if fk.children.min == Min::One {
                push(
                    format!("MinCard_{}_{role}", entity.id),
                    true,
                    format!(
                        "every present {} has at least one {} through {cols}.",
                        parent.id, entity.id
                    ),
                    fk.site.as_ref(),
                    keyed("min-children"),
                );
            }
            if let Some(most) = fk.children.most {
                push(
                    format!("MaxCard_{}_{role}", entity.id),
                    true,
                    format!(
                        "every present {} has at most {most} {} through {cols}.",
                        parent.id, entity.id
                    ),
                    fk.site.as_ref(),
                    keyed("max-children"),
                );
            }
            for (what, prefix, kind, action) in [
                ("delete", "OnDelete", "on-delete", fk.on_delete),
                ("update", "OnUpdate", "on-update", fk.on_update),
            ] {
                if let Some(action) = action {
                    push(
                        format!("{prefix}_{}_{col_names}", entity.id),
                        false,
                        format!(
                            "on {what} of a {}, apply {} to its {} rows through {cols}.",
                            parent.id,
                            ref_action(action),
                            entity.id
                        ),
                        fk.site.as_ref(),
                        keyed(kind),
                    );
                }
            }
        }
        // A 1:1 key column is unique through its `Unique_*` obligation above.
        let one_to_one = |column: &str| {
            model
                .column_fk(key, column)
                .is_some_and(|(_, fk)| fk.unique())
        };
        for column in &entity.columns {
            if column.is_unique && !column.is_pk && !one_to_one(&column.name) {
                push(
                    format!("Unique_{}_{}", entity.id, column.name),
                    false,
                    format!("no two {} rows share {}.", entity.id, column.name),
                    entity_at.as_ref(),
                    on("unique", &[column.name.as_str()]),
                );
            }
            for (i, check) in column.check_constraints.iter().enumerate() {
                push(
                    format!("Check_{}_{}_{i}", entity.id, column.name),
                    true,
                    format!(
                        "every {} row satisfies {} (@check on {}).",
                        entity.id,
                        check.expr.display(),
                        column.name
                    ),
                    check.site.as_ref(),
                    on("check", &[column.name.as_str()]),
                );
            }
            if let Some(expr) = &column.derived_expr {
                push(
                    format!("Derived_{}_{}", entity.id, column.name),
                    false,
                    format!(
                        "{}.{} always equals {}.",
                        entity.id,
                        column.name,
                        expr.display()
                    ),
                    entity_at.as_ref(),
                    on("derived", &[column.name.as_str()]),
                );
            }
        }
        // A one-column set is the column's own `@unique`, listed above.
        for unique in model
            .unique_keys(key)
            .into_iter()
            .filter(|u| u.columns.len() > 1)
        {
            push(
                format!("Unique_{}_{}", entity.id, unique.columns.join("_")),
                false,
                format!(
                    "no two {} rows share ({}).",
                    entity.id,
                    unique.columns.join(", ")
                ),
                entity_at.as_ref(),
                on(
                    "unique",
                    &unique
                        .columns
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                ),
            );
        }
    }
    out
}

/// Every use case of the model, without its actions: [`build_with`] adds them from the
/// transition system of a model `check` passed.
fn usecases(
    model: &SemanticModel,
    site: &dyn Fn(&LocatedSpan) -> Option<String>,
) -> Vec<UseCaseDecl> {
    model
        .use_cases
        .values()
        .map(|uc| UseCaseDecl {
            id: uc.id.clone(),
            label: uc.label.clone(),
            site: model
                .decl_sites
                .located(Kind::UseCase, &uc.id)
                .as_ref()
                .and_then(site),
            actions: Vec::new(),
            error: None,
        })
        .collect()
}

/// Obligations that share a name. A second one of an invariant `check` names is the same
/// invariant seen from elsewhere (an `@check` the data layer lists and no one-row check
/// prints): it goes. Two others (two keys from one child to one parent through one role)
/// keep the first as it is and number the rest from 2, in model order.
fn unique_names(obligations: &mut Vec<Obligation>) {
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    obligations.retain(|o| !o.checked || seen.insert(o.name.clone()));
    for o in obligations.iter_mut().filter(|o| !o.checked) {
        let mut candidate = o.name.clone();
        let mut n = 2;
        while seen.contains(&candidate) {
            candidate = format!("{}_{n}", o.name);
            n += 1;
        }
        seen.insert(candidate.clone());
        o.name = candidate;
    }
}

fn ref_action(action: RefAction) -> &'static str {
    match action {
        RefAction::Cascade => "cascade",
        RefAction::Restrict => "restrict (refuse while rows refer to it)",
        RefAction::SetNull => "set null",
        RefAction::SetDefault => "set default",
        RefAction::NoAction => "no action",
    }
}

/// The type names given out, with the declaration that claimed each.
#[derive(Clone, Default)]
struct Names(BTreeMap<String, String>);

impl Names {
    fn claim(&mut self, name: String, by: String) -> Result<String, CodegenError> {
        if let Some(first) = self.0.get(&name) {
            return Err(CodegenError::NameCollision {
                name,
                first: first.clone(),
                second: by,
            });
        }
        self.0.insert(name.clone(), by);
        Ok(name)
    }
}

/// The first of `candidates` no declaration has, claimed; a collision naming the last when
/// every one is taken.
fn claim_first<const N: usize>(
    names: &mut Names,
    candidates: [String; N],
    by: String,
) -> Result<String, CodegenError> {
    let last = candidates.len() - 1;
    for (i, name) in candidates.into_iter().enumerate() {
        if i == last || !names.0.contains_key(&name) {
            return names.claim(name, by);
        }
    }
    unreachable!("the last candidate is always claimed")
}

/// An identifier in PascalCase: `_` and `-` split words, and the first letter of each word
/// is upper-cased. One that would not start with a letter gets the prefix `V`.
pub(crate) fn type_ident(id: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in id.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    match out.chars().next() {
        Some(c) if c.is_ascii_uppercase() => out,
        _ => format!("V{out}"),
    }
}

/// A string literal every printer accepts: `"..."` with `\` and `"` escaped.
pub(crate) fn string_literal(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Comment text on one line: a newline inside a doc would end the comment.
pub(crate) fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests;

/// camelCase of a PascalCase name.
pub(crate) fn camel(pascal: &str) -> String {
    let mut chars = pascal.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => String::new(),
    }
}
