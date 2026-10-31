//! イベントフロー集約: raises/triggers/transitions の結合ビュー。
//!
//! `collect_event_flows` が唯一のグラフ走査点。検証・可視化・sets 展開から再利用する。
//! `api_diagnostics` も本モジュールで提供する（同様の 1パス集約パターン）。

use crate::diagnostics::{Diagnostic, RdraError};
use crate::location::push_model_decl_diagnostic;
use crate::model::{
    BucKey, EventKey, ExtSystemKey, NodeRef, Raiser, RelKind, SemanticModel, StateTransition,
    TimerKey, UseCaseKey,
};
use std::collections::HashMap;

/// イベントを中心とした因果連鎖ファクト。
#[derive(Debug)]
pub struct EventFlow {
    /// このフローのイベントキー。
    pub event: EventKey,
    /// `raises(UseCase | ExtSystem, event)` で宣言した発生元の一覧。
    pub raised_by: Vec<Raiser>,
    /// `triggers(event, UseCase)` で宣言した UseCase の一覧。
    pub triggers_ucs: Vec<UseCaseKey>,
    /// `triggers(event, Buc)` で宣言した BUC の一覧。
    pub triggers_bucs: Vec<BucKey>,
    /// `triggers(event, ExtSystem)` で宣言した、イベントの配送先外部システムの一覧。
    pub delivered_to: Vec<ExtSystemKey>,
    /// このイベントが `triggers` する UseCase が駆動する状態遷移（下流側、導出値）。
    pub transitions: Vec<StateTransition>,
}

impl EventFlow {
    /// `raises(UseCase, event)` で宣言した UseCase。
    pub fn raising_usecases(&self) -> Vec<UseCaseKey> {
        self.raised_by
            .iter()
            .filter_map(|raiser| match raiser {
                Raiser::UseCase(uk) => Some(*uk),
                Raiser::ExtSystem(_) => None,
            })
            .collect()
    }

    /// `raises(ExtSystem, event)` で宣言した外部システム。
    pub fn raising_extsystems(&self) -> Vec<ExtSystemKey> {
        self.raised_by
            .iter()
            .filter_map(|raiser| match raiser {
                Raiser::ExtSystem(xk) => Some(*xk),
                Raiser::UseCase(_) => None,
            })
            .collect()
    }
}

/// タイマーが起動する UseCase / BUC（イベントを介さない起動元）。
#[derive(Debug)]
pub struct TimerFlow {
    pub timer: TimerKey,
    /// `triggers(Timer, UseCase)` で宣言した UseCase の一覧。
    pub triggers_ucs: Vec<UseCaseKey>,
    /// `triggers(Timer, Buc)` で宣言した BUC の一覧。
    pub triggers_bucs: Vec<BucKey>,
}

/// `triggers` の起動元になっている全タイマーを id 順に集める。
pub fn collect_timer_flows(model: &SemanticModel) -> Vec<TimerFlow> {
    let mut map: HashMap<TimerKey, TimerFlow> = HashMap::new();
    for rel in &model.relations {
        let (RelKind::Triggers, NodeRef::Timer(tk)) = (&rel.kind, &rel.from) else {
            continue;
        };
        let flow = map.entry(*tk).or_insert_with(|| TimerFlow {
            timer: *tk,
            triggers_ucs: Vec::new(),
            triggers_bucs: Vec::new(),
        });
        match &rel.to {
            NodeRef::UseCase(uk) => flow.triggers_ucs.push(*uk),
            NodeRef::Buc(bk) => flow.triggers_bucs.push(*bk),
            _ => {}
        }
    }
    let mut result: Vec<TimerFlow> = map.into_values().collect();
    result.sort_by_key(|f| {
        model
            .timers
            .get(f.timer)
            .map(|t| t.id.clone())
            .unwrap_or_default()
    });
    result
}

