//! Legacy forms of the DSL: what each one is rewritten to, and where it is in its life.
//!
//! [`LEGACY_FORMS`] is the single list. A form is first [`Status::Deprecated`]: it still
//! loads, rewritten to the current form in the AST (so `fmt` prints the new form), with a
//! warning that names the rewrite and the version it is removed in. Later it is
//! [`Status::Removed`]: a load error naming the same rewrite. Only a [`Detect::Predicate`]
//! form can be deprecated, since only a rewrite function keeps it loading.
//!
//! To deprecate a predicate form, add one row with `Detect::Predicate(rewrite_fn)` and
//! `Status::Deprecated { .. }`, where `rewrite_fn` returns the call in its current form and
//! a message ending in "write `<new form>`", or `None` when the call is not in the legacy
//! form. To remove it, change the row's status to `Status::Removed`.

use crate::ast::{Expr, Operand, PredicateArg, PredicateCall, Span};
use crate::format::format_predicate;

/// Where a legacy form is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Loads, rewritten to the current form, with a warning. Deprecated in a minor release,
    /// removed no earlier than the next major one.
    Deprecated {
        since: &'static str,
        removed_in: &'static str,
    },
    /// A load error naming the rewrite.
    Removed { since: &'static str },
}

/// A rewrite of a predicate call: the call in its current form and the message naming it,
/// or `None` when the call is not in the legacy form.
pub type Rewrite = fn(&PredicateCall) -> Option<(PredicateCall, String)>;

