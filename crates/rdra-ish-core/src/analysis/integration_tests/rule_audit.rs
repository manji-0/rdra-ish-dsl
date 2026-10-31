//! Every form of every constraint-forming predicate either records a rule or fails to load.
//! A form that does neither would let a model break the rule with nothing reported.

use crate::analysis::build_model;
use crate::model::SemanticModel;
use crate::predicate::{predicate_chain_methods, KNOWN_PREDICATES};
use crate::rules::business_rules;
use rdra_ish_syntax::parse;

const BASE: &str = r#"
entity Order "Order" { id: Int @pk  status: Enum(open, paid) @default(open)  total: Int }
entity Ticket "Ticket" { id: Int @pk  status: Enum(pending, done) @default(pending) }
relate(Ticket, Order, N:1)
usecase Pay "Pay"
field F "F"
condition Ready "Ready"
"#;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Expect {
    /// The form loads and records each of its parts: the rule (for `maps_field`, the field
    /// mapping) and each quantifier.
    Recorded,
    /// The form fails to load with an error.
    Error,
}
use Expect::*;

/// One line per form. Every predicate a rule can be written with appears here.
const FORMS: &[(&str, Expect)] = &[
    // forbidden
    ("forbidden(Order, status == paid)", Recorded),
    ("forbidden(Order, status == paid, total > 3)", Recorded),
    ("forbidden(Order)", Error),
    (
        "forbidden(Order, Ticket, Order.status == paid, Ticket.status == pending)",
        Recorded,
    ),
    (
        "forbidden(Order.status == paid, Ticket.status == pending)",
        Recorded,
    ),
    (
        "forbidden(Order.status == paid, Ticket.status == pending).along(Ticket, Order)",
        Recorded,
    ),
    // required
    ("required(Order, total >= 0)", Recorded),
    ("required(Order)", Error),
    // exclusive
    ("exclusive(Order, status == paid, total > 3)", Recorded),
    ("exclusive(Order, status == paid)", Error),
    ("exclusive(Order)", Error),
    // invariant, one entity
    (
        "invariant(Order).when(status == paid).then(total > 0)",
        Recorded,
    ),
    ("invariant(Order).when(status == paid)", Error),
    ("invariant(Order).then(total > 0)", Error),
    ("invariant(Order)", Error),
    // invariant, one entity, quantified over related rows: the scope is inferred
    (
        "invariant(Order).when(status == paid).none(Ticket.status == pending)",
        Recorded,
    ),
    (
        "invariant(Order).when(Order.status == paid).has(Ticket.status == done)",
        Recorded,
    ),
    ("invariant(Order).none(Ticket.status == pending)", Recorded),
    ("invariant(Order).when(status == paid).none()", Error),
    ("invariant(Order).when(status == paid).none(Ticket)", Error),
    (
        "invariant(Order).when(status == paid).none(status == open)",
        Error,
    ),
    // invariant, several entities
    (
        "invariant(Order, Ticket).when(Order.status == paid).then(Ticket.status == done)",
        Recorded,
    ),
    (
        "invariant(Order, Ticket).when(Order.status == paid).none(Ticket.status == pending)",
        Recorded,
    ),
    (
        "invariant().when(Order.status == paid).none(Ticket.status == pending)",
        Recorded,
    ),
    (
        "invariant().when(Order.status == paid).then(Ticket.status == done)",
        Recorded,
    ),
    ("invariant(Order, Ticket).when(Order.status == paid)", Error),
    (
        "invariant(Order, Ticket).then(Ticket.status == done)",
        Error,
    ),
    ("invariant().none(Ticket.status == pending)", Error),
    // top-level when: removed, the message names the invariant to write
    (
        "when(Order, status == paid).none(Ticket.status == pending)",
        Error,
    ),
    (
        "when(Order.status == paid).none(Ticket.status == pending)",
        Error,
    ),
    // after
    ("after(Pay).assert(Order.status == paid)", Recorded),
    ("after(Pay)", Error),
    // holds: the body is the rule; its quantifier must not vanish either
    ("holds(Ready, Order.status == paid)", Recorded),
    (
        "holds(Ready, Order.status == paid).none(Ticket.status == pending)",
        Recorded,
    ),
    ("holds(Ready, Order.status == paid).none()", Error),
    ("holds(Ready, Order.status == paid).none(Ticket)", Error),
    // maps_field
    ("maps_field(F, Order.status)", Recorded),
    ("maps_field(F, Order.nosuch)", Error),
    ("maps_field(F, Nosuch.status)", Error),
    ("maps_field(F, status)", Error),
    ("maps_field(F, Order, status)", Error), // the removed three-argument form
    // sets: a column effect, or a comparison driven true or false
    ("sets(Pay, Order, status == paid)", Recorded),
    ("sets(Pay, Order, status == paid, true)", Recorded),
    ("sets(Pay, Order, total < 3, true)", Recorded),
    ("sets(Pay, Order, total < 3, false)", Recorded),
    ("sets(Pay, Order, status == paid, false)", Error),
    ("sets(Pay, Order, status != paid)", Error),
    ("sets(Pay, Order, total < 3)", Error),
    ("sets(Pay, Order, total < 3, maybe)", Error),
    ("sets(Pay, Order, status)", Error),
    ("sets(Pay, Order)", Error),
];

