//! The シーケンス図 (`--kind sequence`): what one use case does, in order. The starter calls
//! the screen, the screen calls each control, the guards decide whether the work runs, each
//! control reads and writes its entities one transaction at a time, and the events it raises
//! reach the external systems and the controls of the use cases they trigger.
//!
//! A Mermaid file holds one diagram, so the scope names exactly one use case.

use super::omitted_linked_kinds;
use super::robustness::controls_of;
use super::usecase::scoped_use_cases;
use crate::diagram_common::{contract_id, node_label, Header};
use crate::{node_kind_filter_name, EmitError, Emitter, Scope, View};
use rdra_ish_core::analysis::{applied_bodies, condition_body_display};
use rdra_ish_core::model::{
    EffectValue, EntityKey, NodeRef, RelKind, SemanticModel, SetValue, UseCaseKey,
};
use rdra_ish_core::{actors_for_usecase, infer_usecase_transactions, triggered_use_cases};
use std::collections::HashSet;

pub struct SequenceMermaidEmitter;

/// Text safe in a participant alias, a message, or an `alt` line: `;` ends a statement.
fn text(raw: &str) -> String {
    raw.replace(';', ",")
        .replace('"', "#quot;")
        .replace('\n', " ")
}

fn label(model: &SemanticModel, node: &NodeRef) -> String {
    text(node_label(model, node).unwrap_or_default())
}

fn effect_value(value: &EffectValue) -> String {
    match value {
        EffectValue::EnumVariant(variant) => variant.clone(),
        EffectValue::Bool(flag) => flag.to_string(),
        EffectValue::Int(number) => number.to_string(),
        EffectValue::Present | EffectValue::TypedPresent(_) => "set".to_string(),
        EffectValue::Null => "null".to_string(),
    }
}

/// What the use case does to `entity` on a write: the operation, the lifecycle moves, and
/// the other columns it sets.
fn write_label(
    model: &SemanticModel,
    usecase: UseCaseKey,
    entity: EntityKey,
    operation: &str,
) -> String {
    let transitions: Vec<_> = model
        .state_transitions
        .iter()
        .filter(|t| t.usecase == usecase && t.entity == entity)
        .collect();
    let chosen = choices(model, usecase, entity);
    let mut parts: Vec<String> = transitions
        .iter()
        .filter(|t| {
            !chosen
                .iter()
                .any(|(column, from, _)| *column == t.column && *from == t.from)
        })
        .map(|t| {
            format!(
                "{} {} -> {}{}",
                t.column,
                model.value_display(entity, &t.column, &t.from),
                model.value_display(entity, &t.column, &t.to),
                arm_guard(model, t)
            )
        })
        .collect();
    parts.extend(
        model
            .column_effects
            .iter()
            .filter(|e| e.origin == usecase && e.entity == entity)
            .filter(|e| !transitions.iter().any(|t| t.column == e.column))
            .map(|e| match &e.value {
                SetValue::Value(v) => format!("{}={}", e.column, effect_value(v)),
                SetValue::Copy(source) => format!(
                    "{}={}.{}",
                    e.column, model.entities[source.entity].id, source.column
                ),
            }),
    );
    // A `chooses`: the actor picks one of the variation's values, or of the listed ones.
    parts.extend(
        model
            .column_choices
            .iter()
            .filter(|c| c.origin == usecase && c.entity == entity)
            .map(|c| format!("{} in ({})", c.column, chosen_values(model, c).join(", "))),
    );
    if parts.is_empty() {
        operation.to_string()
    } else {
        text(&format!("{operation} {}", parts.join(", ")))
    }
}

