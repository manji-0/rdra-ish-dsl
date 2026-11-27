//! The black-box model adapter (#439, PR 3): a file of its own, generated from the IR's
//! [`PortDecl`], that a service implements to be driven along the conformance traces without
//! the generated core. It holds plain data only, the model's view of a store and of a call,
//! so a project may rename or restructure the core and map it in a small adapter.
//!
//! - `ModelState`: per entity, the rows with their id and the columns the model tracks; the
//!   pending run marks; the raises a triggered use case reads. Untracked columns are left out:
//!   the adapter picks their values when it seeds.
//! - `Call`: one case per action with functions: the ids of its rows and run marks, its
//!   event, its inputs and its choices.
//! - `ModelAdapter`: `reset`, `seed`, `observe`, `invoke` (ran, or refused with a kind), and
//!   optionally `prepare` and `commit` for the interleaved rows (`I`) of the traces. An
//!   adapter without them has those rows run as plain calls and counted as skipped.
//! - `conform`: the harness, which reads the same traces as the generated port's harness and
//!   reports a difference in the same words, so the self-check of `check --code=run` works
//!   through it unchanged.
//!
//! [`decl`] makes the language-neutral choices once; `rust`, `go` and `ts` print them.

pub(super) mod go;
pub(super) mod rust;
pub(super) mod ts;

use super::ir::{DomainModel, FieldType, PortArg, PortDecl, Scalar, Track};
use super::{type_ident, CodegenError, Names};

/// The names every language's adapter file declares, claimed before the model's.
const FIXED: [&str; 14] = [
    "ModelAdapter",
    "SplitAdapter",
    "ModelState",
    "Call",
    "Refusal",
    "Outcome",
    "AdapterReport",
    "Conform",
    "Ran",
    "NotEnabled",
    "Rejected",
    "InvalidArgument",
    "NotFound",
    "StoreFailed",
];

/// A value as the adapter holds it: plain data, with no type of the generated core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Val {
    Int,
    Bool,
    /// Text as the traces spell it: a string, a date, a decimal, a money amount.
    Text,
    /// An enum of the adapter file, by its index in [`Adapter::enums`].
    Enum(usize),
}

/// One enum of the adapter file: the model's values, by wire spelling and identifier.
#[derive(Clone, Debug)]
pub(super) struct AdEnum {
    pub name: String,
    /// `(wire, PascalCase)`.
    pub values: Vec<(String, String)>,
}

/// A column of a `ModelState` row.
#[derive(Clone, Debug)]
pub(super) struct AdColumn {
    /// The column's name as the model spells it, which the traces print.
    pub wire: String,
    pub val: Val,
    /// The model tracks only whether it is set: the adapter holds a bool.
    pub presence: bool,
    /// Null is a value (an optional value in the adapter), when not `presence`.
    pub nullable: bool,
}

/// What a token of a traces row `R` is, column by column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RowToken {
    Id,
    /// The tracked column of this index.
    Tracked(usize),
    /// A column the model does not track: the adapter picks its value.
    Skip,
}

#[derive(Clone, Debug)]
pub(super) struct AdEntity {
    /// The entity id, as the traces name it.
    pub entity: String,
    /// The `ModelState` field, snake_case.
    pub field: String,
    /// The row type: `<Entity>Row`.
    pub name: String,
    /// The primary key column.
    pub id: AdColumn,
    pub tracked: Vec<AdColumn>,
    /// The tokens of a row `R` of the traces, one per column of the core's row type.
    pub tokens: Vec<RowToken>,
}

/// The run marks of one split use case on one entity.
#[derive(Clone, Debug)]
pub(super) struct AdRun {
    pub name: String,
    pub field: String,
    pub usecase: String,
    pub entity: String,
    pub id: Val,
}

/// One event type: the payload a triggered call takes, and the raises the state holds.
#[derive(Clone, Debug)]
pub(super) struct AdEvent {
    /// The event type as the traces name it.
    pub ty: String,
    /// The payload type, which a `Call` carries.
    pub name: String,
    /// The type of a raise in `ModelState`: the ids the model remembers, `<Event>Raised`.
    pub raised: String,
    /// The `ModelState` field, snake_case; `None` when the model remembers no raise of it.
    pub field: Option<String>,
    /// `(snake_case name, entity, id value, whether the model remembers the raise on it)`.
    pub ids: Vec<(String, String, Val, bool)>,
}

