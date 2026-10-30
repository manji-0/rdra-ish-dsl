//! The canonical text of a model's published surface: what a consumer may reference and lock.

use std::collections::BTreeMap;

use crate::model::{
    ColumnType, EffectValue, EntityKey, Exposed, ModelColumn, NodeRef, RelKind, SemanticModel,
    UseCaseKey,
};
use crate::tx::infer_usecase_transactions;

/// The published surface as lines of canonical text, sorted by kind and id so declaration
/// order cannot change it. The format is frozen: a consumer's lock stores this text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surface {
    pub lines: Vec<String>,
}

impl Surface {
    pub fn of(model: &SemanticModel) -> Surface {
        let publication = &model.publication;
        let mut use_cases: BTreeMap<String, String> = BTreeMap::new();
        let mut events: BTreeMap<String, String> = BTreeMap::new();
        let mut entities: BTreeMap<String, (EntityKey, String)> = BTreeMap::new();
        for x in &publication.exposures {
            match x.item {
                Exposed::UseCase { usecase, caller } => {
                    let uc = &model.use_cases[usecase];
                    let mut line = format!(
                        "usecase {} {} caller({})",
                        uc.id,
                        quoted(&uc.label),
                        caller.as_str()
                    );
                    let requires =
                        targets(model, usecase, RelKind::RequiresPermission, |n| match n {
                            NodeRef::Permission(k) => Some(model.permissions[*k].id.clone()),
                            _ => None,
                        });
                    if !requires.is_empty() {
                        line.push_str(&format!(" requires({})", requires.join(", ")));
                    }
                    let raises = targets(model, usecase, RelKind::Raises, |n| match n {
                        NodeRef::Event(k) => Some(model.events[*k].id.clone()),
                        _ => None,
                    });
                    if !raises.is_empty() {
                        line.push_str(&format!(" raises({})", raises.join(", ")));
                    }
                    use_cases.insert(uc.id.clone(), line);
                }
                Exposed::Event(event) => {
                    let ev = &model.events[event];
                    events.insert(
                        ev.id.clone(),
                        format!("event {} {}", ev.id, quoted(&ev.label)),
                    );
                }
                Exposed::Entity { entity, mode } => {
                    let e = &model.entities[entity];
                    entities.insert(
                        e.id.clone(),
                        (
                            entity,
                            format!(
                                "entity {} {} {} {}",
                                e.id,
                                quoted(&e.label),
                                mode.as_str(),
                                columns_text(model, &e.columns)
                            ),
                        ),
                    );
                }
            }
        }

        let txs = infer_usecase_transactions(model);
        let mut writers = Vec::new();
        for (entity_id, (entity, _)) in &entities {
            let mut exposed: BTreeMap<String, Vec<String>> = BTreeMap::new();
            let mut unexposed: Option<Vec<String>> = None;
            for utx in &txs {
                if !utx.writes().any(|w| w.entity == *entity) {
                    continue;
                }
                let effects = effects_text(model, utx.usecase, *entity);
                if publication.caller_of(utx.usecase).is_some() {
                    exposed.insert(model.use_cases[utx.usecase].id.clone(), effects);
                } else {
                    unexposed.get_or_insert_with(Vec::new).extend(effects);
                }
            }
            for (uc, effects) in exposed {
                writers.push(writer_line(entity_id, &uc, effects));
            }
            if let Some(effects) = unexposed {
                writers.push(writer_line(entity_id, "(unexposed)", effects));
            }
        }

        let mut shared: BTreeMap<String, String> = BTreeMap::new();
        for s in &publication.shares {
            let e = &model.entities[s.entity];
            shared.insert(
                e.id.clone(),
                format!(
                    "shared {} {} {}",
                    e.id,
                    quoted(&e.label),
                    columns_text(model, &e.columns)
                ),
            );
        }

        let lines = use_cases
            .into_values()
            .chain(events.into_values())
            .chain(entities.into_values().map(|(_, line)| line))
            .chain(writers)
            .chain(shared.into_values())
            .collect();
        Surface { lines }
    }

    /// The lines joined with a newline after each; empty for a model that publishes nothing.
    pub fn text(&self) -> String {
        self.lines.iter().map(|l| format!("{l}\n")).collect()
    }
}

