//! 状態パターン出力エミッタ（table / CSV / JSON）。

use crate::{EmitError, Emitter, Scope, View};
use rdra_ish_core::{
    lifecycle::{
        derive_states, AbstractValue, AxisKind, Caps, EntityStateResult, InvKind, InvText,
        Provenance, Scope as SystemScope, StateDiag,
    },
    SemanticModel,
};

// ── ヘルパー ─────────────────────────────────────────────────────────────────

/// The entity views `states` prints for the view's BUC scope.
fn derive_views(model: &SemanticModel, view: &View, cap: usize) -> Vec<EntityStateResult> {
    let scope = match &view.scope {
        Scope::Bucs(ids) => SystemScope::from_bucs(ids),
        Scope::Whole | Scope::UseCases(_) => SystemScope::All,
    };
    derive_states(model, &scope, cap).views
}

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
fn format_via(provenance: &Provenance) -> String {
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

/// The values an axis takes, as the JSON `domain` lists them.
fn axis_domain(kind: &AxisKind) -> Vec<String> {
    let ints = |max: u8| (0..=max).map(|n| n.to_string());
    match kind {
        AxisKind::Enum(v) => v.clone(),
        AxisKind::Bool | AxisKind::Proposition { .. } => vec!["false".into(), "true".into()],
        AxisKind::Nullable { pg_type } => vec![
            "null".into(),
            match pg_type {
                Some(t) => format!("present:{t}"),
                None => "present".into(),
            },
        ],
        AxisKind::Int { max } => ints(*max).collect(),
        AxisKind::NullableInt { max } => std::iter::once("null".to_string())
            .chain(ints(*max))
            .collect(),
    }
}

/// A constraint violation in the wording `states` has always used for its kind.
fn violation_message(kind: &InvKind, text: &InvText, p: &str) -> Option<String> {
    Some(match (kind, text) {
        (InvKind::Forbidden, InvText::Conditions(c)) => {
            format!("forbidden state reached: ({c}) in pattern ({p})")
        }
        (InvKind::Required, InvText::Conditions(c)) => {
            format!("required state missing: ({c}) is not satisfied in pattern ({p})")
        }
        (InvKind::Exclusive, InvText::Conditions(c)) => {
            format!("exclusive state conditions co-occur: ({c}) in pattern ({p})")
        }
        (InvKind::Invariant, InvText::WhenThen { guards, requireds }) => format!(
            "invariant violated: when ({guards}) holds, ({requireds}) must also hold — but found pattern ({p})"
        ),
        (
            InvKind::CrossForbidden,
            InvText::Cross {
                entities,
                conditions,
            },
        ) => format!(
            "multi-entity forbidden state reached across [{entities}]: ({conditions}) in patterns ({p})"
        ),
        (
            InvKind::CrossInvariant,
            InvText::CrossWhenThen {
                entities,
                guards,
                requireds,
            },
        ) => format!(
            "multi-entity invariant violated across [{entities}]: when ({guards}) holds, ({requireds}) must also hold — but found patterns ({p})"
        ),
        (
            InvKind::Quantifier,
            InvText::Quantifier {
                anchor,
                related,
                constraint,
            },
        ) => format!(
            "to-many quantifier constraint violated from '{anchor}' to '{related}': {constraint} in patterns ({p})"
        ),
        _ => return None,
    })
}

/// 診断メッセージを文字列化する（severity は `StateDiag::severity` と共有）
fn diag_message(d: &StateDiag) -> String {
    let label = d.severity().as_label();
    let body = match d {
        StateDiag::UnreachableEnumVariant { column, variant } => {
            format!("column '{}': variant '{}' is unreachable", column, variant)
        }
        StateDiag::ConflictingEffects {
            usecase, column, ..
        } => {
            format!(
                "usecase '{}': conflicting effects on column '{}' (last-wins)",
                usecase, column
            )
        }
        StateDiag::DoubleModeledEnum { column, .. } => {
            format!(
                "column '{}': driven by both transitions and sets (transitions takes precedence)",
                column
            )
        }
        StateDiag::NoCreationPath => {
            "no creates(...) found; seeded from column defaults".to_string()
        }
        StateDiag::PatternCapReached { cap, bound } => {
            format!(
                "pattern cap reached: {} patterns generated, theoretical bound is {} (constraints on truncated set are not sound)",
                cap, bound
            )
        }
        StateDiag::ConstraintViolated {
            kind,
            text,
            pattern_desc,
            ..
        } => violation_message(kind, text, pattern_desc).unwrap_or_else(|| d.to_string()),
        StateDiag::AfterAssertViolated {
            anchor,
            requireds,
            actual,
            ..
        } => format!(
            "temporal assertion violated after '{anchor}': expected ({requireds}) but found {actual}"
        ),
        StateDiag::StateCapReached { .. }
        | StateDiag::ContradictoryDrivenComparison { .. }
        | StateDiag::AssertOnInertUseCase { .. }
        | StateDiag::IntLiteralOutOfBounds { .. }
        | StateDiag::UnbindableScope { .. }
        | StateDiag::UnbindableCondition { .. }
        | StateDiag::UnwrittenConstrainedVar { .. }
        | StateDiag::UnsupportedComparison { .. }
        | StateDiag::UnresolvedPropertyAtom { .. } => d.to_string(),
    };
    format!("[{label}] {body}")
}

// ── Table エミッタ ────────────────────────────────────────────────────────────

pub struct StatePatternTableEmitter {
    pub cap: usize,
}

impl Default for StatePatternTableEmitter {
    fn default() -> Self {
        Self {
            cap: Caps::DEFAULT_PATTERNS,
        }
    }
}

impl Emitter for StatePatternTableEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        Ok(render_table(&derive_views(model, view, self.cap)))
    }
}

