//! 状態パターン出力エミッタ（table / CSV / JSON）。

use crate::{EmitError, Emitter, Scope, View};
use rdra_ish_core::{
    derive_state_patterns,
    state_pattern::{AbstractValue, AxisKind, EntityStateResult, StateDiag, DEFAULT_PATTERN_CAP},
    SemanticModel,
};

// ── ヘルパー ─────────────────────────────────────────────────────────────────

/// BFS などで `AbstractValue` の表示文字列を取得する。
/// `axes` から Nullable カラムの pg_type を参照して `present:jsonb` 形式にする。
fn display_value(col: &str, val: &AbstractValue, result: &EntityStateResult) -> String {
    if let AbstractValue::Present = val {
        let pg_type = result.axes.iter().find_map(|ax| {
            if ax.column == col {
                if let AxisKind::Nullable { pg_type } = &ax.kind {
                    return pg_type.as_deref();
                }
            }
            None
        });
        return val.display_with_type(pg_type);
    }
    val.display_with_type(None)
}

/// via フィールドを "BucA/UcA, BucB/UcB" 形式に整形する
fn format_via(provenance: &rdra_ish_core::state_pattern::Provenance) -> String {
    if provenance.via.is_empty() {
        return "-".to_string();
    }
    provenance
        .via
        .iter()
        .map(|(buc, uc)| match buc {
            Some(b) => format!("{}/{}", b, uc),
            None => uc.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// 診断メッセージを文字列化する
fn diag_message(d: &StateDiag) -> String {
    match d {
        StateDiag::UnreachableEnumVariant { column, variant } => {
            format!(
                "[warn] column '{}': variant '{}' is unreachable",
                column, variant
            )
        }
        StateDiag::ConflictingEffects { usecase, column } => {
            format!(
                "[warn] usecase '{}': conflicting effects on column '{}' (last-wins)",
                usecase, column
            )
        }
        StateDiag::DoubleModeledEnum { column } => {
            format!("[warn] column '{}': driven by both transitions and sets (transitions takes precedence)", column)
        }
        StateDiag::NoCreationPath => {
            "[info] no creates(...) found; seeded from column defaults".to_string()
        }
        StateDiag::PatternCapReached { cap, bound } => {
            format!(
                "[warn] pattern cap reached: {} patterns generated, theoretical bound is {}",
                cap, bound
            )
        }
        StateDiag::ForbiddenStateViolated {
            conditions,
            pattern_desc,
            correlation_hint,
        } => {
            let mut message = format!(
                "[error] forbidden state reached: ({}) in pattern ({})",
                conditions, pattern_desc
            );
            if let Some(hint) = correlation_hint {
                message.push_str(&format!("; hint: {}", hint));
            }
            message
        }
        StateDiag::InvariantViolated {
            guards,
            requireds,
            pattern_desc,
            flow_order_hint,
        } => {
            let mut message = format!(
                "[error] invariant violated: when ({}) holds, ({}) must also hold — but found pattern ({})",
                guards, requireds, pattern_desc
            );
            if let Some(hint) = flow_order_hint {
                message.push_str(&format!("; hint: {}", hint));
            }
            message
        }
        StateDiag::RequiredStateViolated {
            conditions,
            pattern_desc,
        } => {
            format!(
                "[error] required state missing: ({}) is not satisfied in pattern ({})",
                conditions, pattern_desc
            )
        }
        StateDiag::ExclusiveStateViolated {
            conditions,
            pattern_desc,
        } => {
            format!(
                "[error] exclusive state conditions co-occur: ({}) in pattern ({})",
                conditions, pattern_desc
            )
        }
        StateDiag::CrossForbiddenViolated {
            entities,
            conditions,
            pattern_desc,
            scope_hint,
        } => {
            let mut message = format!(
                "[error] cross-entity forbidden state reached across [{}]: ({}) in patterns ({})",
                entities, conditions, pattern_desc
            );
            if let Some(hint) = scope_hint {
                message.push_str(&format!("; hint: {}", hint));
            }
            message
        }
        StateDiag::CrossInvariantViolated {
            entities,
            guards,
            requireds,
            pattern_desc,
            scope_hint,
        } => {
            let mut message = format!(
                "[error] cross-entity invariant violated across [{}]: when ({}) holds, ({}) must also hold — but found patterns ({})",
                entities, guards, requireds, pattern_desc
            );
            if let Some(hint) = scope_hint {
                message.push_str(&format!("; hint: {}", hint));
            }
            message
        }
        StateDiag::CrossConstraintNotEvaluated {
            entities,
            constraint,
            reason,
        } => {
            format!(
                "[warn] cross-entity constraint not evaluated across [{}]: {} ({})",
                entities, constraint, reason
            )
        }
        StateDiag::TemporalAssertionViolated {
            anchor,
            requireds,
            actual,
        } => {
            format!(
                "[error] temporal assertion violated after '{}': expected ({}) but {}",
                anchor, requireds, actual
            )
        }
        StateDiag::TemporalAssertionNotEvaluated {
            anchor,
            requireds,
            reason,
        } => {
            format!(
                "[warn] temporal assertion after '{}' not evaluated: ({}) ({})",
                anchor, requireds, reason
            )
        }
        StateDiag::QuantifierConstraintNotEvaluated {
            anchor,
            related,
            constraint,
            reason,
        } => {
            format!(
                "[warn] to-many quantifier constraint not evaluated from '{}' to '{}': {} ({})",
                anchor, related, constraint, reason
            )
        }
        StateDiag::UndrivenComparisonProp {
            proposition,
            usage,
            effect,
        } => {
            format!(
                "[warn] comparison proposition '{}' used in {} is not driven by sets(..., <comparison>, true/false): {}",
                proposition, usage, effect
            )
        }
    }
}

// ── Table エミッタ ────────────────────────────────────────────────────────────

pub struct StatePatternTableEmitter {
    pub cap: usize,
}

impl Default for StatePatternTableEmitter {
    fn default() -> Self {
        Self {
            cap: DEFAULT_PATTERN_CAP,
        }
    }
}

impl Emitter for StatePatternTableEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let buc_filter: Vec<String> = match &view.scope {
            Scope::Bucs(ids) => ids.clone(),
            Scope::Whole | Scope::UseCases(_) => vec![],
        };
        let results = derive_state_patterns(model, &buc_filter, self.cap);
        let mut out = String::new();

        for r in &results {
            // ── エンティティヘッダ ────────────────────────────────────────
            out.push_str(&format!("Entity: {} ({})\n", r.entity_id, r.entity_label));

            if r.axes.is_empty() {
                out.push_str("  (no state axes)\n");
                out.push_str(&format!("  reachable: {} / bound: 1\n\n", r.patterns.len()));
                continue;
            }

            // 軸名の表示（ドメイン付き）
            let axis_header: Vec<String> = r
                .axes
                .iter()
                .map(|ax| match &ax.kind {
                    AxisKind::Enum(v) => format!("{}[{}]", ax.column, v.join("|")),
                    AxisKind::Bool => format!("{}[false|true]", ax.column),
                    AxisKind::Nullable { pg_type } => match pg_type {
                        Some(t) => format!("{}[null|present:{}]", ax.column, t),
                        None => format!("{}[null|present]", ax.column),
                    },
                    AxisKind::Proposition { axis_key } => {
                        format!("{}[false|true]", axis_key)
                    }
                })
                .collect();
            out.push_str(&format!("  axes: {}\n\n", axis_header.join(", ")));

            // ── テーブル ────────────────────────────────────────────────
            // カラム幅の計算
            // Proposition 軸はプレフィックス抜きの axis_key を表示名として使う
            let axis_cols: Vec<String> = r
                .axes
                .iter()
                .map(|a| match &a.kind {
                    AxisKind::Proposition { axis_key } => axis_key.clone(),
                    _ => a.column.clone(),
                })
                .collect();
            let mut col_widths: Vec<usize> = axis_cols.iter().map(|c| c.len()).collect();
            let initial_w = "INITIAL".len();
            let terminal_w = "TERMINAL".len();

            // via の最大幅
            let mut via_w = "VIA".len();
            for p in &r.patterns {
                let v = format_via(&p.provenance);
                via_w = via_w.max(v.len());
            }

            // 各パターンの値の最大幅
            for (i, ax) in r.axes.iter().enumerate() {
                let col_w = &mut col_widths[i];
                for p in &r.patterns {
                    if let Some(val) = p.pattern.values.get(&ax.column) {
                        let s = display_value(&ax.column, val, r);
                        *col_w = (*col_w).max(s.len());
                    }
                }
            }

            // ヘッダ行
            let header_parts: Vec<String> = axis_cols
                .iter()
                .enumerate()
                .map(|(i, c)| format!("{:<width$}", c.to_uppercase(), width = col_widths[i]))
                .chain([
                    format!("{:<width$}", "INITIAL", width = initial_w),
                    format!("{:<width$}", "TERMINAL", width = terminal_w),
                    format!("{:<width$}", "VIA", width = via_w),
                ])
                .collect();
            out.push_str(&format!("  {}\n", header_parts.join("  ")));

            // セパレータ
            let sep_parts: Vec<String> = col_widths
                .iter()
                .map(|&w| "\u{2500}".repeat(w))
                .chain([
                    "\u{2500}".repeat(initial_w),
                    "\u{2500}".repeat(terminal_w),
                    "\u{2500}".repeat(via_w),
                ])
                .collect();
            out.push_str(&format!("  {}\n", sep_parts.join("  ")));

            // データ行
            for p in &r.patterns {
                let vals: Vec<String> = r
                    .axes
                    .iter()
                    .enumerate()
                    .map(|(i, ax)| {
                        let v = p
                            .pattern
                            .values
                            .get(&ax.column)
                            .map(|v| display_value(&ax.column, v, r))
                            .unwrap_or_else(|| "?".to_string());
                        format!("{:<width$}", v, width = col_widths[i])
                    })
                    .chain([
                        format!(
                            "{:<width$}",
                            if p.is_initial { "yes" } else { "no" },
                            width = initial_w
                        ),
                        format!(
                            "{:<width$}",
                            if p.is_terminal { "yes" } else { "no" },
                            width = terminal_w
                        ),
                        format_via(&p.provenance),
                    ])
                    .collect();
                out.push_str(&format!("  {}\n", vals.join("  ")));
            }

            // フッタ
            let bound = compute_bound_str(r);
            out.push_str(&format!(
                "\n  reachable: {}{}/ bound: {}\n",
                r.patterns.len(),
                if r.truncated { " (truncated) " } else { " " },
                bound
            ));

            // 診断
            if !r.diagnostics.is_empty() {
                out.push_str("  diagnostics:\n");
                for d in &r.diagnostics {
                    out.push_str(&format!("    {}\n", diag_message(d)));
                }
            }

            out.push('\n');
        }

        Ok(out)
    }
}

// ── CSV エミッタ（long-form） ─────────────────────────────────────────────────

pub struct StatePatternCsvEmitter {
    pub cap: usize,
}

impl Default for StatePatternCsvEmitter {
    fn default() -> Self {
        Self {
            cap: DEFAULT_PATTERN_CAP,
        }
    }
}

impl Emitter for StatePatternCsvEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let buc_filter: Vec<String> = match &view.scope {
            Scope::Bucs(ids) => ids.clone(),
            Scope::Whole | Scope::UseCases(_) => vec![],
        };
        let results = derive_state_patterns(model, &buc_filter, self.cap);
        let mut wtr = csv::Writer::from_writer(vec![]);
        wtr.write_record([
            "entity_id",
            "entity_label",
            "pattern_idx",
            "column",
            "value",
            "is_initial",
            "is_terminal",
            "via",
        ])?;

        for r in &results {
            for (pi, p) in r.patterns.iter().enumerate() {
                let is_initial = if p.is_initial { "true" } else { "false" };
                let is_terminal = if p.is_terminal { "true" } else { "false" };
                let via = format_via(&p.provenance);

                if r.axes.is_empty() {
                    // 軸なし: 1行出力
                    wtr.write_record([
                        &r.entity_id,
                        &r.entity_label,
                        &pi.to_string(),
                        "(no axes)",
                        "",
                        is_initial,
                        is_terminal,
                        &via,
                    ])?;
                } else {
                    for ax in &r.axes {
                        let val = p
                            .pattern
                            .values
                            .get(&ax.column)
                            .map(|v| display_value(&ax.column, v, r))
                            .unwrap_or_default();
                        wtr.write_record([
                            &r.entity_id,
                            &r.entity_label,
                            &pi.to_string(),
                            &ax.column,
                            &val,
                            is_initial,
                            is_terminal,
                            &via,
                        ])?;
                    }
                }
            }
        }

        let data = wtr
            .into_inner()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(String::from_utf8(data).unwrap_or_default())
    }
}

