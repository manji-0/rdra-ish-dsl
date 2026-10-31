//! Migration hints for keywords and predicates removed from the DSL.

use crate::ast::{Expr, Operand, PredicateArg, PredicateCall, Span};

const CONCEPTUAL: &str = "conceptual-model kinds were removed; model persistent structure with `entity` (and `dto` for payloads)";

const HINTS: &[(&str, &str)] = &[
    (
        "usagescene",
        "use `timer` for a schedule that starts a use case, and `medium` / `.by(...)` for Business-BUC context",
    ),
    (
        "location",
        "no output consumed it. Delete the declaration and any `.where(...)` on `belongs`; keep `medium` / `.by(...)` for Business-BUC context",
    ),
    (
        "timing",
        "use `timer`; `belongs(...).when(...)` was removed, connect the timer with `triggers(Timer, UseCase)`",
    ),
    (
        "uses",
        "RDRA connects an external system to use cases through events: `raises(ExtSystem, Event)` for inbound, `triggers(Event, ExtSystem)` for outbound",
    ),
    (
        "state",
        "states are derived from the Enum column named in `transitions(Entity.col, ...)`; delete the declaration and list each state as a variant of the lifecycle Enum column",
    ),
    ("concept", CONCEPTUAL),
    ("domain_object", CONCEPTUAL),
    ("aggregate", CONCEPTUAL),
    ("value_object", CONCEPTUAL),
    ("valueobject", CONCEPTUAL),
    (
        "maps_to",
        "removed with the conceptual-model kinds; model persistent structure with `entity`",
    ),
];

/// What to do instead of a removed keyword or predicate, if `word` is one.
pub fn migration_hint(word: &str) -> Option<String> {
    HINTS
        .iter()
        .find(|(removed, _)| *removed == word)
        .map(|(removed, hint)| format!("`{removed}` is no longer supported; {hint}"))
}

/// The error for a removed top-level `when(...)` rule, naming the `invariant` to write: the
/// leading entity arguments, or else the entity of the first qualified column, become its
/// scope, and the conditions and chain carry over as written.
pub(crate) fn removed_when(call: &PredicateCall, src: &str) -> Option<String> {
    if call.name != "when" {
        return None;
    }
    let text = |span: Span| src.get(span).unwrap_or("").trim().to_string();
    let leading = call
        .args
        .iter()
        .take_while(|arg| matches!(arg, PredicateArg::Ref(_)))
        .count();
    let (scope, conds) = call.args.split_at(leading);
    let mut scope: Vec<String> = scope
        .iter()
        .filter_map(|arg| match arg {
            PredicateArg::Ref(qref) => Some(text(qref.span.clone())),
            _ => None,
        })
        .collect();
    if scope.is_empty() {
        scope.extend(conds.iter().find_map(first_entity).map(text));
    }
    let conds = match (conds.first(), conds.last()) {
        (Some(first), Some(last)) => text(arg_span(first).start..arg_span(last).end),
        _ => String::new(),
    };
    let chain: String = call.chain.iter().map(|cc| text(cc.span.clone())).collect();
    let entity = if scope.is_empty() {
        "Entity".to_string()
    } else {
        scope.join(", ")
    };
    Some(format!(
        "`when(...)` is no longer a rule; write `invariant({entity}).when({conds}){chain}`"
    ))
}

fn arg_span(arg: &PredicateArg) -> Span {
    match arg {
        PredicateArg::Ref(qref) => qref.span.clone(),
        PredicateArg::Expr(expr) => expr_span(expr),
        _ => 0..0,
    }
}

fn expr_span(expr: &Expr) -> Span {
    match expr {
        Expr::Cmp(cmp) => cmp.span.clone(),
        Expr::Not(inner) => expr_span(inner),
        Expr::And(a, b) | Expr::Or(a, b) => expr_span(a).start..expr_span(b).end,
    }
}