/// How the parser meets a legacy form.
#[derive(Clone, Copy)]
pub enum Detect {
    /// A removed keyword or predicate name: the row's `id` is the word.
    Keyword,
    /// A call of the named predicate, rewritten by the function.
    Predicate(&'static str, Rewrite),
    /// Matched where it occurs in the grammar (the top-level `when(...)` rule, the two-word
    /// `acceptance criteria`); the row documents it.
    Grammar,
}

/// One legacy form.
#[derive(Clone, Copy)]
pub struct LegacyForm {
    /// The keyword for a [`Detect::Keyword`] row; a stable name otherwise.
    pub id: &'static str,
    /// What to write instead. For a keyword, the message after "`<id>` is no longer supported; ".
    pub instead: &'static str,
    pub status: Status,
    pub detect: Detect,
}

const CONCEPTUAL: &str = "conceptual-model kinds were removed; model persistent structure with `entity` (and `dto` for payloads)";

const fn removed_word(id: &'static str, since: &'static str, instead: &'static str) -> LegacyForm {
    LegacyForm {
        id,
        instead,
        status: Status::Removed { since },
        detect: Detect::Keyword,
    }
}

/// Every legacy form, deprecated or removed.
pub const LEGACY_FORMS: &[LegacyForm] = &[
    removed_word(
        "usagescene",
        "0.3.0",
        "use `timer` for a schedule that starts a use case, and `medium` / `.by(...)` for Business-BUC context",
    ),
    removed_word(
        "location",
        "0.3.0",
        "no output consumed it. Delete the declaration and any `.where(...)` on `belongs`; keep `medium` / `.by(...)` for Business-BUC context",
    ),
    removed_word(
        "timing",
        "0.3.0",
        "use `timer`; `belongs(...).when(...)` was removed, connect the timer with `triggers(Timer, UseCase)`",
    ),
    removed_word(
        "uses",
        "0.3.0",
        "RDRA connects an external system to use cases through events: `raises(ExtSystem, Event)` for inbound, `triggers(Event, ExtSystem)` for outbound",
    ),
    removed_word(
        "state",
        "0.3.0",
        "states are derived from the Enum column named in `transitions(UseCase, Entity.col, ...)`; delete the declaration and list each state as a variant of the lifecycle Enum column",
    ),
    removed_word("concept", "0.3.0", CONCEPTUAL),
    removed_word("domain_object", "0.3.0", CONCEPTUAL),
    removed_word("aggregate", "0.3.0", CONCEPTUAL),
    removed_word("value_object", "0.3.0", CONCEPTUAL),
    removed_word("valueobject", "0.3.0", CONCEPTUAL),
    removed_word(
        "maps_to",
        "0.3.0",
        "removed with the conceptual-model kinds; model persistent structure with `entity`",
    ),
    removed_word(
        "writes",
        "0.4.0",
        "use `updates` (or `creates`/`deletes`); `scripts/migrate-syntax-v2.py` rewrites it to `updates`",
    ),
    LegacyForm {
        id: "transitions-column-first",
        instead: "`transitions(UseCase, Entity.column, from -> to)`",
        status: Status::Removed { since: "0.4.0" },
        detect: Detect::Predicate("transitions", legacy_transitions),
    },
    LegacyForm {
        id: "maps_field-three-arguments",
        instead: "`maps_field(Field, Entity.column)`",
        status: Status::Removed { since: "0.4.0" },
        detect: Detect::Predicate("maps_field", legacy_maps_field),
    },
    LegacyForm {
        id: "top-level-when",
        instead: "`invariant(Entity).when(conditions...).none(...)`",
        status: Status::Removed { since: "0.4.0" },
        detect: Detect::Grammar,
    },
    LegacyForm {
        id: "acceptance-criteria-two-words",
        instead: "`acceptance_criteria \"...\"`",
        status: Status::Removed { since: "0.4.0" },
        detect: Detect::Grammar,
    },
    LegacyForm {
        id: "sets-four-arguments",
        instead: "`drives(UseCase, Entity, comparison, true|false)`",
        status: Status::Deprecated {
            since: "0.5.0",
            removed_in: "2.0.0",
        },
        detect: Detect::Predicate("sets", legacy_sets_truth),
    },
];

/// What to do instead of a removed keyword or predicate, if `word` is one.
pub fn migration_hint(word: &str) -> Option<String> {
    LEGACY_FORMS
        .iter()
        .find(|form| matches!(form.detect, Detect::Keyword) && form.id == word)
        .map(|form| format!("`{}` is no longer supported; {}", form.id, form.instead))
}

/// A predicate call written in a legacy form, rewritten to its current form.
pub(crate) struct LegacyCall {
    pub rewritten: PredicateCall,
    /// For a removed form the error; for a deprecated one the warning.
    pub message: String,
    pub deprecated: bool,
}

/// The legacy form `pred` is written in, if any, rewritten to its current form.
pub(crate) fn legacy_predicate(pred: &PredicateCall) -> Option<LegacyCall> {
    LEGACY_FORMS.iter().find_map(|form| {
        let Detect::Predicate(name, rewrite) = form.detect else {
            return None;
        };
        if name != pred.name {
            return None;
        }
        let (rewritten, message) = rewrite(pred)?;
        Some(match form.status {
            Status::Removed { .. } => LegacyCall {
                rewritten,
                message,
                deprecated: false,
            },
            Status::Deprecated { since, removed_in } => LegacyCall {
                rewritten,
                message: format!(
                    "{message} (deprecated since {since}, removed in {removed_in}; `rdra-ish fmt --write` rewrites it)"
                ),
                deprecated: true,
            },
        })
    })
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

fn legacy_transitions(pred: &PredicateCall) -> Option<(PredicateCall, String)> {
    let [PredicateArg::Ref(first), PredicateArg::Ref(second), _] = pred.args.as_slice() else {
        return None;
    };
    // `Entity.column` first and a one-part use case second. A namespaced use case
    // (`ns.Pay`) second is left to the type check.
    if first.kind_qualifier.is_some() || first.parts.len() < 2 || second.parts.len() != 1 {
        return None;
    }
    let mut rewritten = pred.clone();
    rewritten.args.swap(0, 1);
    let message = format!(
        "`transitions` takes the use case first; write `{}`",
        format_predicate(&rewritten)
    );
    Some((rewritten, message))
}

/// `sets(UC, E, cmp, true|false)`: `sets` assigns a value, and a comparison driven true or
/// false is `drives`. Every four-argument call is renamed; `drives` reports a bad argument.
fn legacy_sets_truth(pred: &PredicateCall) -> Option<(PredicateCall, String)> {
    if pred.args.len() != 4 {
        return None;
    }
    let mut rewritten = pred.clone();
    rewritten.name = "drives".into();
    let message = format!(
        "`sets` with a true/false argument drives a comparison, which is now `drives`; write `{}`",
        format_predicate(&rewritten)
    );
    Some((rewritten, message))
}

fn legacy_maps_field(pred: &PredicateCall) -> Option<(PredicateCall, String)> {
    let [field, entity, column] = pred.args.as_slice() else {
        return None;
    };
    let column = match column {
        PredicateArg::Lit(name) => Some(name.clone()),
        PredicateArg::Ref(qref) => qref.parts.last().cloned(),
        _ => None,
    };
    let (PredicateArg::Ref(entity), Some(column)) = (entity, column) else {
        return Some((
            pred.clone(),
            "`maps_field` takes two arguments; write `maps_field(Field, Entity.column)`".into(),
        ));
    };
    let mut target = entity.clone();
    target.parts.push(column);
    let mut rewritten = pred.clone();
    rewritten.args = vec![field.clone(), PredicateArg::Ref(target)];
    let message = format!(
        "`maps_field` names the column as `Entity.column`; write `{}`",
        format_predicate(&rewritten)
    );
    Some((rewritten, message))
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
            parse("entity E \"E\" { condition: String\n  state: String\n  location: String\n  when: String\n  writes: Int }\n");
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn writes_points_to_updates() {
        assert_eq!(
            messages("writes(Pay, Order)\n"),
            vec!["`writes` is no longer supported; use `updates` (or `creates`/`deletes`); `scripts/migrate-syntax-v2.py` rewrites it to `updates`"]
        );
    }

    #[test]
    fn transitions_with_the_column_first_names_the_rewrite() {
        assert_eq!(
            messages("transitions(Order.status, Pay, pending -> paid)\n"),
            vec!["`transitions` takes the use case first; write `transitions(Pay, Order.status, pending -> paid)`"]
        );
        assert_eq!(
            messages("transitions(Order.status, usecase::Pay, pending -> paid)\n"),
            vec!["`transitions` takes the use case first; write `transitions(usecase::Pay, Order.status, pending -> paid)`"]
        );
    }

    #[test]
    fn transitions_with_the_use_case_first_parses() {
        assert!(messages("transitions(Pay, Order.status, pending -> paid)\n").is_empty());
        assert!(messages("transitions(ns.Pay, Order.status, pending -> paid)\n").is_empty());
    }

    #[test]
    fn maps_field_with_three_arguments_names_the_rewrite() {
        for column in ["\"total\"", "total", "Order.total"] {
            assert_eq!(
                messages(&format!("maps_field(Total, Order, {column})\n")),
                vec!["`maps_field` names the column as `Entity.column`; write `maps_field(Total, Order.total)`"],
                "{column}"
            );
        }
        assert!(messages("maps_field(Total, Order.total)\n").is_empty());
    }

    #[test]
    fn four_argument_sets_loads_as_drives_with_a_warning() {
        let src = "sets(Sell, Stock, stock < selling, true)\n";
        let parsed = crate::parse_with_deprecations(src);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let messages: Vec<_> = parsed.deprecations.iter().map(|d| &d.message).collect();
        assert_eq!(
            messages,
            vec![
                "`sets` with a true/false argument drives a comparison, which is now `drives`; \
                 write `drives(Sell, Stock, stock < selling, true)` \
                 (deprecated since 0.5.0, removed in 2.0.0; `rdra-ish fmt --write` rewrites it)"
            ]
        );
        assert_eq!(parsed.deprecations[0].span, 0..src.len() - 1);
        let Some(crate::ast::Item::Predicate(pred)) = parsed.ast.items.first() else {
            panic!("no predicate");
        };
        assert_eq!(pred.name, "drives");
    }

    #[test]
    fn fmt_rewrites_four_argument_sets_to_drives() {
        assert_eq!(
            crate::format_source("// sold out\nsets(Sell, Stock, stock < selling, false)\n")
                .unwrap(),
            "// sold out\ndrives(Sell, Stock, stock < selling, false)\n"
        );
    }

    #[test]
    fn a_deprecated_form_is_not_a_parse_error() {
        assert!(messages("sets(Sell, Stock, stock < selling, true)\n").is_empty());
    }

    #[test]
    fn three_argument_sets_is_left_alone() {
        let parsed = crate::parse_with_deprecations("sets(Pay, Order, status == paid)\n");
        assert!(parsed.deprecations.is_empty());
        let Some(crate::ast::Item::Predicate(pred)) = parsed.ast.items.first() else {
            panic!("no predicate");
        };
        assert_eq!(pred.name, "sets");
    }

    #[test]
    fn legacy_form_ids_are_unique() {
        let mut ids: Vec<_> = super::LEGACY_FORMS.iter().map(|form| form.id).collect();
        let len = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), len);
    }

    #[test]
    fn only_a_predicate_rewrite_can_be_deprecated() {
        for form in super::LEGACY_FORMS {
            if matches!(form.status, super::Status::Deprecated { .. }) {
                assert!(
                    matches!(form.detect, super::Detect::Predicate(..)),
                    "{} is deprecated but nothing rewrites it",
                    form.id
                );
            }
        }
    }

    #[test]
    fn a_legacy_predicate_is_kept_in_its_rewritten_form() {
        let (ast, _) = parse("maps_field(Total, Order, \"total\")\n");
        let Some(crate::ast::Item::Predicate(pred)) = ast.items.first() else {
            panic!("no predicate");
        };
        assert_eq!(
            crate::format::format_predicate(pred),
            "maps_field(Total, Order.total)"
        );
    }
}