// ── JSON エミッタ ─────────────────────────────────────────────────────────────

pub struct StatePatternJsonEmitter {
    pub cap: usize,
}

impl Default for StatePatternJsonEmitter {
    fn default() -> Self {
        Self {
            cap: DEFAULT_PATTERN_CAP,
        }
    }
}

impl Emitter for StatePatternJsonEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let buc_filter: Vec<String> = match &view.scope {
            Scope::Bucs(ids) => ids.clone(),
            Scope::Whole | Scope::UseCases(_) => vec![],
        };
        let results = derive_state_patterns(model, &buc_filter, self.cap);

        // シリアライズ可能な中間表現に変換
        let serializable: Vec<serde_json::Value> =
            results.iter().map(entity_result_to_json).collect();

        let json = serde_json::to_string_pretty(&serializable)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(json + "\n")
    }
}

fn entity_result_to_json(r: &EntityStateResult) -> serde_json::Value {
    let axes: Vec<serde_json::Value> = r
        .axes
        .iter()
        .map(|ax| {
            let domain: serde_json::Value = match &ax.kind {
                AxisKind::Enum(v) => serde_json::json!(v),
                AxisKind::Bool => serde_json::json!(["false", "true"]),
                AxisKind::Nullable { pg_type } => match pg_type {
                    Some(t) => serde_json::json!(["null", format!("present:{}", t)]),
                    None => serde_json::json!(["null", "present"]),
                },
                AxisKind::Proposition { .. } => serde_json::json!(["false", "true"]),
            };
            serde_json::json!({
                "column": ax.column,
                "domain": domain,
            })
        })
        .collect();

    let patterns: Vec<serde_json::Value> = r
        .patterns
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let values: serde_json::Map<String, serde_json::Value> = r
                .axes
                .iter()
                .filter_map(|ax| {
                    p.pattern.values.get(&ax.column).map(|v| {
                        (
                            ax.column.clone(),
                            serde_json::json!(display_value(&ax.column, v, r)),
                        )
                    })
                })
                .collect();
            serde_json::json!({
                "idx": i,
                "values": values,
                "is_initial": p.is_initial,
                "is_terminal": p.is_terminal,
                "via": p.provenance.via.iter().map(|(b, u)| {
                    match b {
                        Some(buc) => format!("{}/{}", buc, u),
                        None => u.clone(),
                    }
                }).collect::<Vec<_>>(),
            })
        })
        .collect();

    let diagnostics: Vec<serde_json::Value> = r
        .diagnostics
        .iter()
        .map(|d| serde_json::json!(diag_message(d)))
        .collect();

    serde_json::json!({
        "entity_id": r.entity_id,
        "entity_label": r.entity_label,
        "axes": axes,
        "patterns": patterns,
        "reachable": r.patterns.len(),
        "truncated": r.truncated,
        "no_creation_path": r.no_creation_path,
        "diagnostics": diagnostics,
    })
}