/// The rules a model records, its field mappings, its `sets` effects, and each condition body
/// and its quantifiers.
fn recorded(model: &SemanticModel) -> usize {
    let bodies: usize = model
        .conditions
        .values()
        .filter_map(|c| c.body.as_ref())
        .map(|body| 1 + body.quantifiers.len())
        .sum();
    business_rules(model).len()
        + model.field_mappings.len()
        + model.column_effects.len()
        + model.proposition_effects.len()
        + bodies
}

fn outcome(line: &str) -> (Expect, String) {
    let (ast, parse_errors) = parse(&format!("{BASE}{line}\n"));
    if !parse_errors.is_empty() {
        return (Error, format!("{parse_errors:?}"));
    }
    let (model, diags) = build_model(&ast);
    let errors: Vec<String> = diags
        .iter()
        .filter(|d| !d.is_warning)
        .map(|d| d.error.to_string())
        .collect();
    if !errors.is_empty() {
        return (Error, errors.join(" | "));
    }
    let (base_ast, _) = parse(BASE);
    let (base, _) = build_model(&base_ast);
    let (got, want) = (recorded(&model) - recorded(&base), parts(line));
    if got == want {
        (Recorded, String::new())
    } else {
        (
            Error,
            format!("SILENTLY DROPPED: {got} of {want} parts recorded, and no error"),
        )
    }
}

/// The parts a form records: one per `.has/.none(...)`, plus the rule or body itself unless the
/// form is only a quantifier.
fn parts(line: &str) -> usize {
    let quantifiers = line.matches(".has(").count() + line.matches(".none(").count();
    let only_quantifier =
        quantifiers > 0 && !line.starts_with("holds(") && !line.contains(".then(");
    quantifiers + usize::from(!only_quantifier)
}

#[test]
fn every_rule_form_records_a_rule_or_fails_to_load() {
    let mut wrong = Vec::new();
    for (line, want) in FORMS {
        let (got, detail) = outcome(line);
        let dropped = detail.starts_with("SILENTLY DROPPED");
        if got != *want || dropped {
            wrong.push(format!("{line}\n    want {want:?}, got {got:?}: {detail}"));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

#[test]
fn every_rule_predicate_has_a_form_in_the_audit() {
    let labelled = KNOWN_PREDICATES
        .iter()
        .filter(|p| predicate_chain_methods(p).contains(&"label"));
    for pred in labelled.chain(&["holds", "maps_field", "sets"]) {
        assert!(
            FORMS
                .iter()
                .any(|(line, _)| line.starts_with(&format!("{pred}("))),
            "no audit form for `{pred}`"
        );
    }
}