/// モデル内の全イベントについて `EventFlow` を収集する。
///
/// `model.relations` を 1 パス走査してイベントキーに集約し、
/// `triggers` 先 UseCase の `transitions` を下流の遷移として付ける。
pub fn collect_event_flows(model: &SemanticModel) -> Vec<EventFlow> {
    let flow = |ek, raised_by| EventFlow {
        event: ek,
        raised_by,
        triggers_ucs: Vec::new(),
        triggers_bucs: Vec::new(),
        delivered_to: Vec::new(),
        transitions: Vec::new(),
    };
    let mut map: HashMap<EventKey, EventFlow> = model
        .events
        .keys()
        .map(|ek| (ek, flow(ek, Vec::new())))
        .collect();
    // A foreign event the product reacts to is raised by the external system it is bound
    // through; one it never names stays out.
    for rel in &model.relations {
        if let (RelKind::Triggers, NodeRef::Event(ek)) = (&rel.kind, &rel.from) {
            if let Some(origin) = model.events.origin(*ek) {
                map.entry(*ek)
                    .or_insert_with(|| flow(*ek, vec![Raiser::ExtSystem(origin.ext)]));
            }
        }
    }

    for rel in &model.relations {
        match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Raises, from, NodeRef::Event(ek)) => {
                if let (Some(flow), Some(raiser)) = (map.get_mut(ek), Raiser::from_node_ref(from)) {
                    flow.raised_by.push(raiser);
                }
            }
            (RelKind::Triggers, NodeRef::Event(ek), NodeRef::ExtSystem(xk)) => {
                if let Some(flow) = map.get_mut(ek) {
                    flow.delivered_to.push(*xk);
                }
            }
            (RelKind::Triggers, NodeRef::Event(ek), NodeRef::UseCase(uk)) => {
                if let Some(flow) = map.get_mut(ek) {
                    flow.triggers_ucs.push(*uk);
                }
            }
            (RelKind::Triggers, NodeRef::Event(ek), NodeRef::Buc(bk)) => {
                if let Some(flow) = map.get_mut(ek) {
                    flow.triggers_bucs.push(*bk);
                }
            }
            _ => {}
        }
    }

    for flow in map.values_mut() {
        flow.transitions = model
            .state_transitions
            .iter()
            .filter(|st| flow.triggers_ucs.contains(&st.usecase))
            .cloned()
            .collect();
    }

    let mut result: Vec<EventFlow> = map.into_values().collect();
    result.sort_by_key(|f| {
        model
            .events
            .get(f.event)
            .map(|e| e.id.clone())
            .unwrap_or_default()
    });
    result
}

/// イベント整合性の診断を生成する。
///
/// `build_model` 後、`tx_diagnostics` と同様に呼び出す。
/// 全て `Diagnostic::warning` として返す（エラーではない）。
pub fn event_diagnostics(model: &SemanticModel) -> Vec<Diagnostic> {
    let flows = collect_event_flows(model);
    let mut diags = Vec::new();

    for flow in &flows {
        let event_id = model
            .events
            .get(flow.event)
            .map(|e| e.id.clone())
            .unwrap_or_default();

        // どの UC からも raises されていない
        if flow.raised_by.is_empty() {
            push_model_decl_diagnostic(
                model,
                &mut diags,
                "event",
                &event_id,
                RdraError::EventNeverRaised {
                    event: event_id.clone(),
                },
                true,
            );
        }

        // raise されているが triggers 先も outbox もない（行き止まり）
        if !flow.raised_by.is_empty()
            && flow.triggers_ucs.is_empty()
            && flow.triggers_bucs.is_empty()
            && flow.delivered_to.is_empty()
            && !model.outbox_events.contains(&flow.event)
        {
            push_model_decl_diagnostic(
                model,
                &mut diags,
                "event",
                &event_id,
                RdraError::EventNeverConsumed {
                    event: event_id.clone(),
                },
                true,
            );
        }

        // triggers 先 UC がどの BUC にも contains されていない
        for &uc_key in &flow.triggers_ucs {
            let belongs_to_buc = model
                .relations
                .iter()
                .any(|r| r.kind == RelKind::Contains && r.to == NodeRef::UseCase(uc_key));
            if !belongs_to_buc {
                let uc_id = model
                    .use_cases
                    .get(uc_key)
                    .map(|u| u.id.clone())
                    .unwrap_or_default();
                push_model_decl_diagnostic(
                    model,
                    &mut diags,
                    "usecase",
                    &uc_id,
                    RdraError::TriggeredUseCaseUnreachable {
                        event: event_id.clone(),
                        usecase: uc_id.clone(),
                    },
                    true,
                );
            }
        }
    }

    // timer が triggers する UseCase がどの BUC にも contains されていない
    for rel in &model.relations {
        let (RelKind::Triggers, NodeRef::Timer(timer_key), NodeRef::UseCase(uc_key)) =
            (&rel.kind, &rel.from, &rel.to)
        else {
            continue;
        };
        let belongs_to_buc = model
            .relations
            .iter()
            .any(|r| r.kind == RelKind::Contains && r.to == NodeRef::UseCase(*uc_key));
        if belongs_to_buc {
            continue;
        }
        let timer_id = model
            .timers
            .get(*timer_key)
            .map(|t| t.id.clone())
            .unwrap_or_default();
        let uc_id = model
            .use_cases
            .get(*uc_key)
            .map(|u| u.id.clone())
            .unwrap_or_default();
        push_model_decl_diagnostic(
            model,
            &mut diags,
            "usecase",
            &uc_id,
            RdraError::TimerTriggeredUseCaseUnreachable {
                timer: timer_id,
                usecase: uc_id.clone(),
            },
            true,
        );
    }

    diags
}

