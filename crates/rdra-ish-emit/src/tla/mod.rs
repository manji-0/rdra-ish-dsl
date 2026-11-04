//! TLA+ / TLC printer of a `TransitionSystem`. It reads the system and nothing else: the
//! semantics live in `rdra_ish_core::lifecycle`, and this module only spells them in TLA+.
//!
//! Every per-instance var is a function over `<E>_Ids == 1..InstanceCount`. A value prints as
//! TLC prints it back (`render_value`), so a `tlc -dump` state compares as text with a state
//! the explorer reached (`TlaNames::canon`).

mod module;
mod names;
mod pred;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use rdra_ish_core::lifecycle::{Domain, EntId, Ltl, TransitionSystem, Value};

pub use names::{Decoder, StateCheck, TlaNames};

/// What the cfg asks TLC to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrintMode {
    /// Check `TypeOK`, every invariant, every after-assert, and every temporal property.
    Check,
    /// `TypeOK`, invariants, and after-asserts: the checks a cone answers exactly.
    Safety,
    /// `TypeOK` and the temporal properties, which need the whole system's fairness.
    Temporal,
    /// `TypeOK` and `properties()[i]` alone: TLC does not say which property a temporal
    /// counterexample breaks, so `verify` checks each in its own run.
    Property(usize),
    /// `TypeOK` and `properties()[i]` alone on a cone, with no `WF_vars(Next)` in `Spec`: the
    /// whole model's `WF_vars(Next)` is met by steps outside the cone, so the cone assumes
    /// only the property's own `.fair`. An `ok` here is the whole model's verdict.
    ConeProperty(usize),
    /// No INVARIANT or PROPERTY lines, so `tlc -dump` sees the whole reachable set.
    Explore,
    /// `TypeOK` and the `Deadlock_` invariants of a `print_state_models` module.
    Deadlocks,
    /// `TypeOK` and the `Witness_` invariants of a `print_state_models` module, each violated
    /// where its state is reached.
    Witnesses,
}

/// Files written by `export --kind tla` and `verify --backend tlc`.
#[derive(Debug, Clone)]
pub struct TlaBundle {
    pub module_name: String,
    pub tla: String,
    pub cfg: String,
    /// The `Witnesses` cfg of a `print_state_models` module.
    pub witness_cfg: Option<String>,
    pub names: TlaNames,
}

pub fn print(ts: &TransitionSystem, mode: PrintMode, module_name: &str) -> TlaBundle {
    let module_name = module_name_of(module_name);
    let names = TlaNames::new(ts);
    let wf_next = !matches!(mode, PrintMode::ConeProperty(_)) && needs_fairness(ts);
    let tla = module::render(ts, &names, &module_name, wf_next);
    let cfg = cfg_without(ts, &names, mode, &BTreeSet::new());
    TlaBundle {
        module_name,
        tla,
        cfg,
        witness_cfg: None,
        names,
    }
}

/// The module of `ts`, an entity cone, with the state-model checks of `entities`: `cfg` is
/// the `Deadlocks` cfg and `witness_cfg` the `Witnesses` one. TLC on the cone reaches the
/// states BFS reaches, so the checks report what `state_model_findings` reports, except a
/// deadlock while the column's cone keeps moving forever (docs/formal-verification.md,
/// "State models with TLC alone").
pub fn print_state_models(
    ts: &TransitionSystem,
    entities: &[EntId],
    module_name: &str,
) -> TlaBundle {
    let module_name = module_name_of(module_name);
    let names = TlaNames::with_state_models(ts, entities);
    let tla = module::render(ts, &names, &module_name, false);
    let none = BTreeSet::new();
    TlaBundle {
        cfg: cfg_without(ts, &names, PrintMode::Deadlocks, &none),
        witness_cfg: Some(cfg_without(ts, &names, PrintMode::Witnesses, &none)),
        module_name,
        tla,
        names,
    }
}

/// A value as TLA+ text, which is also how TLC prints it.
pub fn render_value(domain: &Domain, value: Value) -> String {
    let i = usize::from(value.0);
    match domain {
        Domain::Existence => quote(["absent", "present", "deleted"][i]),
        Domain::Enum(values) => quote(&values[i]),
        Domain::Bool => if value == Value::TRUE {
            "TRUE"
        } else {
            "FALSE"
        }
        .to_string(),
        Domain::Nullable => quote(if value == Value::NULL {
            "null"
        } else {
            "present"
        }),
        Domain::Int => i.to_string(),
        Domain::NullableInt => match i {
            0 => NULL_INT.to_string(),
            n => (n - 1).to_string(),
        },
        // A parent slot `s` is TLA index `s + 1`; an unlinked optional owner is `0`.
        Domain::Instance { optional: true, .. } => i.to_string(),
        Domain::Instance {
            optional: false, ..
        } => (i + 1).to_string(),
    }
}

