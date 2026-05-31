//! イベントフロー集約: raises/triggers/transitions の結合ビュー。
//!
//! `collect_event_flows` が唯一のグラフ走査点。検証・可視化・sets 展開から再利用する。
//! `api_diagnostics` も本モジュールで提供する（同様の 1パス集約パターン）。

use crate::diagnostics::{Diagnostic, RdraError};
use crate::model::{BucKey, EventKey, NodeRef, RelKind, SemanticModel, StateKey, UseCaseKey};
use std::collections::HashMap;

/// イベントを中心とした因果連鎖ファクト。
#[derive(Debug)]
pub struct EventFlow {
    /// このフローのイベントキー。
    pub event: EventKey,
    /// `raises(UseCase, event)` で宣言した UseCase の一覧。
    pub raised_by: Vec<UseCaseKey>,
    /// `triggers(event, UseCase)` で宣言した UseCase の一覧。
    pub triggers_ucs: Vec<UseCaseKey>,
    /// `triggers(event, Buc)` で宣言した BUC の一覧。
    pub triggers_bucs: Vec<BucKey>,
    /// `transitions(event, From, To)` で宣言した状態遷移の一覧。
    pub transitions: Vec<(StateKey, StateKey)>,
}

/// モデル内の全イベントについて `EventFlow` を収集する。
///
/// `model.relations` と `model.state_transitions` を 1 パス走査し、
/// イベントキーをインデックスに集約する。
pub fn collect_event_flows(model: &SemanticModel) -> Vec<EventFlow> {
    let mut map: HashMap<EventKey, EventFlow> = model
        .events
        .keys()
        .map(|ek| {
            (
                ek,
                EventFlow {
                    event: ek,
                    raised_by: Vec::new(),
                    triggers_ucs: Vec::new(),
                    triggers_bucs: Vec::new(),
                    transitions: Vec::new(),
                },
            )
        })
        .collect();

    for rel in &model.relations {
        match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Raises, NodeRef::UseCase(uk), NodeRef::Event(ek)) => {
                if let Some(flow) = map.get_mut(ek) {
                    flow.raised_by.push(*uk);
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

    for st in &model.state_transitions {
        if let (NodeRef::Event(ek), NodeRef::State(from_sk), NodeRef::State(to_sk)) =
            (&st.event, &st.from, &st.to)
        {
            if let Some(flow) = map.get_mut(ek) {
                flow.transitions.push((*from_sk, *to_sk));
            }
        }
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
            diags.push(Diagnostic::warning(RdraError::EventNeverRaised {
                event: event_id.clone(),
            }));
        }

        // raise されているが transition も triggers もしない（行き止まり）
        if !flow.raised_by.is_empty()
            && flow.transitions.is_empty()
            && flow.triggers_ucs.is_empty()
            && flow.triggers_bucs.is_empty()
        {
            diags.push(Diagnostic::warning(RdraError::EventNeverConsumed {
                event: event_id.clone(),
            }));
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
                diags.push(Diagnostic::warning(
                    RdraError::TriggeredUseCaseUnreachable {
                        event: event_id.clone(),
                        usecase: uc_id,
                    },
                ));
            }
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
}

/// API 整合性の診断を生成する。
///
/// - 宣言されたが誰にも invoke されない api → `ApiNeverInvoked` 警告。
/// - invoke されるが entity を操作しない api → `ApiInvokedButNoEntity` 警告。
///
/// `event_diagnostics` と同じパターンで `model.relations` を 1 パス走査する。
pub fn api_diagnostics(model: &SemanticModel) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    for (ak, api) in model.apis.iter() {
        let invoked = model
            .relations
            .iter()
            .any(|r| r.kind == RelKind::Invokes && r.to == NodeRef::Api(ak));

        if !invoked {
            diags.push(Diagnostic::warning(RdraError::ApiNeverInvoked {
                api: api.id.clone(),
            }));
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
            diags.push(Diagnostic::warning(RdraError::ApiInvokedButNoEntity {
                api: api.id.clone(),
            }));
        }
    }

    diags
}
