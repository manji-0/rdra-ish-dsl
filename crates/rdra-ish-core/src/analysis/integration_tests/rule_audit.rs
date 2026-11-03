//! Every form of every constraint-forming predicate either records a rule or fails to load.
//! A form that does neither would let a model break the rule with nothing reported.

use crate::analysis::build_model;
use crate::model::SemanticModel;
use crate::predicate::{predicate_chain_methods, KNOWN_PREDICATES};
use crate::rules::business_rules;
use rdra_ish_syntax::parse;

const BASE: &str = r#"
variation Kind "Kind" {
  a "A"
  b "B"
}
entity Order "Order" { id: Int @pk  status: Enum(open, paid) @default(open)  total: Int  kind: Kind  rush: Bool }
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
    (
        "invariant(Order).when(status == \"paid\").then(total > 0)",
        Error,
    ),
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
    // a bare entity: no linked row at all (#73)
    ("invariant(Order).when(status == paid).none(Ticket)", Recorded),
    ("invariant(Order).when(status == paid).has(Ticket)", Recorded),
    ("invariant(Order).when(status == paid).none(Ticket).by(order)", Recorded),
    ("invariant(Order).when(status == paid).none(Ticket).by(nosuch)", Error),
    ("invariant(Order).when(status == paid).none(Nosuch)", Error),
    ("invariant(Order).by(order).when(status == paid).none(Ticket)", Error),
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
    ("holds(Ready, Order.status == \"paid\")", Error),
    ("holds(Ready, Order.status in (\"paid\", open))", Error),
    ("holds(Ready, Order.kind not in (\"a\"))", Error),
    ("holds(Ready, Order.status == paid).none()", Error),
    ("holds(Ready, Order.status == paid).none(Ticket)", Recorded),
    ("holds(Ready, Order.status == paid).none(Ticket).by(order)", Recorded),
    ("holds(Ready, Ticket.status == done).by(order)", Recorded),
    ("holds(Ready, Ticket.status == done).by(nosuch)", Error),
    // maps_field
    ("maps_field(F, Order.status)", Recorded),
    ("maps_field(F, Order.nosuch)", Error),
    ("maps_field(F, Nosuch.status)", Error),
    ("maps_field(F, status)", Error),
    ("maps_field(F, Order, status)", Error), // the removed three-argument form
    // sets: a column effect, its column written `Entity.column`
    ("sets(Pay, Order.status == paid)", Recorded),
    ("sets(Pay, Order.status == \"paid\")", Error),
    ("sets(Pay, Order.kind == \"a\")", Error),
    ("sets(Pay, Order.status != paid)", Error),
    ("sets(Pay, Order.total < 3)", Error),
    ("sets(Pay, status == paid)", Error),
    ("sets(Pay, Nosuch.status == paid)", Error),
    ("sets(Pay, Order.status)", Error),
    ("sets(Pay, Order)", Error),
    ("sets(Pay, Order.status == paid, Order)", Error),
    // the removed forms that name the entity apart from the column
    ("sets(Pay, Order, status == paid)", Error),
    ("sets(Pay, Order, total < 3, true)", Error),
    ("drives(Pay, Order, total < 3, true)", Error),
    // drives: a comparison driven true or false; `== value, true` sets the value
    ("drives(Pay, Order.total < 3, true)", Recorded),
    ("drives(Pay, Order.total < 3, false)", Recorded),
    ("drives(Pay, Order.status == paid, true)", Recorded),
    ("drives(Pay, Order.status == paid, false)", Error),
    ("drives(Pay, Order.total < 3, maybe)", Error),
    ("drives(Pay, Order.total < 3)", Error),
    ("drives(Pay, Order.status, true)", Error),
    ("drives(Pay, total < 3, true)", Error),
    ("drives(Pay, Order.total < nosuch, true)", Error),
    ("drives(Pay, Order)", Error),
    // a use case leaves one value, so a set membership is an error in sets and drives
    ("sets(Pay, Order.status in (open, paid))", Error),
    ("sets(Pay, Order.status not in (open))", Error),
    ("sets(Pay, Order.status in (paid))", Error),
    ("drives(Pay, Order.status in (paid), true)", Error),
    ("drives(Pay, Order.status not in (paid), false)", Error),
    // set membership: wherever a comparison is a condition
    ("forbidden(Order, status in (open, paid))", Recorded),
    (
        "forbidden(Order, status not in (open), total in (1, 2))",
        Recorded,
    ),
    ("forbidden(Order, status in ())", Error),
    ("forbidden(Order, status in (open, open))", Error),
    ("forbidden(Order, status in (closed))", Error),
    (
        "forbidden(Order, Ticket, Order.status in (paid), Ticket.status not in (done))",
        Recorded,
    ),
    ("required(Order, status not in (open))", Recorded),
    ("exclusive(Order, status in (paid), total in (3))", Recorded),
    (
        "invariant(Order).when(status in (open, paid)).then(total not in (0))",
        Recorded,
    ),
    (
        "invariant(Order).when(status in (paid)).none(Ticket.status not in (done))",
        Recorded,
    ),
    (
        "invariant(Order).when(status not in (open)).has(Ticket.status in (done))",
        Recorded,
    ),
    (
        "invariant(Order, Ticket).when(Order.status in (paid)).then(Ticket.status in (done))",
        Recorded,
    ),
    ("after(Pay).assert(Order.status in (paid))", Recorded),
    ("holds(Ready, Order.status not in (open, paid))", Recorded),
    (
        "holds(Ready, Order.status in (paid)).none(Ticket.status in (pending))",
        Recorded,
    ),
    // chooses: the actor picks a variation column's value again; nothing else is a choice
    ("chooses(Pay, Order.kind)", Recorded),
    ("chooses(Pay, Order.status)", Error),
    ("chooses(Pay, Order.rush)", Error),
    ("chooses(Pay, Order.total)", Error),
    ("chooses(Pay, Order.nosuch)", Error),
    ("chooses(Pay, Nosuch.kind)", Error),
    ("chooses(Pay, kind)", Error),
    ("chooses(Pay, Order, kind)", Error),
    ("chooses(Pay)", Error),
    ("chooses(Order.kind, Pay)", Error),
    ("chooses(Pay, Order.kind).when(Ready)", Error),
    ("sets(Pay, Order.kind == a)\nchooses(Pay, Order.kind)", Error),
    // chooses with values: each line lists one value, a `.when(Cond)` guards it (#65)
    ("chooses(Pay, Order.kind, a)", Recorded),
    ("chooses(Pay, Order.kind, a)\nchooses(Pay, Order.kind, b)", Recorded),
    ("chooses(Pay, Order.kind, a)\nchooses(Pay, Order.kind, a)", Recorded),
    (
        "holds(Ready, Ticket.status == done)\nchooses(Pay, Order.kind, a).when(Ready)\nchooses(Pay, Order.kind, b)",
        Recorded,
    ),
    (
        "chooses(Pay, Order.kind, a).when(Ready)\nholds(Ready, Ticket.status == done)",
        Recorded,
    ),
    (
        "holds(Ready, Ticket.status == done)\nchooses(Pay, Order.kind, a).when(Ready)\nchooses(Pay, Order.kind, a)",
        Recorded,
    ),
    ("sets(Pay, Order.kind == a)\nchooses(Pay, Order.kind, b)", Error),
    ("chooses(Pay, Order.kind, c)", Error),
    ("chooses(Pay, Order.kind, A)", Error),
    // A value is a bare token; a quoted string is free text (#48).
    ("chooses(Pay, Order.kind, \"a\")", Error),
    ("chooses(Pay, Order.kind, Order.kind)", Error),
    ("chooses(Pay, Order.kind, a, b)", Error),
    ("chooses(Pay, Order.status, open)", Error),
    ("chooses(Pay, Order.kind)\nchooses(Pay, Order.kind, a)", Error),
    ("chooses(Pay, Order.kind, a)\nchooses(Pay, Order.kind)", Error),
    ("chooses(Pay, Order.kind, a).when(Ready)", Error),
    ("chooses(Pay, Order.kind, a).when(Nosuch)", Error),
    ("chooses(Pay, Order.kind, a).when(Order)", Error),
    ("chooses(Pay, Order.kind, a).when()", Error),
    (
        "holds(Ready, Ticket.status == done)\nchooses(Pay, Order.kind, a).when(Ready, Ready)",
        Error,
    ),
    (
        "holds(Ready, Ticket.status == done)\nchooses(Pay, Order.kind, a).when(Ready).when(Ready)",
        Error,
    ),
    (
        "holds(Ready, Ticket.status == done)\nchooses(Pay, Order.kind, a).when(Ticket.status == done)",
        Error,
    ),
    (
        "holds(Ready, Ticket.status == done)\nchooses(Pay, Order.kind).when(Ready)",
        Error,
    ),
    (
        "holds(Ready, Ticket.status == done)\nchooses(Pay, Order.kind, a).unless(Ready)",
        Error,
    ),
    // transitions: a `.when(Cond)` arm guard is a rule, so its condition must have a body
    ("transitions(Pay, Order.status, open -> paid)", Recorded),
    ("transitions(Pay, Order.status, \"open\" -> paid)", Error),
    ("transitions(Pay, Order.status, open -> \"paid\")", Error),
    (
        "holds(Ready, Ticket.status == done)\ntransitions(Pay, Order.status, open -> paid).when(Ready)",
        Recorded,
    ),
    (
        "transitions(Pay, Order.status, open -> paid).when(Ready)\nholds(Ready, Ticket.status == done)",
        Recorded,
    ),
    ("transitions(Pay, Order.status, open -> paid).when(Ready)", Error),
    ("transitions(Pay, Order.status, open -> paid).when(Nosuch)", Error),
    ("transitions(Pay, Order.status, open -> paid).when(Order)", Error),
    ("transitions(Pay, Order.status, open -> paid).when()", Error),
    (
        "holds(Ready, Ticket.status == done)\ntransitions(Pay, Order.status, open -> paid).when(Ready, Ready)",
        Error,
    ),
    (
        "holds(Ready, Ticket.status == done)\ntransitions(Pay, Order.status, open -> paid).when(Ready).when(Ready)",
        Error,
    ),
    (
        "holds(Ready, Ticket.status == done)\ntransitions(Pay, Order.status, open -> paid).when(Ticket.status == done)",
        Error,
    ),
    (
        "holds(Ready, Ticket.status == done)\ntransitions(Pay, Order.status, open -> paid).unless(Ready)",
        Error,
    ),
    // property: a temporal formula records the property and each `.fair` use case; fairness
    // only weakens a progress claim, so it is an error on `always`, where it changes nothing
    ("property P always(Order.status != paid)", Recorded),
    ("property P eventually(Order.status == paid)", Recorded),
    (
        "property P leads_to(Order.status == open, Order.status == paid)",
        Recorded,
    ),
    (
        "property P leads_to(Order.status == open, Order.status == paid).fair(Pay)",
        Recorded,
    ),
    (
        "usecase Ship \"Ship\"\nproperty P leads_to(Order.status == open, Order.status == paid).fair(Pay, Ship)",
        Recorded,
    ),
    ("property P eventually(Order.status == paid).fair(Pay)", Recorded),
    ("property P always(Order.status != paid).fair(Pay)", Error),
    (
        "property P leads_to(Order.status == open, Order.status == paid).fair(Nosuch)",
        Error,
    ),
    (
        "property P leads_to(Order.status == open, Order.status == paid).fair(Order)",
        Error,
    ),
    (
        "property P leads_to(Order.status == open, Order.status == paid).fair(Order.status == paid)",
        Error,
    ),
    (
        "property P leads_to(Order.status == open, Order.status == paid).fair()",
        Error,
    ),
    (
        "property P leads_to(Order.status == open, Order.status == paid).fair(Pay, Pay)",
        Error,
    ),
    (
        "property P leads_to(Order.status == open, Order.status == paid).fair(Pay).fair(Pay)",
        Error,
    ),
    (
        "property P leads_to(Order.status == open, Order.status == paid).when(Ready)",
        Error,
    ),
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
    let arm_guards = model
        .state_transitions
        .iter()
        .filter(|t| t.guard.is_some())
        .count();
    // The value-less `chooses` records the choice, the value form each listed value and guard.
    let choices: usize = model
        .column_choices
        .iter()
        .map(|c| c.values.len().max(1) + c.values.iter().filter(|v| v.guard.is_some()).count())
        .sum();
    business_rules(model).len()
        + arm_guards
        + model.field_mappings.len()
        + model.column_effects.len()
        + choices
        + model.proposition_effects.len()
        + bodies
        + model
            .temporal_properties
            .iter()
            .map(|p| 1 + p.fair.len())
            .sum::<usize>()
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

/// The parts a form records, line by line: one per `.has/.none(...)`, plus the rule or body
/// itself unless the form is only a quantifier; a `transitions` arm records its `.when` guard,
/// a `chooses` line its choice or value and its `.when` guard, a `property` itself and each
/// use case its `.fair` lists.
fn parts(form: &str) -> usize {
    form.lines().map(line_parts).sum()
}

fn line_parts(line: &str) -> usize {
    if line.starts_with("usecase ") {
        return 0;
    }
    if line.starts_with("property ") {
        let fair = line
            .split_once(".fair(")
            .map_or(0, |(_, rest)| rest.trim_end_matches(')').split(',').count());
        return 1 + fair;
    }
    if line.starts_with("transitions(") {
        return line.matches(".when(").count();
    }
    if line.starts_with("chooses(") {
        return 1 + line.matches(".when(").count();
    }
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
    for pred in labelled.chain(&[
        "holds",
        "maps_field",
        "sets",
        "drives",
        "chooses",
        "transitions",
    ]) {
        assert!(
            FORMS
                .iter()
                .any(|(line, _)| line.starts_with(&format!("{pred}("))),
            "no audit form for `{pred}`"
        );
    }
}