/// An argument of a call.
#[derive(Clone, Debug)]
pub(super) enum ArgKind {
    Value {
        val: Val,
        nullable: bool,
    },
    /// The event of a triggered use case, by its index in [`Adapter::events`].
    Event(usize),
}

#[derive(Clone, Debug)]
pub(super) struct AdArg {
    /// snake_case, as the port names it.
    pub name: String,
    pub kind: ArgKind,
}

/// One action as a call.
#[derive(Clone, Debug)]
pub(super) struct AdCall {
    /// The transition system's action, which the traces name.
    pub action: String,
    /// PascalCase: the case of `Call`.
    pub pascal: String,
    /// Go: the struct of the case, `<Action>Call`.
    pub name: String,
    pub usecase: String,
    pub args: Vec<AdArg>,
}

/// Everything an adapter file declares.
#[derive(Clone, Debug)]
pub(super) struct Adapter {
    pub enums: Vec<AdEnum>,
    pub entities: Vec<AdEntity>,
    pub runs: Vec<AdRun>,
    pub events: Vec<AdEvent>,
    pub calls: Vec<AdCall>,
    /// What the harness does not compare, as the port says.
    pub unchecked: Vec<String>,
}

/// The adapter of `domain`'s port, or why there is none: a model with no harness port.
pub(super) fn decl(domain: &DomainModel) -> Result<Adapter, CodegenError> {
    let port: &PortDecl = domain.harness().ok_or_else(|| {
        CodegenError::Conformance(
            "the adapter drives the conformance traces: generate it with them".to_string(),
        )
    })?;
    let mut names = Names::default();
    for name in FIXED {
        names.claim(name.to_string(), "the model adapter".to_string())?;
    }
    // The enums the adapter declares, each with the IR's name it stands for.
    let mut enums: Vec<(String, AdEnum)> = Vec::new();
    let mut enum_of = |ir: &str, names: &mut Names| -> Result<usize, CodegenError> {
        if let Some(i) = enums.iter().position(|(n, _)| n == ir) {
            return Ok(i);
        }
        let decl = domain
            .enums
            .iter()
            .find(|e| e.name == ir)
            .expect("an enum type the IR names is declared");
        let name = super::claim_first(
            names,
            [decl.name.clone(), format!("{}Value", decl.name)],
            format!("the value set {}", decl.name),
        )?;
        enums.push((
            ir.to_string(),
            AdEnum {
                name,
                values: decl
                    .values
                    .iter()
                    .map(|v| (v.wire.clone(), v.ident.clone()))
                    .collect(),
            },
        ));
        Ok(enums.len() - 1)
    };
    let id_val = |id_ty: &str| match domain.id_repr(id_ty) {
        Scalar::Int => Val::Int,
        _ => Val::Text,
    };
    let mut val_of = |ty: &FieldType, names: &mut Names| -> Result<Val, CodegenError> {
        Ok(match ty {
            FieldType::Scalar(Scalar::Int) => Val::Int,
            FieldType::Scalar(Scalar::Bool) => Val::Bool,
            FieldType::Scalar(_) => Val::Text,
            FieldType::Id(id) => id_val(id),
            FieldType::Enum(e) => Val::Enum(enum_of(e, names)?),
        })
    };

    let mut entities = Vec::new();
    for e in &port.entities {
        let mut tracked = Vec::new();
        for (field, how) in &e.tracked {
            let presence = *how == Track::Presence;
            tracked.push(AdColumn {
                wire: field.wire.clone(),
                val: if presence {
                    Val::Bool
                } else {
                    val_of(&field.ty, &mut names)?
                },
                presence,
                nullable: field.nullable && !presence,
            });
        }
        let tokens = e
            .columns
            .iter()
            .map(|c| {
                if c.wire == e.id.wire {
                    RowToken::Id
                } else if let Some(i) = tracked.iter().position(|t| t.wire == c.wire) {
                    RowToken::Tracked(i)
                } else {
                    RowToken::Skip
                }
            })
            .collect();
        let pascal = type_ident(&e.entity);
        entities.push(AdEntity {
            entity: e.entity.clone(),
            field: e.field.clone(),
            name: super::claim_first(
                &mut names,
                [format!("{pascal}Row"), format!("{pascal}ModelRow")],
                format!("the tracked columns of {}", e.entity),
            )?,
            id: AdColumn {
                wire: e.id.wire.clone(),
                val: val_of(&e.id.ty, &mut names)?,
                presence: false,
                nullable: false,
            },
            tracked,
            tokens,
        });
    }

    let mut runs = Vec::new();
    for r in &port.runs {
        runs.push(AdRun {
            name: super::claim_first(
                &mut names,
                [r.name.clone(), format!("{}Mark", r.name)],
                format!("the run marks of {} on {}", r.usecase, r.entity),
            )?,
            field: r.field.clone(),
            usecase: r.usecase.clone(),
            entity: r.entity.clone(),
            id: id_val(&r.id_ty),
        });
    }

    let mut events = Vec::new();
    // The IR's name of each event of `events`.
    let mut event_ir: Vec<&str> = Vec::new();
    for ev in &domain.events {
        let remembered = port.events.iter().find(|p| p.ty == ev.name);
        let used = remembered.is_some()
            || port
                .methods
                .iter()
                .any(|m| m.event.as_ref().is_some_and(|a| a.ty == ev.name));
        if !used {
            continue;
        }
        let ids = ev
            .fields
            .iter()
            .map(|f| {
                let tracked =
                    remembered.is_some_and(|p| p.ids.iter().any(|(pf, t)| *t && pf.name == f.name));
                (f.name.clone(), f.entity.clone(), id_val(&f.id_ty), tracked)
            })
            .collect();
        events.push(AdEvent {
            ty: ev.name.clone(),
            name: super::claim_first(
                &mut names,
                [ev.name.clone(), format!("{}Event", ev.name)],
                format!("the event {}", ev.name),
            )?,
            raised: super::claim_first(
                &mut names,
                [
                    format!("{}Raised", ev.name),
                    format!("{}ModelRaised", ev.name),
                ],
                format!("the raises of {}", ev.name),
            )?,
            field: remembered.map(|p| p.field.clone()),
            ids,
        });
        event_ir.push(&ev.name);
    }

    let mut calls = Vec::new();
    for m in &port.methods {
        let mut args = Vec::new();
        for a in m.args() {
            let kind = match a {
                PortArg::Id(a) => ArgKind::Value {
                    val: id_val(&a.ty),
                    nullable: false,
                },
                PortArg::Event(a) => ArgKind::Event(
                    event_ir
                        .iter()
                        .position(|ir| *ir == a.ty)
                        .expect("a triggered call's event is in the adapter"),
                ),
                PortArg::Input(i) => ArgKind::Value {
                    val: val_of(&i.ty, &mut names)?,
                    nullable: i.nullable,
                },
                PortArg::Choice(a) => ArgKind::Value {
                    val: val_of(&FieldType::Enum(a.ty.clone()), &mut names)?,
                    nullable: false,
                },
            };
            args.push(AdArg {
                name: a.name().to_string(),
                kind,
            });
        }
        calls.push(AdCall {
            action: m.action.clone(),
            pascal: m.name.clone(),
            name: super::claim_first(
                &mut names,
                [format!("{}Call", m.name), format!("{}ModelCall", m.name)],
                format!("the call of {}", m.action),
            )?,
            usecase: m.usecase.clone(),
            args,
        });
    }
    Ok(Adapter {
        enums: enums.into_iter().map(|(_, e)| e).collect(),
        entities,
        runs,
        events,
        calls,
        unchecked: port.unchecked.clone(),
    })
}