/// The values a `chooses` lets the actor pick, by their labels: every value of the variation,
/// or the listed ones in declaration order, each with ` [label]` of its own `.when(Cond)` (UML
/// guard notation, as for an arm). A value listed twice is enabled by either line: an
/// unguarded line drops the guard, guarded lines join theirs with ` or `.
fn chosen_values(model: &SemanticModel, c: &rdra_ish_core::model::ColumnChoice) -> Vec<String> {
    let entity = c.entity;
    if c.values.is_empty() {
        return model.entities[entity]
            .columns
            .iter()
            .find(|col| col.name == c.column)
            .and_then(|col| model.column_values(col))
            .unwrap_or_default()
            .iter()
            .map(|v| model.value_display(entity, &c.column, v).to_string())
            .collect();
    }
    // Each value once: its guards' labels, `None` once an unguarded line enables it.
    let mut values: Vec<(&str, Option<Vec<&str>>)> = Vec::new();
    for v in &c.values {
        let label = v.guard.map(|ck| model.conditions[ck].label.as_str());
        match values.iter_mut().find(|(name, _)| *name == v.value) {
            Some((_, guards)) => match (guards.as_mut(), label) {
                (Some(gs), Some(l)) if !gs.contains(&l) => gs.push(l),
                (Some(_), None) => *guards = None,
                _ => {}
            },
            None => values.push((&v.value, label.map(|l| vec![l]))),
        }
    }
    values
        .into_iter()
        .map(|(v, guards)| {
            let shown = model.value_display(entity, &c.column, v);
            match guards {
                Some(gs) => format!("{shown} [{}]", gs.join(" or ")),
                None => shown.to_string(),
            }
        })
        .collect()
}

/// ` [label]` of an arm's own `.when(Cond)`, or nothing (UML guard notation, as the state
/// diagram writes it).
fn arm_guard(model: &SemanticModel, t: &rdra_ish_core::StateTransition) -> String {
    t.guard
        .map(|ck| format!(" [{}]", model.conditions[ck].label))
        .unwrap_or_default()
}

/// A choice on one column from one state: `(column, from, [(to, arm guard)])`.
type Choice = (String, String, Vec<(String, String)>);

/// The use case's choices on `entity`: each `(column, from)` with more than one arm, and the
/// `to` of each arm in declaration order with its own guard. One run takes one arm.
fn choices(model: &SemanticModel, usecase: UseCaseKey, entity: EntityKey) -> Vec<Choice> {
    let mut groups: Vec<Choice> = Vec::new();
    for t in model
        .state_transitions
        .iter()
        .filter(|t| t.usecase == usecase && t.entity == entity)
    {
        let arm = (t.to.clone(), arm_guard(model, t));
        match groups
            .iter_mut()
            .find(|(column, from, _)| *column == t.column && *from == t.from)
        {
            Some((_, _, tos)) => {
                if !tos.iter().any(|(to, _)| *to == t.to) {
                    tos.push(arm);
                }
            }
            None => groups.push((t.column.clone(), t.from.clone(), vec![arm])),
        }
    }
    groups.retain(|(_, _, tos)| tos.len() > 1);
    groups
}

/// The guards of the use case: each applied condition with a `holds` body, then the `from`
/// states of each transitioned column, each named once.
fn guards(model: &SemanticModel, usecase: UseCaseKey) -> Vec<String> {
    let conditions: Vec<String> = applied_bodies(model)
        .into_iter()
        .filter(|(_, uc, _)| *uc == usecase)
        .map(|(condition, _, _)| {
            let name = &model.conditions[condition].label;
            match condition_body_display(model, condition) {
                Some(body) => format!("{name}: {body}"),
                None => name.clone(),
            }
        })
        .collect();
    // A condition that already says `Entity.column == from` makes the same state guard redundant.
    let mut froms: Vec<(EntityKey, &str, Vec<String>)> = Vec::new();
    for t in model
        .state_transitions
        .iter()
        .filter(|t| t.usecase == usecase)
    {
        let from = model
            .value_display(t.entity, &t.column, &t.from)
            .to_string();
        match froms
            .iter_mut()
            .find(|(e, column, _)| *e == t.entity && *column == t.column)
        {
            Some((_, _, values)) => {
                if !values.contains(&from) {
                    values.push(from);
                }
            }
            None => froms.push((t.entity, &t.column, vec![from])),
        }
    }
    let states = froms
        .into_iter()
        .map(|(entity, column, values)| {
            let column = format!("{}.{column}", model.entities[entity].id);
            match &values[..] {
                [one] => format!("{column} == {one}"),
                many => format!("{column} in ({})", many.join(", ")),
            }
        })
        .filter(|said| !conditions.iter().any(|condition| condition.contains(said)));
    conditions
        .iter()
        .cloned()
        .chain(states)
        .map(|guard| text(&guard))
        .collect()
}

