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

use rdra_ish_core::lifecycle::{Domain, Ltl, TransitionSystem, Value};

pub use names::TlaNames;

/// What the cfg asks TLC to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrintMode {
    /// Check `TypeOK`, every invariant, every after-assert, and every temporal property.
    Check,
    /// No INVARIANT or PROPERTY lines, so `tlc -dump` sees the whole reachable set.
    Explore,
}

/// Files written by `export --kind tla` and `verify --backend tlc`.
#[derive(Debug, Clone)]
pub struct TlaBundle {
    pub module_name: String,
    pub tla: String,
    pub cfg: String,
    pub names: TlaNames,
}

pub fn print(ts: &TransitionSystem, mode: PrintMode, module_name: &str) -> TlaBundle {
    let module_name = sanitize_module_name(module_name);
    let names = TlaNames::new(ts);
    let tla = module::render(ts, &names, &module_name);
    let cfg = render_cfg(ts, &names, mode);
    TlaBundle {
        module_name,
        tla,
        cfg,
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

fn render_cfg(ts: &TransitionSystem, names: &TlaNames, mode: PrintMode) -> String {
    let mut out = String::from("SPECIFICATION Spec\n");
    if mode == PrintMode::Check {
        out.push_str("INVARIANT TypeOK\n");
        for i in 0..ts.invariants().len() {
            out.push_str(&format!("INVARIANT {}\n", names.invariant(i)));
        }
    }
    // Lifecycles end in states with no enabled action; that is not an error until phase 6.
    out.push_str("CHECK_DEADLOCK FALSE\n");
    if mode == PrintMode::Check {
        for i in 0..ts.after_asserts().len() {
            out.push_str(&format!("PROPERTY {}\n", names.after(i)));
        }
        for i in 0..ts.properties().len() {
            out.push_str(&format!("PROPERTY {}\n", names.property(i)));
        }
    }
    out
}

/// Fairness is needed only when a property asks for progress.
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

fn sanitize_module_name(s: &str) -> String {
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
