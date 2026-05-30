//! イベントフロー集約: raises/triggers/transitions の結合ビュー。
//!
//! `collect_event_flows` が唯一のグラフ走査点。検証・可視化・sets 展開から再利用する。

use crate::diagnostics::{Diagnostic, RdraError};
use crate::model::{EventKey, NodeRef, RelKind, SemanticModel, StateKey, UseCaseKey};
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

    let mut result: Vec<EventFlow> = map.into_iter().map(|(_, v)| v).collect();
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
        {
            diags.push(Diagnostic::warning(RdraError::EventNeverConsumed {
                event: event_id.clone(),
            }));
        }

        // triggers 先 UC がどの BUC にも contains されていない
        for &uc_key in &flow.triggers_ucs {
            let belongs_to_buc = model.relations.iter().any(|r| {
                r.kind == RelKind::Contains && r.to == NodeRef::UseCase(uc_key)
            });
            if !belongs_to_buc {
                let uc_id = model
                    .use_cases
                    .get(uc_key)
                    .map(|u| u.id.clone())
                    .unwrap_or_default();
                diags.push(Diagnostic::warning(RdraError::TriggeredUseCaseUnreachable {
                    event: event_id.clone(),
                    usecase: uc_id,
                }));
            }
        }
    }

    diags
}