/// API 整合性の診断を生成する。
///
/// - 宣言されたが誰にも invoke されない api → `ApiNeverInvoked` 警告。
/// - invoke されるが entity を操作しない api → `ApiInvokedButNoEntity` 警告。
///
/// `event_diagnostics` と同じパターンで `model.relations` を 1 パス走査する。
/// Duplicate method+path pairs are hard errors (also run from model build).
pub fn api_route_diagnostics(model: &SemanticModel) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut routes: HashMap<(String, String), String> = HashMap::new();
    for (_, api) in model.apis.iter() {
        let (Some(method), Some(path)) = (&api.method, &api.path) else {
            continue;
        };
        let key = (method.to_ascii_uppercase(), path.clone());
        if let Some(other) = routes.get(&key) {
            push_model_decl_diagnostic(
                model,
                &mut diags,
                "api",
                &api.id,
                RdraError::DuplicateApiRoute {
                    api: api.id.clone(),
                    other: other.clone(),
                    method: method.clone(),
                    path: path.clone(),
                },
                false,
            );
        } else {
            routes.insert(key, api.id.clone());
        }
    }
    diags
}

pub fn api_diagnostics(model: &SemanticModel) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    for (ak, api) in model.apis.iter() {
        let invoked = model
            .relations
            .iter()
            .any(|r| r.kind == RelKind::Invokes && r.to == NodeRef::Api(ak));

        if !invoked {
            push_model_decl_diagnostic(
                model,
                &mut diags,
                "api",
                &api.id,
                RdraError::ApiNeverInvoked {
                    api: api.id.clone(),
                },
                true,
            );
            continue; // 未呼出しなら entity 操作の確認は不要
        }

        let operates_entity = model.relations.iter().any(|r| {
            r.from == NodeRef::Api(ak)
                && matches!(
                    r.kind,
                    RelKind::Reads
                        | RelKind::Writes
                        | RelKind::Creates
                        | RelKind::Updates
                        | RelKind::Deletes
                )
        });

        if !operates_entity {
            push_model_decl_diagnostic(
                model,
                &mut diags,
                "api",
                &api.id,
                RdraError::ApiInvokedButNoEntity {
                    api: api.id.clone(),
                },
                true,
            );
        }
    }

    diags
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        model
    }

    const EXT_SRC: &str = r#"