/// What the adapter probe's conformance test prints on a pass, before the counts: a run
/// command that shows the test's output lets `check --code=run` say what was run.
pub const REPORT: &str = "rdra-ish conformance: ok ";

/// The counts a passing adapter run printed after [`REPORT`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ran {
    pub traces: usize,
    pub calls: usize,
    pub refusals: usize,
    /// Interleaved rows run with the adapter's `prepare` and `commit`.
    pub interleavings: usize,
    /// Interleaved rows run as plain calls: the adapter has no `prepare` and `commit`.
    pub skipped: usize,
    /// Calls and refused calls made again by `prepare` and then `commit`.
    pub split: usize,
}

impl Ran {
    /// The counts of the last [`REPORT`] line in `output`, when it has one.
    pub fn read(output: &str) -> Option<Ran> {
        let line = output
            .lines()
            .rev()
            .find_map(|l| l.find(REPORT).map(|at| &l[at + REPORT.len()..]))?;
        let n: Vec<usize> = line
            .split(", ")
            .map(|part| part.split_whitespace().next()?.parse().ok())
            .collect::<Option<_>>()?;
        match n.as_slice() {
            [traces, calls, refusals, interleavings, skipped, split] => Some(Ran {
                traces: *traces,
                calls: *calls,
                refusals: *refusals,
                interleavings: *interleavings,
                skipped: *skipped,
                split: *split,
            }),
            _ => None,
        }
    }
}