/// The null of a nullable Int column: outside `IntRange`, so TLC never compares a string
/// with an integer.
const NULL_INT: &str = "-1";

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The set of a domain, as TypeOK and free initial choices read it.
fn domain_set(ts: &TransitionSystem, names: &TlaNames, domain: &Domain) -> String {
    let values = |n: usize| -> String {
        let items: Vec<String> = (0..n)
            .map(|v| render_value(domain, Value(v as u8)))
            .collect();
        format!("{{{}}}", items.join(", "))
    };
    match domain {
        Domain::Existence | Domain::Enum(_) | Domain::Nullable => values(domain.size(ts.bounds)),
        Domain::Bool => "BOOLEAN".to_string(),
        Domain::Int => "IntRange".to_string(),
        Domain::NullableInt => format!("IntRange \\cup {{{NULL_INT}}}"),
        Domain::Instance { parent, optional } => {
            let ids = names.ids(*parent);
            if *optional {
                format!("{ids} \\cup {{0}}")
            } else {
                ids.to_string()
            }
        }
    }
}

/// The cfg of `mode` without the invariants and after-asserts `skip` names. With `-continue`
/// TLC reports one violated check per state or step, the first in cfg order, so a check
/// violated only where an earlier one is shows once the earlier one is dropped: `verify`
/// reruns a cone without the checks it has reported until a run reports no new one.
pub fn cfg_without(
    ts: &TransitionSystem,
    names: &TlaNames,
    mode: PrintMode,
    skip: &BTreeSet<String>,
) -> String {
    let all = 0..ts.properties().len();
    let (safety, properties) = match mode {
        PrintMode::Check => (true, all),
        PrintMode::Safety => (true, 0..0),
        PrintMode::Temporal => (false, all),
        PrintMode::Property(i) | PrintMode::ConeProperty(i) => (false, i..i + 1),
        PrintMode::Explore => (false, 0..0),
        PrintMode::Deadlocks | PrintMode::Witnesses => return state_cfg(names, mode, skip),
    };
    let mut out = String::from("SPECIFICATION Spec\n");
    if mode != PrintMode::Explore {
        out.push_str("INVARIANT TypeOK\n");
    }
    if safety {
        for i in 0..ts.invariants().len() {
            let name = names.invariant(i);
            if !skip.contains(name) {
                out.push_str(&format!("INVARIANT {name}\n"));
            }
        }
    }
    // TLC's deadlock check stays off. Every bounded model ends with no enabled action, since a
    // deleted slot is never reused; a cone spec drops actions; and an always-enabled loop
    // hides an instance that is stuck. `state_model_findings` reports a `Deadlock` per
    // instance on the BFS cone graph instead.
    out.push_str("CHECK_DEADLOCK FALSE\n");
    if safety {
        for i in 0..ts.after_asserts().len() {
            let name = names.after(i);
            if !skip.contains(name) {
                out.push_str(&format!("PROPERTY {name}\n"));
            }
        }
    }
    for i in properties {
        out.push_str(&format!("PROPERTY {}\n", names.property(i)));
    }
    out
}

/// The cfg of a `print_state_models` module, without the checks `skip` names.
fn state_cfg(names: &TlaNames, mode: PrintMode, skip: &BTreeSet<String>) -> String {
    let (head, checks): (&str, Vec<&str>) = match mode {
        PrintMode::Deadlocks => (
            "\\* A Deadlock_<E>_<col>_<s> TLC reports violated is that deadlock: an instance at s,\n\
             \\* which has a way out, in a state with no move left. A pass does not prove there is\n\
             \\* none: a cycle anywhere in the column's cone hides it. rdra-ish verify decides.\n",
            names.states().iter().filter_map(|s| s.deadlock.as_deref()).collect(),
        ),
        _ => (
            "\\* Each Witness_<E>_<col>_<s> says no instance is ever at s, so TLC reports it violated\n\
             \\* where s is reached: a healthy model violates every one. A Witness_ TLC does not\n\
             \\* report is a state with no witness. -continue prints a trace for each violating\n\
             \\* state; on a large cone rerun without it, which reports one invariant per run.\n",
            names.states().iter().map(|s| s.witness.as_str()).collect(),
        ),
    };
    let rerun = "\\* With -continue TLC reports one invariant per state, the first here: rerun\n\
                 \\* without the reported ones until a run reports no new one.\n";
    let mut out = format!("{head}{rerun}SPECIFICATION Spec\nINVARIANT TypeOK\n");
    for name in checks.into_iter().filter(|n| !skip.contains(*n)) {
        out.push_str(&format!("INVARIANT {name}\n"));
    }
    out.push_str("CHECK_DEADLOCK FALSE\n");
    out
}

/// `WF_vars(Next)` is needed only when a property asks for progress.
fn needs_fairness(ts: &TransitionSystem) -> bool {
    ts.properties()
        .iter()
        .any(|p| matches!(p.formula, Ltl::Eventually(_) | Ltl::LeadsTo(..)))
}

pub(crate) fn sanitize_ident(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        out.push('X');
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, 'X');
    }
    out
}

/// The module name `print` gives `s`: without a `.tla` or `.cfg` suffix, as an identifier.
pub fn module_name_of(s: &str) -> String {
    let stem = s.trim();
    let stem = stem
        .strip_suffix(".tla")
        .or_else(|| stem.strip_suffix(".cfg"))
        .unwrap_or(stem);
    if stem.is_empty() {
        "RdraSpec".to_string()
    } else {
        sanitize_ident(stem)
    }
}