usecase Book "Book"
usecase Notify "Notify"
buc Scheduling "Scheduling"
contains(Scheduling, Book)
contains(Scheduling, Notify)
extsystem Gateway "Gateway"
extsystem Lab "Lab"
event EvBooked "Booked"
event EvResult "Result"
timer Nightly "Nightly"
raises(Book, EvBooked)
triggers(EvBooked, Gateway)
raises(Lab, EvResult)
triggers(EvResult, Notify)
triggers(Nightly, Book)
"#;

    fn warnings(model: &SemanticModel) -> Vec<RdraError> {
        event_diagnostics(model)
            .into_iter()
            .map(|d| d.error)
            .collect()
    }

    #[test]
    fn event_delivered_to_an_extsystem_is_consumed() {
        let model = model_from(EXT_SRC);
        let flows = collect_event_flows(&model);
        let booked = flows
            .iter()
            .find(|f| model.events[f.event].id == "EvBooked")
            .unwrap();
        assert_eq!(booked.delivered_to.len(), 1);
        let result = flows
            .iter()
            .find(|f| model.events[f.event].id == "EvResult")
            .unwrap();
        assert_eq!(result.raising_extsystems().len(), 1);
        assert!(result.raising_usecases().is_empty());
        let w = warnings(&model);
        assert!(
            !w.iter().any(|e| matches!(
                e,
                RdraError::EventNeverConsumed { .. } | RdraError::EventNeverRaised { .. }
            )),
            "{w:?}"
        );
    }

    #[test]
    fn timer_triggered_use_case_outside_any_buc_is_flagged() {
        let model =
            model_from("usecase Orphan \"o\"\ntimer Nightly \"n\"\ntriggers(Nightly, Orphan)\n");
        assert!(
            warnings(&model).iter().any(|e| matches!(
                e,
                RdraError::TimerTriggeredUseCaseUnreachable { timer, usecase }
                    if timer == "Nightly" && usecase == "Orphan"
            )),
            "{:?}",
            warnings(&model)
        );
        let model = model_from(EXT_SRC);
        assert!(!warnings(&model)
            .iter()
            .any(|e| matches!(e, RdraError::TimerTriggeredUseCaseUnreachable { .. })));
        assert_eq!(collect_timer_flows(&model).len(), 1);
    }

    #[test]
    fn timer_cannot_trigger_an_extsystem() {
        let (ast, errs) = parse("timer T \"t\"\nextsystem X \"x\"\ntriggers(T, X)\n");
        assert!(errs.is_empty());
        let (_model, diags) = build_model(&ast);
        assert!(
            diags.iter().any(|d| !d.is_warning
                && matches!(&d.error, RdraError::TypeMismatch { pred, actual, .. }
                    if pred == "triggers" && actual == "timer -> extsystem")),
            "{:?}",
            diags.iter().map(|d| &d.error).collect::<Vec<_>>()
        );
    }

    #[test]
    fn duplicate_api_routes_are_errors() {
        let model_src = r#"
api A "a" method POST path "/x"
api B "b" method POST path "/x"
"#;
        let (ast, errs) = parse(model_src);
        assert!(errs.is_empty());
        let (_model, diags) = build_model(&ast);
        assert!(
            diags.iter().any(|d| matches!(
                &d.error,
                RdraError::DuplicateApiRoute { api, other, .. }
                    if (api == "B" && other == "A") || (api == "A" && other == "B")
            )),
            "expected DuplicateApiRoute, got: {:?}",
            diags.iter().map(|d| &d.error).collect::<Vec<_>>()
        );
    }

    #[test]
    fn collect_event_flows_includes_triggered_bucs() {
        let model = model_from(
            r#"
usecase SignEncounter "Sign Encounter"
buc BucBillingClaims "Billing Claims"
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
triggers(EncounterSigned, BucBillingClaims)
"#,
        );

        let flows = collect_event_flows(&model);
        let flow = flows
            .iter()
            .find(|flow| model.events[flow.event].id == "EncounterSigned")
            .expect("EncounterSigned flow should exist");

        assert_eq!(flow.triggers_bucs.len(), 1);
        assert_eq!(model.bucs[flow.triggers_bucs[0]].id, "BucBillingClaims");
    }

    #[test]
    fn flow_transitions_are_those_of_triggered_usecases() {
        let model = model_from(
            r#"
usecase SignEncounter "Sign Encounter"
usecase SubmitClaim "Submit Claim"
entity Encounter "Encounter" {
  id: Int @pk
  status: Enum(open, signed) @default(open)
}
entity Claim "Claim" {
  id: Int @pk
  status: Enum(draft, submitted) @default(draft)
}
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
triggers(EncounterSigned, SubmitClaim)
transitions(Encounter.status, SignEncounter, open -> signed)
transitions(Claim.status, SubmitClaim, draft -> submitted)
"#,
        );

        let flow = collect_event_flows(&model).pop().unwrap();
        let transitions: Vec<_> = flow
            .transitions
            .iter()
            .map(|st| (model.use_cases[st.usecase].id.as_str(), st.to.as_str()))
            .collect();
        assert_eq!(transitions, vec![("SubmitClaim", "submitted")]);
    }

    #[test]
    fn event_whose_raiser_drives_a_transition_is_still_unconsumed() {
        let model = model_from(
            r#"
usecase SignEncounter "Sign Encounter"
entity Encounter "Encounter" {
  id: Int @pk
  status: Enum(open, signed) @default(open)
}
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
transitions(Encounter.status, SignEncounter, open -> signed)
"#,
        );

        let diags = event_diagnostics(&model);
        assert!(diags
            .iter()
            .any(|diag| matches!(&diag.error, RdraError::EventNeverConsumed { event } if event == "EncounterSigned")));
    }

    #[test]
    fn event_triggering_buc_counts_as_consumed() {
        let model = model_from(
            r#"
usecase SignEncounter "Sign Encounter"
buc BucBillingClaims "Billing Claims"
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
triggers(EncounterSigned, BucBillingClaims)
"#,
        );

        let diags = event_diagnostics(&model);
        assert!(!diags
            .iter()
            .any(|diag| matches!(&diag.error, RdraError::EventNeverConsumed { .. })));
    }

    #[test]
    fn outbox_event_suppresses_never_consumed_warning() {
        let model = model_from(
            r#"
usecase PublishCustomerChanged "Publish Customer Changed"
event CustomerChanged "Customer Changed"
raises(PublishCustomerChanged, CustomerChanged)
outbox(CustomerChanged)
"#,
        );

        let diags = event_diagnostics(&model);
        assert!(!diags
            .iter()
            .any(|diag| matches!(&diag.error, RdraError::EventNeverConsumed { .. })));
    }
}