/// The adapter file of `domain` in `options.lang`: its port's model adapter and harness.
/// It needs the port of the conformance traces (`options.conformance`).
pub fn generate(domain: &DomainModel, options: &super::Options) -> Result<String, CodegenError> {
    let a = decl(domain)?;
    let header = header_lines(options, &a);
    Ok(match options.lang {
        super::Lang::Rust => rust::print(&a, &header),
        super::Lang::Go => go::print(&a, &header, &options.package),
        super::Lang::TypeScript => ts::print(&a, &header),
    })
}

/// The adapter probe of `domain` in `lang`: the conformance test that drives the adapter
/// `binding` names along the traces. `None` without the port of the traces, or a binding
/// with no conformance test.
pub fn probe(
    lang: super::Lang,
    domain: &DomainModel,
    binding: &super::ProbeBinding,
) -> Option<super::probe::Probe> {
    decl(domain).ok()?;
    let (new, traces) = binding.conformance.as_ref()?;
    match lang {
        super::Lang::Rust => rust::probe(binding, new, traces),
        super::Lang::Go => go::probe(binding, new, traces),
        super::Lang::TypeScript => ts::probe(binding, new, traces),
    }
}

/// The comment lines atop the file: where it comes from, and what it is for.
fn header_lines(options: &super::Options, a: &Adapter) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Generated by rdra-ish {} codegen --adapter (experimental) from {}.",
            options.version, options.model_path
        ),
        "Code generation is outside rdra-ish's compatibility contract: a later release may"
            .to_string(),
        "generate different names or shapes from the same model. Generate again after an"
            .to_string(),
        "upgrade and compare.".to_string(),
        String::new(),
        "The model adapter of the conformance traces: plain data and one interface, with no"
            .to_string(),
        "type of the generated domain code. A service implements ModelAdapter over its own"
            .to_string(),
        "store and use cases, whatever its code looks like, and passes the harness below with"
            .to_string(),
        "the traces written with this file. The adapter must reach the real store (H3)."
            .to_string(),
    ];
    if !a.unchecked.is_empty() {
        lines.push(String::new());
        lines.push("The harness does not compare:".to_string());
        for u in &a.unchecked {
            lines.push(format!("- {u}"));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counts are read from the line wherever a test runner puts it (`go test -v` indents
    /// it after the file and line), and a line of another shape is no report.
    #[test]
    fn a_report_line_is_read_where_a_runner_puts_it() {
        let ran = Ran {
            traces: 9,
            calls: 19,
            refusals: 9,
            interleavings: 0,
            skipped: 7,
            split: 0,
        };
        let line = format!(
            "{REPORT}9 traces, 19 calls, 9 refusals, 0 interleavings, 7 interleavings skipped, 0 split calls"
        );
        assert_eq!(Ran::read(&line), Some(ran));
        assert_eq!(
            Ran::read(&format!(
                "=== RUN   T\n    probe_test.go:28: {line}\n--- PASS: T"
            )),
            Some(ran)
        );
        assert_eq!(Ran::read("rdra-ish conformance: ok 9 traces"), None);
        assert_eq!(Ran::read("test result: ok."), None);
    }
}