/// The span of the entity in the first `Entity.column` an argument names.
fn first_entity(arg: &PredicateArg) -> Option<Span> {
    fn in_expr(expr: &Expr) -> Option<Span> {
        match expr {
            Expr::Cmp(cmp) => [&cmp.lhs, &cmp.rhs].into_iter().find_map(|op| match op {
                Operand::QualifiedColumn(qcol) => Some(qcol.entity.span.clone()),
                _ => None,
            }),
            Expr::Not(inner) => in_expr(inner),
            Expr::And(a, b) | Expr::Or(a, b) => in_expr(a).or_else(|| in_expr(b)),
        }
    }
    match arg {
        PredicateArg::Expr(expr) => in_expr(expr),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::format_parse_error;
    use crate::parser::parse;

    fn messages(src: &str) -> Vec<String> {
        parse(src).1.iter().map(format_parse_error).collect()
    }

    #[test]
    fn usagescene_points_to_timer() {
        let msgs = messages("usagescene Counter \"Counter\"\n");
        assert!(
            msgs.iter()
                .any(|m| m.contains("`usagescene`") && m.contains("use `timer`")),
            "{msgs:?}"
        );
        assert!(!msgs.iter().any(|m| m.contains("`location`")), "{msgs:?}");
    }

    #[test]
    fn location_points_to_medium() {
        let msgs = messages("location FrontDesk \"Front Desk\"\n");
        assert!(
            msgs.iter()
                .any(|m| m.contains("`location`") && m.contains("`.where(...)`")),
            "{msgs:?}"
        );
    }

    #[test]
    fn timing_points_to_timer() {
        let msgs = messages("timing Nightly \"Nightly\"\n");
        assert!(
            msgs.iter().any(|m| m.contains("`timing`")
                && m.contains("use `timer`")
                && m.contains("triggers(Timer, UseCase)")),
            "{msgs:?}"
        );
    }

    #[test]
    fn uses_points_to_events() {
        let msgs = messages("uses(Staff, Lab)\n");
        assert!(
            msgs.iter().any(|m| m.contains("`uses`")
                && m.contains("raises(ExtSystem, Event)")
                && m.contains("triggers(Event, ExtSystem)")),
            "{msgs:?}"
        );
    }

    #[test]
    fn state_points_to_the_lifecycle_enum() {
        let msgs = messages("state Pending \"Order Received\"\n");
        assert!(
            msgs.iter()
                .any(|m| m.contains("`state`") && m.contains("variant of the lifecycle Enum")),
            "{msgs:?}"
        );
    }

    #[test]
    fn conceptual_kinds_point_to_entity() {
        for kw in [
            "concept",
            "domain_object",
            "aggregate",
            "value_object",
            "valueobject",
        ] {
            let msgs = messages(&format!("{kw} X \"X\"\n"));
            assert!(
                msgs.iter()
                    .any(|m| m.contains(&format!("`{kw}`")) && m.contains("`entity`")),
                "{kw}: {msgs:?}"
            );
        }
    }

    #[test]
    fn maps_to_points_to_removal_with_conceptual_kinds() {
        let msgs = messages("maps_to(A, B)\n");
        assert!(
            msgs.iter()
                .any(|m| m.contains("`maps_to`") && m.contains("conceptual-model kinds")),
            "{msgs:?}"
        );
    }

    #[test]
    fn a_top_level_when_names_its_invariant_rewrite() {
        let msgs =
            messages("when(Cert, status == revoked).none(Assign.status == active).label(\"x\")\n");
        assert_eq!(
            msgs,
            vec![
                "`when(...)` is no longer a rule; write \
                 `invariant(Cert).when(status == revoked).none(Assign.status == active).label(\"x\")`"
            ]
        );
    }

    #[test]
    fn a_top_level_when_takes_its_entity_from_the_first_qualified_column() {
        let msgs = messages(
            "when(ClientCertificate.status == revoked)\n  .none(TerminalCertAssignment.status == active)\n",
        );
        assert_eq!(
            msgs,
            vec![
                "`when(...)` is no longer a rule; write \
                 `invariant(ClientCertificate).when(ClientCertificate.status == revoked)\
                 .none(TerminalCertAssignment.status == active)`"
            ]
        );
    }

    #[test]
    fn a_top_level_when_does_not_load() {
        let (ast, _) = parse("when(Cert.status == revoked).none(Assign.status == active)\n");
        assert!(ast.items.is_empty(), "{:?}", ast.items);
    }

    #[test]
    fn removed_words_stay_usable_as_column_names() {
        let (_, errs) =
            parse("entity E \"E\" { condition: String\n  state: String\n  location: String\n  when: String }\n");
        assert!(errs.is_empty(), "{errs:?}");
    }
}