/// One block per entity: its axes, a row per pattern, the reachable count, and diagnostics.
pub fn render_table(results: &[EntityStateResult]) -> String {
    let mut out = String::new();

    for r in results {
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
                AxisKind::Proposition { axis_key } => format!("{}[false|true]", axis_key),
                AxisKind::Int { max } => format!("{}[0..{}]", ax.column, max),
                AxisKind::NullableInt { max } => format!("{}[null|0..{}]", ax.column, max),
                kind => format!("{}[{}]", ax.column, axis_domain(kind).join("|")),
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
        out.push_str(&format!(
            "\n  reachable: {}{}/ bound: {}\n",
            r.patterns.len(),
            if r.truncated { " (truncated) " } else { " " },
            r.bound()
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

    out
}

// ── CSV エミッタ（long-form） ─────────────────────────────────────────────────

pub struct StatePatternCsvEmitter {
    pub cap: usize,
}

impl Default for StatePatternCsvEmitter {
    fn default() -> Self {
        Self {
            cap: Caps::DEFAULT_PATTERNS,
        }
    }
}

impl Emitter for StatePatternCsvEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        render_csv(&derive_views(model, view, self.cap))
    }
}

/// One row per (entity, pattern, axis).
pub fn render_csv(results: &[EntityStateResult]) -> Result<String, EmitError> {
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

    for r in results {
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

// ── JSON エミッタ ─────────────────────────────────────────────────────────────

pub struct StatePatternJsonEmitter {
    pub cap: usize,
}

impl Default for StatePatternJsonEmitter {
    fn default() -> Self {
        Self {
            cap: Caps::DEFAULT_PATTERNS,
        }
    }
}

impl Emitter for StatePatternJsonEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        render_json(&derive_views(model, view, self.cap))
    }
}

/// A pretty-printed array with one object per entity.
pub fn render_json(results: &[EntityStateResult]) -> Result<String, EmitError> {
    let serializable: Vec<serde_json::Value> = results.iter().map(entity_result_to_json).collect();
    let json = serde_json::to_string_pretty(&serializable)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(json + "\n")
}

fn entity_result_to_json(r: &EntityStateResult) -> serde_json::Value {
    let axes: Vec<serde_json::Value> = r
        .axes
        .iter()
        .map(|ax| {
            serde_json::json!({
                "column": ax.column,
                "domain": axis_domain(&ax.kind),
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
creates(PlaceOrder, Order)
updates(CapturePay,   Order)
updates(ShipOrder,    Order)
updates(DeliverOrder, Order)
updates(CancelOrder,  Order)
transitions(Order.status, CapturePay, pending -> paid)
transitions(Order.status, ShipOrder, paid -> shipped)
transitions(Order.status, DeliverOrder, shipped -> delivered)
transitions(Order.status, CancelOrder, pending -> cancelled)
sets(usecase::DeliverOrder, Order, delivered_at == present)
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

    #[test]
    fn pattern_cap_reached_is_error() {
        let msg = diag_message(&StateDiag::PatternCapReached { cap: 1, bound: 48 });
        assert!(
            msg.starts_with("[error]"),
            "truncation must be fail-closed, got: {msg}"
        );
        assert!(msg.contains("not sound"));
    }

    #[test]
    fn int_columns_print_their_values_and_domain() {
        let model = model_from(
            r#"
entity Item "商品" {
  id:      Int @pk
  status:  Enum(active, archived) @default(active)
  stock:   Int @default(1)
}
usecase Add "登録"
usecase Restock "補充"
creates(Add, Item)
updates(Restock, Item)
sets(Restock, Item, stock == 3)
forbidden(Item, stock < 1)
"#,
        );
        let table = StatePatternTableEmitter::default()
            .emit(&model, &View::whole())
            .unwrap();
        assert!(
            table.contains("axes: status[active|archived], stock[0..5]"),
            "{table}"
        );
        assert!(table.contains("reachable: 2 / bound: 12"), "{table}");
        let json = StatePatternJsonEmitter::default()
            .emit(&model, &View::whole())
            .unwrap();
        assert!(json.contains(r#""stock": "3""#), "{json}");
    }
}