/// The participants in the order the diagram lists them: starters, screen, controls,
/// entities, then the controls and external systems an event reaches.
#[derive(Default)]
struct Participants(Vec<(u8, NodeRef)>);

impl Participants {
    fn add(&mut self, rank: u8, node: &NodeRef) {
        if !self.0.iter().any(|(_, seen)| seen == node) {
            self.0.push((rank, node.clone()));
        }
    }

    fn ordered(mut self) -> Vec<NodeRef> {
        self.0.sort_by_key(|(rank, _)| *rank);
        self.0.into_iter().map(|(_, node)| node).collect()
    }
}

/// The lines of the diagram body, each with the indent of the block it sits in.
struct Script<'a> {
    model: &'a SemanticModel,
    lines: Vec<String>,
    depth: usize,
}

impl Script<'_> {
    fn push(&mut self, line: String) {
        self.lines
            .push(format!("{}{line}", "  ".repeat(self.depth + 1)));
    }

    fn send(&mut self, from: &NodeRef, arrow: &str, to: &NodeRef, message: &str) {
        let id = |node: &NodeRef| contract_id(self.model, node).unwrap_or_default();
        self.push(format!("{} {arrow} {}: {message}", id(from), id(to)));
    }

    fn note(&mut self, over: &NodeRef, message: &str) {
        let id = contract_id(self.model, over).unwrap_or_default();
        self.push(format!("Note over {id}: {message}"));
    }
}

fn raisers_of(model: &SemanticModel, event: &NodeRef) -> Vec<NodeRef> {
    let mut raisers: Vec<NodeRef> = model
        .relations
        .iter()
        .filter(|rel| rel.kind == RelKind::Raises && rel.to == *event)
        .map(|rel| rel.from.clone())
        .collect();
    raisers.dedup();
    raisers
}