// ── ヘルパー ─────────────────────────────────────────────────────────────────

fn compute_bound_str(r: &EntityStateResult) -> String {
    let bound: usize = r.axes.iter().fold(1usize, |acc, ax| {
        let f = match &ax.kind {
            AxisKind::Enum(v) => v.len(),
            AxisKind::Bool => 2,
            AxisKind::Nullable { .. } => 2,
            AxisKind::Proposition { .. } => 2,
        };
        acc.saturating_mul(f)
    });
    bound.to_string()
}

// ── テスト ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::analysis::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, errors) = parse(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let (model, diags) = build_model(&ast);
        let errs: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errs.is_empty(), "model errors: {:?}", errs);
        model
    }

    const ORDER_SRC: &str = r#"
entity Order "注文" {
  id:           Int @pk
  status:       Enum(pending, paid, shipped, delivered, cancelled) @default(pending)
  delivered_at: DateTime @null
}
usecase PlaceOrder  "注文確定"
usecase CapturePay  "決済確定"
usecase ShipOrder   "発送"
usecase DeliverOrder "配達確認"
usecase CancelOrder "注文キャンセル"
event EvCapture  "決済確定"
event EvShip     "発送開始"
event EvDeliver  "配達確認"
event EvCancel   "注文キャンセル"
creates(PlaceOrder, Order)
updates(CapturePay,   Order)
updates(ShipOrder,    Order)
updates(DeliverOrder, Order)
updates(CancelOrder,  Order)
raises(CapturePay,   EvCapture)
raises(ShipOrder,    EvShip)
raises(DeliverOrder, EvDeliver)
raises(CancelOrder,  EvCancel)
state Pending   "注文受付"
state Paid      "決済完了"
state Shipped   "発送済"
state Delivered "配達完了"
state Cancelled "キャンセル"
transitions(EvCapture, Pending,  Paid)
transitions(EvShip,    Paid,     Shipped)
transitions(EvDeliver, Shipped,  Delivered)
transitions(EvCancel,  Pending,  Cancelled)
sets(usecase::DeliverOrder, Order, "delivered_at", "timestamptz")
"#;

    #[test]
    fn test_order_states_table_snapshot() {
        let model = model_from(ORDER_SRC);
        let view = View::whole();
        let result = StatePatternTableEmitter::default()
            .emit(&model, &view)
            .unwrap();
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_order_states_csv_snapshot() {
        let model = model_from(ORDER_SRC);
        let view = View::whole();
        let result = StatePatternCsvEmitter::default()
            .emit(&model, &view)
            .unwrap();
        insta::assert_snapshot!(result);
    }

    #[test]
    fn test_order_states_json_snapshot() {
        let model = model_from(ORDER_SRC);
        let view = View::whole();
        let result = StatePatternJsonEmitter::default()
            .emit(&model, &view)
            .unwrap();
        insta::assert_snapshot!(result);
    }
}
