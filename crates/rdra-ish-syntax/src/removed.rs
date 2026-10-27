//! Migration hints for keywords and predicates removed from the DSL.

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
    fn removed_words_stay_usable_as_column_names() {
        let (_, errs) =
            parse("entity E \"E\" { condition: String\n  state: String\n  location: String }\n");
        assert!(errs.is_empty(), "{errs:?}");
    }
}