impl Emitter for SequenceMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let usecases = match &view.scope {
            Scope::UseCases(_) => scoped_use_cases(model, &view.scope),
            _ => Vec::new(),
        };
        let [usecase_key] = usecases[..] else {
            return Err(EmitError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "a sequence diagram draws exactly one use case",
            )));
        };
        let usecase = NodeRef::UseCase(usecase_key);
        let controls = controls_of(model, std::slice::from_ref(&usecase))
            .into_iter()
            .next()
            .map(|(_, own)| own)
            .unwrap_or_default();
        let first = controls.first();
        let uc_label = label(model, &usecase);
        let reaching = |rel_to: &NodeRef| {
            model
                .relations
                .iter()
                .filter(|rel| rel.kind == RelKind::Triggers && rel.to == *rel_to)
                .map(|rel| rel.from.clone())
                .collect::<Vec<_>>()
        };

        // Who starts the use case: its performers, or what triggers it.
        let triggered = triggered_use_cases(model).contains(&usecase_key);
        let performers: Vec<NodeRef> = actors_for_usecase(model, usecase_key)
            .into_iter()
            .map(NodeRef::Actor)
            .collect();
        let triggers = if triggered {
            reaching(&usecase)
        } else {
            Vec::new()
        };
        let mut starters: Vec<(NodeRef, NodeRef)> = Vec::new();
        for trigger in &triggers {
            match trigger {
                NodeRef::Event(_) => starters.extend(
                    raisers_of(model, trigger)
                        .into_iter()
                        .map(|raiser| (raiser, trigger.clone())),
                ),
                _ => starters.push((trigger.clone(), trigger.clone())),
            }
        }
        starters.sort_by_key(|(from, _)| contract_id(model, from));
        starters.dedup();

        let screen = model
            .relations
            .iter()
            .find(|rel| rel.kind == RelKind::Displays && rel.from == usecase)
            .map(|rel| rel.to.clone());

        let mut participants = Participants::default();
        for node in performers
            .iter()
            .chain(starters.iter().map(|(from, _)| from))
        {
            participants.add(0, node);
        }
        if let Some(screen) = &screen {
            participants.add(1, screen);
        }
        for control in &controls {
            participants.add(2, control);
        }

        let mut script = Script {
            model,
            lines: Vec::new(),
            depth: 0,
        };
        let entry = screen.as_ref().or(first);
        if let Some(entry) = entry {
            for actor in &performers {
                script.send(actor, "->>", entry, &uc_label);
            }
        }
        if let Some(control) = first {
            for (from, event) in &starters {
                script.send(from, "->>", control, &label(model, event));
            }
        }
        if let (Some(screen), Some(control)) = (&screen, first) {
            let call = if *control == usecase {
                uc_label.clone()
            } else {
                label(model, control)
            };
            script.send(screen, "->>", control, &call);
        }

        // Whoever receives the reply: the screen, else the first starter.
        let caller = screen
            .clone()
            .or_else(|| performers.first().cloned())
            .or_else(|| starters.first().map(|(from, _)| from.clone()));

        let guard_texts = guards(model, usecase_key);
        let guarded = !guard_texts.is_empty();
        if guarded {
            script.push(format!("alt {}", guard_texts.join(" and ")));
            script.depth += 1;
        }

        if let Some(screen) = &screen {
            for control in controls.iter().skip(1) {
                script.send(screen, "->>", control, &label(model, control));
            }
        }

        let transactions = infer_usecase_transactions(model)
            .into_iter()
            .find(|tx| tx.usecase == usecase_key);
        for control in &controls {
            let own_api = match control {
                NodeRef::Api(key) => Some(*key),
                _ => None,
            };
            let mut read: Vec<&NodeRef> = Vec::new();
            for rel in &model.relations {
                if rel.kind == RelKind::Reads
                    && rel.from == *control
                    && matches!(rel.to, NodeRef::Entity(_))
                    && !read.contains(&&rel.to)
                {
                    read.push(&rel.to);
                }
            }
            for entity in read {
                participants.add(3, entity);
                script.send(control, "->>", entity, "read");
            }
            let groups = transactions.iter().flat_map(|tx| tx.groups());
            for group in groups.filter(|g| {
                g.ordered_writes
                    .first()
                    .is_some_and(|write| write.via_api == own_api)
            }) {
                if let Some(frame) = group.kind.frame_label() {
                    script.note(control, frame);
                }
                for write in &group.ordered_writes {
                    let entity = NodeRef::Entity(write.entity);
                    participants.add(3, &entity);
                    let message = write_label(model, usecase_key, write.entity, write.kind.label());
                    script.send(control, "->>", &entity, &message);
                    for (column, from, tos) in choices(model, usecase_key, write.entity) {
                        let shown =
                            |v: &str| model.value_display(write.entity, &column, v).to_string();
                        for (i, (to, guard)) in tos.iter().enumerate() {
                            let keyword = if i == 0 { "alt" } else { "else" };
                            if i > 0 {
                                script.depth -= 1;
                            }
                            script.push(format!(
                                "{keyword} {}",
                                text(&format!(
                                    "{column} {} -> {}{guard}",
                                    shown(&from),
                                    shown(to)
                                ))
                            ));
                            script.depth += 1;
                            script.send(
                                control,
                                "->>",
                                &entity,
                                &text(&format!("{column} = {}", shown(to))),
                            );
                        }
                        script.depth -= 1;
                        script.push("end".to_string());
                    }
                }
            }
        }

        // The events the use case raises, and the controls and systems they reach.
        if let Some(control) = first {
            for raise in model
                .relations
                .iter()
                .filter(|rel| rel.kind == RelKind::Raises && rel.from == usecase)
            {
                let event = label(model, &raise.to);
                let targets: Vec<NodeRef> = model
                    .relations
                    .iter()
                    .filter(|rel| rel.kind == RelKind::Triggers && rel.from == raise.to)
                    .filter_map(|rel| match &rel.to {
                        target @ NodeRef::UseCase(_) => {
                            controls_of(model, std::slice::from_ref(target))
                                .into_iter()
                                .next()
                                .and_then(|(_, own)| own.into_iter().next())
                        }
                        target @ NodeRef::ExtSystem(_) => Some(target.clone()),
                        _ => None,
                    })
                    .collect();
                if targets.is_empty() {
                    script.note(control, &format!("raises {event}"));
                }
                for target in &targets {
                    participants.add(4, target);
                    script.send(control, "-)", target, &event);
                }
            }
        }

        if let (Some(control), Some(caller)) = (first, &caller) {
            script.send(control, "-->>", caller, "done");
        }
        if guarded {
            script.depth -= 1;
            script.push("else guard fails".to_string());
            script.depth += 1;
            match (first, &caller) {
                (Some(control), Some(caller)) => script.send(control, "-->>", caller, "rejected"),
                (Some(control), None) => script.note(control, "rejected"),
                _ => {}
            }
            script.depth -= 1;
            script.push("end".to_string());
        }

        let nodes = participants.ordered();
        let mut body = String::new();
        for node in &nodes {
            let keyword = if matches!(node, NodeRef::Actor(_)) {
                "actor"
            } else {
                "participant"
            };
            body.push_str(&format!(
                "  {keyword} {} as {}\n",
                contract_id(model, node).unwrap_or_default(),
                label(model, node)
            ));
        }
        for line in &script.lines {
            body.push_str(line);
            body.push('\n');
        }

        let mut kinds: Vec<&'static str> = Vec::new();
        for node in &nodes {
            let kind = node_kind_filter_name(node);
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
        let drawn: HashSet<NodeRef> = nodes.into_iter().collect();
        let header = Header {
            diagram: "sequence",
            view,
            layers: None,
            omitted: omitted_linked_kinds(model, |node| *node == usecase, &drawn, &kinds),
            kinds,
        };
        Ok(format!("{}sequenceDiagram\n{body}", header.render()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagram_common::model_from;

    const CLINIC: &str = r#"
actor Patient "Patient"
actor Clerk "Clerk"
extsystem Bank "Bank"
buc BucPay "Pay"
usecase Pay "Pay bill"
usecase Reconcile "Reconcile"
usecase Archive "Archive bill"
usecase Remind "Remind"
usecase Split "Split bill"
screen PayScreen "Pay screen"
api PayApi "Pay API"
api LedgerApi "Ledger API"
entity Bill "Bill" {
  id: Int @pk
  status: Enum(open, paid) @default(open)
  paid_at: DateTime @null
}
entity Receipt "Receipt" { id: Int @pk }
event Paid "Paid"
event Wire "Wire"
timer Nightly "Nightly"
performs(Patient, BucPay)
performs(Clerk, Remind)
contains(BucPay, Pay)
contains(BucPay, Split)
displays(Pay, PayScreen)
displays(Split, PayScreen)
invokes(Pay, PayApi)
reads(PayApi, Bill)
updates(PayApi, Bill)
creates(PayApi, Receipt)
transitions(Pay, Bill.status, open -> paid)
sets(Pay, Bill, paid_at == present)
condition Billable "Billable bill"
holds(Billable, Bill.status == open)
applies_to(Billable, Pay)
raises(Pay, Paid)
triggers(Paid, Reconcile)
triggers(Paid, Bank)
raises(Bank, Wire)
triggers(Wire, Archive)
updates(Archive, Bill)
triggers(Nightly, Remind)
invokes(Split, PayApi)
invokes(Split, LedgerApi)
updates(LedgerApi, Receipt)
"#;

    fn emit(usecase: &str) -> String {
        SequenceMermaidEmitter
            .emit(
                &model_from(CLINIC),
                &View::usecases(vec![usecase.to_string()]),
            )
            .unwrap()
    }

    fn has_in_order(text: &str, lines: &[&str]) {
        let mut rest = text;
        for line in lines {
            let at = rest
                .find(line)
                .unwrap_or_else(|| panic!("`{line}` missing or out of order in\n{text}"));
            rest = &rest[at + line.len()..];
        }
    }

    #[test]
    fn a_screen_use_case_runs_guarded_writes_and_raises_its_event() {
        let result = emit("Pay");
        assert!(
            result.starts_with("%% rdra-ish diagram=sequence scope=usecase:Pay\n"),
            "{result}"
        );
        has_in_order(
            &result,
            &[
                "sequenceDiagram\n",
                "  actor actor__Patient as Patient\n",
                "  participant screen__PayScreen as Pay screen\n",
                "  participant api__PayApi as Pay API\n",
                "  participant entity__Bill as Bill\n",
                "  participant entity__Receipt as Receipt\n",
                "  participant usecase__Reconcile as Reconcile\n",
                "  participant extsystem__Bank as Bank\n",
                "  actor__Patient ->> screen__PayScreen: Pay bill\n",
                "  screen__PayScreen ->> api__PayApi: Pay API\n",
                "  alt Billable bill: Bill.status == open\n",
                "    api__PayApi ->> entity__Bill: read\n",
                "    Note over api__PayApi: transaction (API atomic boundary)\n",
                "    api__PayApi ->> entity__Bill: update status open -> paid, paid_at=set\n",
                "    api__PayApi ->> entity__Receipt: create\n",
                "    api__PayApi -) usecase__Reconcile: Paid\n",
                "    api__PayApi -) extsystem__Bank: Paid\n",
                "    api__PayApi -->> screen__PayScreen: done\n",
                "  else guard fails\n",
                "    api__PayApi -->> screen__PayScreen: rejected\n",
                "  end\n",
            ],
        );
    }

    #[test]
    fn a_use_case_without_an_api_is_its_own_control_and_without_guards_has_no_alt() {
        let result = emit("Reconcile");
        assert!(!result.contains("alt "), "{result}");
        assert!(result.contains("  participant usecase__Reconcile as Reconcile\n"));
        let result = emit("Archive");
        assert!(
            result.contains("  usecase__Archive ->> entity__Bill: update\n"),
            "{result}"
        );
    }

    #[test]
    fn an_event_starts_a_use_case_from_its_source_and_a_timer_from_itself() {
        let result = emit("Archive");
        has_in_order(
            &result,
            &[
                "  participant extsystem__Bank as Bank\n",
                "  participant usecase__Archive as Archive bill\n",
                "  extsystem__Bank ->> usecase__Archive: Wire\n",
            ],
        );
        assert!(!result.contains("actor "), "{result}");
        let timer = emit("Remind");
        assert!(
            timer.contains("  timer__Nightly ->> usecase__Remind: Nightly\n"),
            "{timer}"
        );
        assert!(timer.contains("  actor actor__Clerk as Clerk\n"), "{timer}");
    }

    #[test]
    fn two_apis_are_two_controls_in_order() {
        let result = emit("Split");
        has_in_order(
            &result,
            &[
                "  screen__PayScreen ->> api__LedgerApi: Ledger API\n",
                "  api__LedgerApi ->> entity__Receipt: update\n",
            ],
        );
        assert!(
            result.contains("  screen__PayScreen ->> api__PayApi: Pay API\n"),
            "{result}"
        );
    }

    const CHOICE: &str = r#"
actor Board "Board"
usecase Decide "Decide"
performs(Board, Decide)
entity Doc "Doc" {
  id: Int @pk
  status: Enum(searched, file, secret, drop, held) @default(searched)
  flag: Bool @default(false)
}
updates(Decide, Doc)
transitions(Decide, Doc.status, searched -> file)
transitions(Decide, Doc.status, searched -> secret)
transitions(Decide, Doc.status, held -> drop)
sets(Decide, Doc, flag == true)
"#;

    /// An arm's own `.when(Cond)` heads its `alt` branch, and a single guarded arm carries it
    /// on the write.
    #[test]
    fn an_arm_guard_heads_its_branch() {
        let src =
            format!("{CHOICE}condition Ready \"権利取得済\"\nholds(Ready, Doc.flag == true)\n")
                .replace(
                    "transitions(Decide, Doc.status, searched -> file)",
                    "transitions(Decide, Doc.status, searched -> file).when(Ready)",
                )
                .replace(
                    "transitions(Decide, Doc.status, held -> drop)",
                    "transitions(Decide, Doc.status, held -> drop).when(Ready)",
                );
        let result = SequenceMermaidEmitter
            .emit(
                &model_from(&src),
                &View::usecases(vec!["Decide".to_string()]),
            )
            .unwrap();
        has_in_order(
            &result,
            &[
                "update status held -> drop [権利取得済], flag=true\n",
                "    alt status searched -> file [権利取得済]\n",
                "    else status searched -> secret\n",
            ],
        );
    }

    /// Arms from one state are the actor's choice: the guard names each source state once,
    /// the write carries the rest, and an `alt` holds one branch per arm.
    #[test]
    fn arms_from_one_state_are_an_alt_per_arm() {
        let result = SequenceMermaidEmitter
            .emit(
                &model_from(CHOICE),
                &View::usecases(vec!["Decide".to_string()]),
            )
            .unwrap();
        has_in_order(
            &result,
            &[
                "  alt Doc.status in (searched, held)\n",
                "    usecase__Decide ->> entity__Doc: update status held -> drop, flag=true\n",
                "    alt status searched -> file\n",
                "      usecase__Decide ->> entity__Doc: status = file\n",
                "    else status searched -> secret\n",
                "      usecase__Decide ->> entity__Doc: status = secret\n",
                "    end\n",
                "    usecase__Decide -->> actor__Board: done\n",
                "  else guard fails\n",
            ],
        );
    }

    /// A `chooses` writes the values the actor may pick: every value of the variation, or the
    /// listed ones, each with its own `.when(Cond)` guard.
    #[test]
    fn a_choice_writes_its_values_with_their_guards() {
        let base = r#"
variation Scope "範囲" {
  limited "限定"
  free "自由"
  none "なし"
}
actor Attorney "Attorney"
usecase Draft "Draft"
performs(Attorney, Draft)
entity Notice "Notice" {
  id: Int @pk
  final: Bool @default(false)
  amendment: Scope
}
updates(Draft, Notice)
condition NotFinal "最後の通知でない"
holds(NotFinal, Notice.final == false)
"#;
        let emit = |lines: &str| {
            SequenceMermaidEmitter
                .emit(
                    &model_from(&format!("{base}{lines}\n")),
                    &View::usecases(vec!["Draft".to_string()]),
                )
                .unwrap()
        };
        let any = emit("chooses(Draft, Notice.amendment)");
        assert!(
            any.contains("update amendment in (限定, 自由, なし)"),
            "{any}"
        );
        let listed = emit(
            "chooses(Draft, Notice.amendment, free).when(NotFinal)\nchooses(Draft, Notice.amendment, limited)",
        );
        assert!(
            listed.contains("update amendment in (自由 [最後の通知でない], 限定)"),
            "{listed}"
        );
    }

    #[test]
    fn a_scope_that_is_not_one_use_case_is_refused() {
        let model = model_from(CLINIC);
        let two = View::usecases(vec!["Pay".to_string(), "Split".to_string()]);
        assert!(SequenceMermaidEmitter.emit(&model, &two).is_err());
        assert!(SequenceMermaidEmitter.emit(&model, &View::whole()).is_err());
    }
}