/// Sorted ids of the `kind` targets of `usecase`.
fn targets(
    model: &SemanticModel,
    usecase: UseCaseKey,
    kind: RelKind,
    id_of: impl Fn(&NodeRef) -> Option<String>,
) -> Vec<String> {
    let mut ids: Vec<String> = model
        .relations
        .iter()
        .filter(|r| r.kind == kind && r.from == NodeRef::UseCase(usecase))
        .filter_map(|r| id_of(&r.to))
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// A label between double quotes, escaped so every element stays on one line.
fn quoted(label: &str) -> String {
    let mut out = String::with_capacity(label.len() + 2);
    out.push('"');
    for ch in label.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn writer_line(entity: &str, writer: &str, mut effects: Vec<String>) -> String {
    effects.sort();
    effects.dedup();
    if effects.is_empty() {
        format!("writer {entity} {writer}")
    } else {
        format!("writer {entity} {writer}: {}", effects.join(", "))
    }
}

/// `col == val` for every `sets` of `usecase` on `entity`.
fn effects_text(model: &SemanticModel, usecase: UseCaseKey, entity: EntityKey) -> Vec<String> {
    let columns = model
        .column_effects
        .iter()
        .filter(|e| e.origin == usecase && e.entity == entity)
        .map(|e| format!("{} == {}", e.column, effect_value_text(&e.value)));
    let propositions = model
        .proposition_effects
        .iter()
        .filter(|e| e.origin == usecase && e.entity == entity)
        .map(|e| format!("{} == {}", e.prop.display(), e.truth));
    columns.chain(propositions).collect()
}

fn effect_value_text(value: &EffectValue) -> String {
    match value {
        EffectValue::EnumVariant(v) => v.clone(),
        EffectValue::Bool(b) => b.to_string(),
        EffectValue::Int(n) => n.to_string(),
        EffectValue::Present | EffectValue::TypedPresent(_) => "present".to_string(),
        EffectValue::Null => "null".to_string(),
    }
}

fn columns_text(model: &SemanticModel, columns: &[ModelColumn]) -> String {
    let cols: Vec<String> = columns
        .iter()
        .map(|c| {
            let mut text = format!("{}: {}", c.name, type_text(model, &c.col_type));
            if c.is_pk {
                text.push_str(" @pk");
            }
            if c.is_unique {
                text.push_str(" @unique");
            }
            if c.is_nullable {
                text.push_str(" @null");
            }
            text
        })
        .collect();
    if cols.is_empty() {
        "{ }".to_string()
    } else {
        format!("{{ {} }}", cols.join("  "))
    }
}

fn type_text(model: &SemanticModel, col_type: &ColumnType) -> String {
    match col_type {
        ColumnType::Int => "Int".to_string(),
        ColumnType::String => "String".to_string(),
        ColumnType::Money => "Money".to_string(),
        ColumnType::DateTime => "DateTime".to_string(),
        ColumnType::Date => "Date".to_string(),
        ColumnType::Bool => "Bool".to_string(),
        ColumnType::Decimal => "Decimal".to_string(),
        ColumnType::Enum(values) => format!("Enum({})", values.join(", ")),
        ColumnType::Variation(key) => format!("Enum({})", model.variations[*key].values.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::build_model;
    use rdra_ish_syntax::parse;

    fn surface(src: &str) -> String {
        let (ast, errors) = parse(src);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "model errors: {errors:?}");
        Surface::of(&model).text()
    }

    const DECLS: &[&str] = &[
        "system AuthSystem \"Authentication System\"",
        "actor User \"User\"",
        "permission CanLogin \"Can log in\"",
        "has_permission(User, CanLogin)",
        "usecase Login \"Log in\"",
        "usecase Revoke \"Revoke a session\"",
        "performs(User, Login)",
        "performs(User, Revoke)",
        "requires_permission(Login, CanLogin)",
        "entity Session \"Session\" { id: Int @pk  status: Enum(none, active, revoked) @default(none) }",
        "creates(Login, Session)",
        "transitions(Session.status, Login, none -> active)",
        "transitions(Session.status, Revoke, active -> revoked)",
        "entity SessionView \"Session (published)\" { session_id: Int @pk  account_id: Int  status: Enum(active, ended) }",
        "writes(Login, SessionView)",
        "sets(Login, SessionView, status == active)",
        "writes(Revoke, SessionView)",
        "sets(Revoke, SessionView, status == ended)",
        "event EvLoggedIn \"Logged in\"",
        "raises(Login, EvLoggedIn)",
        "outbox(EvLoggedIn)",
        "exposes(AuthSystem, Login)",
        "exposes(AuthSystem, SessionView).upsert()",
    ];

    const CANONICAL: &str = "\
usecase Login \"Log in\" caller(actor) requires(CanLogin) raises(EvLoggedIn)
entity SessionView \"Session (published)\" upsert { session_id: Int @pk  account_id: Int  status: Enum(active, ended) }
writer SessionView Login: status == active
writer SessionView (unexposed): status == ended
";

    #[test]
    fn the_surface_is_the_design_text_and_declaration_order_cannot_change_it() {
        let forward = surface(&DECLS.join("\n"));
        assert_eq!(forward, CANONICAL);
        let mut reversed: Vec<&str> = DECLS.to_vec();
        reversed.reverse();
        assert_eq!(surface(&reversed.join("\n")), forward);
    }

    #[test]
    fn exposed_events_and_shared_entities_get_their_own_lines() {
        let text = surface(
            r#"
system S "S"
usecase Ping "Ping"
event EvPing "Pinged"
raises(Ping, EvPing)
outbox(EvPing)
entity Beat "Beat" { id: Int @pk }
entity Log "Log" { id: Int @pk  note: String @null }
creates(Ping, Beat)
creates(Ping, Log)
exposes(S, EvPing)
exposes(S, Ping).caller(system)
exposes(S, Log).append()
shares(S, Log)
"#,
        );
        assert_eq!(
            text,
            "usecase Ping \"Ping\" caller(system) raises(EvPing)\n\
             event EvPing \"Pinged\"\n\
             entity Log \"Log\" append { id: Int @pk  note: String @null }\n\
             writer Log Ping\n\
             shared Log \"Log\" { id: Int @pk  note: String @null }\n"
        );
    }

    #[test]
    fn a_label_with_a_newline_backslash_or_quote_stays_on_one_line() {
        let text = surface(
            "system S \"S\"\nevent EvPing \"Pinged, with\nnewline \\\\ and 'quote'\"\nusecase Ping \"Ping\"\nraises(Ping, EvPing)\noutbox(EvPing)\nexposes(S, EvPing)",
        );
        assert_eq!(
            text,
            "event EvPing \"Pinged, with\\nnewline \\\\\\\\ and 'quote'\"\n"
        );
        assert_eq!(quoted("say \"hi\"\r\n"), "\"say \\\"hi\\\"\\r\\n\"");
    }

    #[test]
    fn a_model_that_publishes_nothing_has_an_empty_surface() {
        assert_eq!(surface("usecase U \"U\""), "");
    }
}
