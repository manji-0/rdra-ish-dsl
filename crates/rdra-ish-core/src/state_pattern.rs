//! BUC パターンから各 entity の取り得る状態パターンを導出するモジュール。
//!
//! 「状態パターン」とは、entity の *state-defining columns*
//! （Enum カラム・Bool カラム・Nullable カラム）の抽象値の組み合わせ。
//! 有限直積空間上の BFS で到達可能なパターン集合を求める。

use crate::model::{
    ColumnEffect, ColumnType, ComparisonProp, CrossCmpRhs, CrossComparisonProp,
    CrossConstraintScope, CrossEntityCondition, CrossEntityInvariant, CrossForbiddenConstraint,
    EffectValue, EntityKey, ModelColumn, NodeRef, QualifiedModelColumnRef, QuantifierConstraint,
    QuantifierKind, RelKind, SemanticModel, StateKey, TemporalAssertion, UseCaseKey,
};
use crate::resolver::reachable_from_bucs;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

/// 比較命題軸の column キーに付けるプレフィックス。
/// 実カラム名との衝突を防ぐために使用する。
/// 例: `stock<selling` → `__cmp:stock<selling`
const PROP_COL_PREFIX: &str = "__cmp:";

// ── デフォルト上限値 ─────────────────────────────────────────────────────────

/// entity ごとのパターン数の上限（デフォルト）。
/// 上限を超えた場合は `EntityStateResult.truncated = true` となる。
pub const DEFAULT_PATTERN_CAP: usize = 256;
const CROSS_PATTERN_COMBO_CAP: usize = 4096;
const CROSS_VIOLATION_DIAG_CAP: usize = 16;

// ── 状態軸 ───────────────────────────────────────────────────────────────────

/// 状態を定義する軸の種類
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AxisKind {
    /// Enum カラム: 宣言済みバリアントが状態の取り得る値
    Enum(Vec<String>),
    /// Bool カラム: {false, true}
    Bool,
    /// Nullable カラム: {null, present}
    Nullable {
        /// `sets(...)` で TypedPresent が宣言された場合の PG 型名（表示用）
        pg_type: Option<String>,
    },
    /// 比較命題の派生 Bool 軸（例: `stock < selling`, `expired_at < now`）。
    /// 既存 Bool 軸と同型だが、実カラムではなく比較式に由来する。
    /// `sets(origin, entity, <expr>, true/false)` によって真偽が駆動される。
    /// デフォルト値は `Bool(false)`。
    Proposition {
        /// 比較命題の正規化キー（例: `"stock<selling"`）。
        /// 表示と軸照合に使用する。
        axis_key: String,
    },
}

/// entity の状態を定義する 1 カラム分の軸
#[derive(Debug, Clone)]
pub struct StateAxis {
    pub column: String,
    pub kind: AxisKind,
}

// ── 抽象値 ───────────────────────────────────────────────────────────────────

/// 状態パターン内での抽象値
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AbstractValue {
    Enum(String),
    Bool(bool),
    Present,
    Null,
}

impl AbstractValue {
    /// `EffectValue` を到達判定用抽象値に変換（TypedPresent → Present）
    pub fn from_effect(v: &EffectValue) -> Self {
        match v {
            EffectValue::EnumVariant(s) => AbstractValue::Enum(s.clone()),
            EffectValue::Bool(b) => AbstractValue::Bool(*b),
            EffectValue::Present | EffectValue::TypedPresent(_) => AbstractValue::Present,
            EffectValue::Null => AbstractValue::Null,
        }
    }

    /// 表示文字列（PG 型名を持つ Nullable の present は型を付加）
    pub fn display_with_type(&self, pg_type: Option<&str>) -> String {
        match self {
            AbstractValue::Present => {
                if let Some(t) = pg_type {
                    format!("present:{}", t)
                } else {
                    "present".to_string()
                }
            }
            AbstractValue::Null => "null".to_string(),
            AbstractValue::Enum(s) => s.clone(),
            AbstractValue::Bool(b) => b.to_string(),
        }
    }
}

// ── 状態パターン ─────────────────────────────────────────────────────────────

/// entity の到達可能な状態パターン（全軸の総割り当て）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StatePattern {
    /// カラム名 → 抽象値（BTreeMap で常に同一順序・ハッシュが確定）
    pub values: BTreeMap<String, AbstractValue>,
}

impl StatePattern {
    /// 指定カラムに効果を適用した新パターンを返す
    fn apply_effects(&self, effects: &[(String, AbstractValue)]) -> Self {
        let mut values = self.values.clone();
        for (col, val) in effects {
            if values.contains_key(col) {
                values.insert(col.clone(), val.clone());
            }
        }
        StatePattern { values }
    }
}

// ── provenance（来歴） ──────────────────────────────────────────────────────

/// パターンへの到達経路に関与した (BUC id, usecase id) の組
#[derive(Debug, Clone, Default)]
pub struct Provenance {
    pub via: BTreeSet<(Option<String>, String)>,
}

// ── 内部演算型 ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum OpKind {
    Create,
    Update,
    Delete,
}

/// ガード条件: カラム `column` が `value` と等しい場合に演算を適用可能
#[derive(Debug, Clone)]
struct AxisConstraint {
    column: String,
    value: AbstractValue,
}

/// 演算（usecase が entity に対して行う状態変化）
#[derive(Debug, Clone)]
struct Operation {
    usecase_id: String,
    buc_id: Option<String>,
    op_kind: OpKind,
    guard: Vec<AxisConstraint>,
    effects: Vec<(String, AbstractValue)>,
}

// ── 結果型 ───────────────────────────────────────────────────────────────────

/// 導出後の単一パターンエントリ
#[derive(Debug, Clone)]
pub struct ReachablePattern {
    pub pattern: StatePattern,
    /// creates によって生成されたシードパターンか
    pub is_initial: bool,
    /// これ以上の遷移が無い（自然な終端、または deletes の起点）
    pub is_terminal: bool,
    pub provenance: Provenance,
}

/// 状態パターン導出時の診断
#[derive(Debug, Clone)]
pub enum StateDiag {
    /// 宣言または transitions のターゲットではあるが到達不能な Enum バリアント
    UnreachableEnumVariant { column: String, variant: String },
    /// 同一演算が同一カラムに矛盾する効果を持つ（last-wins で解決）
    ConflictingEffects { usecase: String, column: String },
    /// transitions と sets の両方が同一 enum 軸を駆動している（transitions 優先）
    DoubleModeledEnum { column: String },
    /// entity に creates がなく defaults からシードした
    NoCreationPath,
    /// cap に達してパターン探索を打ち切った
    PatternCapReached { cap: usize, bound: usize },
    /// `forbidden(Entity, (col, val), ...)` で禁止された状態に到達可能
    ForbiddenStateViolated {
        /// 禁止条件を "col=val AND col=val" 形式に整形した文字列
        conditions: String,
        pattern_desc: String,
        correlation_hint: Option<String>,
    },
    /// `invariant(Entity).when(...).then(...)` の不変条件を違反する到達可能な状態
    InvariantViolated {
        /// ガード条件を "col=val AND col=val" 形式に整形した文字列
        guards: String,
        /// 必要条件を "col=val AND col=val" 形式に整形した文字列
        requireds: String,
        pattern_desc: String,
        flow_order_hint: Option<String>,
    },
    /// `required(Entity, ...)` の常時成立条件を満たさない到達可能な状態
    RequiredStateViolated {
        conditions: String,
        pattern_desc: String,
    },
    /// `exclusive(Entity, ...)` の相互排他条件が同時成立した到達可能な状態
    ExclusiveStateViolated {
        conditions: String,
        pattern_desc: String,
    },
    /// `cross_forbidden(...)` で禁止された複数 entity の状態組合せに到達可能
    CrossForbiddenViolated {
        entities: String,
        conditions: String,
        pattern_desc: String,
        scope_hint: Option<String>,
    },
    /// `cross_invariant(...).when(...).then(...)` が複数 entity の状態組合せで違反
    CrossInvariantViolated {
        entities: String,
        guards: String,
        requireds: String,
        pattern_desc: String,
        scope_hint: Option<String>,
    },
    /// クロスエンティティ制約が per-entity state-pattern では完全評価できない
    CrossConstraintNotEvaluated {
        entities: String,
        constraint: String,
        reason: String,
    },
    /// `after(UseCase).assert(...)` がアンカー usecase の即時効果で満たされない
    TemporalAssertionViolated {
        anchor: String,
        requireds: String,
        actual: String,
    },
    /// 時相アンカー制約が現在の即時効果モデルでは評価できない
    TemporalAssertionNotEvaluated {
        anchor: String,
        requireds: String,
        reason: String,
    },
    /// to-many 量化制約が現在の抽象状態パターンでは評価できない
    QuantifierConstraintNotEvaluated {
        anchor: String,
        related: String,
        constraint: String,
        reason: String,
    },
}

/// entity 単位の状態パターン導出結果
#[derive(Debug, Clone)]
pub struct EntityStateResult {
    pub entity_id: String,
    pub entity_label: String,
    /// 状態軸の一覧（宣言順）
    pub axes: Vec<StateAxis>,
    /// 到達可能なパターン（挿入順）
    pub patterns: Vec<ReachablePattern>,
    /// cap に達して探索を打ち切った
    pub truncated: bool,
    /// creates が無く defaults からシードした
    pub no_creation_path: bool,
    pub diagnostics: Vec<StateDiag>,
}

// ── 公開 API ─────────────────────────────────────────────────────────────────

/// 全 entity（または buc_filter で絞った範囲）の到達可能な状態パターンを導出する。
///
/// `buc_filter` が空の場合は全 BUC を対象とする。
/// `cap` はエンティティ単位のパターン数上限（`DEFAULT_PATTERN_CAP` を推奨）。
pub fn derive_state_patterns(
    model: &SemanticModel,
    buc_filter: &[String],
    cap: usize,
) -> Vec<EntityStateResult> {
    // BUC 絞り込み: filter が空なら全ノードが対象
    let buc_reachable: Option<HashSet<NodeRef>> = if buc_filter.is_empty() {
        None
    } else {
        Some(reachable_from_bucs(model, buc_filter))
    };

    // BUC id → usecase を contains から引く
    let buc_of_usecase = build_buc_of_usecase(model);

    // 全 entity を処理
    let mut results = Vec::new();
    let mut entity_keys: Vec<EntityKey> = model.entities.keys().collect();
    // id 順にソートして出力を安定させる
    entity_keys.sort_by(|a, b| model.entities[*a].id.cmp(&model.entities[*b].id));

    for &ek in &entity_keys {
        let result = derive_for_entity(model, ek, &buc_of_usecase, buc_reachable.as_ref(), cap);
        results.push(result);
    }

    let result_index: HashMap<EntityKey, usize> = entity_keys
        .iter()
        .enumerate()
        .map(|(idx, ek)| (*ek, idx))
        .collect();
    check_cross_entity_constraints(model, &mut results, &result_index);
    check_temporal_assertions(model, &mut results, &result_index);
    check_quantifier_constraints(model, &mut results, &result_index);

    results
}

// ── 内部実装 ─────────────────────────────────────────────────────────────────

/// usecase key → BUC id のマップを `contains(Buc, UseCase)` から構築
fn build_buc_of_usecase(model: &SemanticModel) -> HashMap<UseCaseKey, String> {
    let mut map: HashMap<UseCaseKey, String> = HashMap::new();
    for rel in &model.relations {
        if rel.kind == RelKind::Contains {
            if let (NodeRef::Buc(bk), NodeRef::UseCase(uk)) = (&rel.from, &rel.to) {
                let buc_id = model.bucs[*bk].id.clone();
                map.entry(*uk).or_insert(buc_id);
            }
        }
    }
    map
}

/// entity の Enum カラムと state 集合のマッピングを構築する。
/// State id を小文字化して Enum バリアントと照合し、
/// `status` カラムに対応する State キーの集合を返す。
/// 戻り値: (状態を表すカラム名, StateKey → バリアント文字列 のマップ)
fn link_states_to_enum(
    model: &SemanticModel,
    ek: EntityKey,
) -> Option<(String, HashMap<StateKey, String>)> {
    let entity = &model.entities[ek];

    // 全 State ノードの id 集合（lowercase）
    let state_ids_lower: HashSet<String> =
        model.states.values().map(|s| s.id.to_lowercase()).collect();

    // entity の Enum カラムで、バリアント集合が state ids と交差するものを探す
    for col in &entity.columns {
        if let ColumnType::Enum(variants) = &col.col_type {
            let variants_lower: Vec<String> = variants.iter().map(|v| v.to_lowercase()).collect();
            let matching = variants_lower
                .iter()
                .filter(|v| state_ids_lower.contains(*v))
                .count();
            // バリアントの半数以上が state と一致 → このカラムが status 軸
            if matching * 2 >= variants.len() && matching > 0 {
                // StateKey → variant (lowercase) マップを構築
                let mut map: HashMap<StateKey, String> = HashMap::new();
                for (sk, state) in model.states.iter() {
                    let lower = state.id.to_lowercase();
                    if variants_lower.contains(&lower) {
                        map.insert(sk, lower);
                    }
                }
                return Some((col.name.clone(), map));
            }
        }
    }
    None
}

/// 比較命題の StatePattern 内でのキー（実カラムとの衝突を避けるプレフィックス付き）。
/// 例: `ComparisonProp { lhs="stock", op=Lt, rhs=Column("selling") }` → `"__cmp:stock<selling"`
pub fn prop_col_key(prop: &ComparisonProp) -> String {
    format!("{}{}", PROP_COL_PREFIX, prop.axis_key())
}

/// entity ごとの状態軸を特定する。
/// `proposition_props` には、この entity の制約述語と `sets` で参照される比較命題
/// （重複なし）を渡す。各命題は `AxisKind::Proposition` 軸として追加される。
fn identify_axes(
    entity_cols: &[ModelColumn],
    column_effects: &[&ColumnEffect],
    proposition_props: &[ComparisonProp],
) -> Vec<StateAxis> {
    let mut axes = Vec::new();

    // TypedPresent の型名を effects から収集（カラム名 → PG 型名）
    let mut pg_type_map: HashMap<String, String> = HashMap::new();
    for eff in column_effects {
        if let EffectValue::TypedPresent(t) = &eff.value {
            pg_type_map
                .entry(eff.column.clone())
                .or_insert_with(|| t.clone());
        }
    }

    for col in entity_cols {
        match &col.col_type {
            ColumnType::Enum(variants) => {
                axes.push(StateAxis {
                    column: col.name.clone(),
                    kind: AxisKind::Enum(variants.clone()),
                });
            }
            ColumnType::Bool => {
                axes.push(StateAxis {
                    column: col.name.clone(),
                    kind: AxisKind::Bool,
                });
            }
            _ => {
                if col.is_nullable {
                    let pg_type = pg_type_map.get(&col.name).cloned();
                    axes.push(StateAxis {
                        column: col.name.clone(),
                        kind: AxisKind::Nullable { pg_type },
                    });
                }
                // 非nullable, 非Enum, 非Bool は状態軸にしない
            }
        }
    }

    // 比較命題軸を追加（重複 axis_key は1軸のみ）
    let mut seen_keys: HashSet<String> = HashSet::new();
    for prop in proposition_props {
        let key = prop.axis_key();
        if seen_keys.insert(key.clone()) {
            axes.push(StateAxis {
                column: prop_col_key(prop),
                kind: AxisKind::Proposition { axis_key: key },
            });
        }
    }

    axes
}

/// `@default` またはフォールバックから軸の初期値を決定する
fn default_value_for_axis(col: &ModelColumn) -> AbstractValue {
    match &col.col_type {
        ColumnType::Enum(variants) => {
            // @default がある場合はそのバリアント、なければ先頭バリアント
            if let Some(d) = &col.default_val {
                if variants.contains(d) {
                    return AbstractValue::Enum(d.clone());
                }
            }
            AbstractValue::Enum(variants[0].clone())
        }
        ColumnType::Bool => {
            if let Some(d) = &col.default_val {
                return AbstractValue::Bool(d == "true");
            }
            AbstractValue::Bool(false)
        }
        _ => {
            // Nullable カラム
            if let Some(d) = &col.default_val {
                if d != "null" {
                    return AbstractValue::Present;
                }
            }
            AbstractValue::Null
        }
    }
}

/// `contains(Buc, UseCase)` + `raises(UseCase, Event)` + `state_transitions`
/// から、entity の status 列に対する Update 演算を構築する。
/// 返り値: (state_column_name, Vec<Operation>) — status 列が無い場合は None
fn build_status_update_ops(
    model: &SemanticModel,
    ek: EntityKey,
    buc_of_usecase: &HashMap<UseCaseKey, String>,
    buc_reachable: Option<&HashSet<NodeRef>>,
    axes: &[StateAxis],
    diags: &mut Vec<StateDiag>,
    effects_for_entity: &[&ColumnEffect],
) -> (Option<String>, Vec<Operation>) {
    let Some((status_col, state_variant_map)) = link_states_to_enum(model, ek) else {
        return (None, Vec::new());
    };

    // status 軸が軸リストに存在するか確認
    let status_in_axes = axes.iter().any(|a| a.column == status_col);
    if !status_in_axes {
        return (None, Vec::new());
    }

    // effects にも同じカラムへの指定があれば DoubleModeledEnum を診断
    let has_effect_on_status = effects_for_entity.iter().any(|e| e.column == status_col);
    if has_effect_on_status {
        diags.push(StateDiag::DoubleModeledEnum {
            column: status_col.clone(),
        });
    }

    let mut ops: Vec<Operation> = Vec::new();

    for st in &model.state_transitions {
        let (from_sk, to_sk) = match (&st.from, &st.to) {
            (NodeRef::State(f), NodeRef::State(t)) => (*f, *t),
            _ => continue,
        };
        let event_key = match &st.event {
            NodeRef::Event(ek) => *ek,
            _ => continue,
        };
        let from_variant = match state_variant_map.get(&from_sk) {
            Some(v) => v.clone(),
            None => continue,
        };
        let to_variant = match state_variant_map.get(&to_sk) {
            Some(v) => v.clone(),
            None => continue,
        };

        // このイベントを raise する usecase を取得
        for rel in &model.relations {
            if rel.kind != RelKind::Raises {
                continue;
            }
            let (uc_key, ev_key) = match (&rel.from, &rel.to) {
                (NodeRef::UseCase(u), NodeRef::Event(e)) => (*u, *e),
                _ => continue,
            };
            if ev_key != event_key {
                continue;
            }
            // BUC フィルタ: reachable に含まれない usecase はスキップ
            if let Some(reachable) = buc_reachable {
                if !reachable.contains(&NodeRef::UseCase(uc_key)) {
                    continue;
                }
            }
            let usecase_id = model.use_cases[uc_key].id.clone();
            let buc_id = buc_of_usecase.get(&uc_key).cloned();

            ops.push(Operation {
                usecase_id,
                buc_id,
                op_kind: OpKind::Update,
                guard: vec![AxisConstraint {
                    column: status_col.clone(),
                    value: AbstractValue::Enum(from_variant.clone()),
                }],
                effects: vec![(status_col.clone(), AbstractValue::Enum(to_variant.clone()))],
            });
        }
    }

    (Some(status_col), ops)
}

/// entity に対する全演算（Create/Update/Delete）を収集する
fn collect_operations(
    model: &SemanticModel,
    ek: EntityKey,
    buc_of_usecase: &HashMap<UseCaseKey, String>,
    buc_reachable: Option<&HashSet<NodeRef>>,
    axes: &[StateAxis],
    diags: &mut Vec<StateDiag>,
    effects_for_entity: &[&ColumnEffect],
) -> (Vec<Operation>, Option<String>) {
    // transitions 由来の status 遷移 Update 演算を構築し、
    // それぞれに対応する usecase key を取得する（non-status 効果マージ用）
    let (status_col, mut ops) = build_status_update_ops(
        model,
        ek,
        buc_of_usecase,
        buc_reachable,
        axes,
        diags,
        effects_for_entity,
    );

    // usecase key → 対応する transitions 由来演算のインデックス集合
    // （同一 usecase が複数 transition を起こす場合に全てにマージ）
    let mut transition_op_idx_of_uc: HashMap<UseCaseKey, Vec<usize>> = HashMap::new();
    for (i, op) in ops.iter().enumerate() {
        // transitions 由来の ops は guard が status==from_variant を持つ Update
        if op.op_kind == OpKind::Update && !op.guard.is_empty() {
            // ops[i] の usecase_id から UseCaseKey を引く
            if let Some(uc_key) = model.use_cases.iter().find_map(|(k, uc)| {
                if uc.id == op.usecase_id {
                    Some(k)
                } else {
                    None
                }
            }) {
                transition_op_idx_of_uc.entry(uc_key).or_default().push(i);
            }
        }
    }

    // effects をカラム毎にまとめる（usecase key → Vec<(col, val)>）
    // status カラムに対する effects は DoubleModeledEnum により無視
    let mut uc_effects: HashMap<UseCaseKey, Vec<(String, AbstractValue)>> = HashMap::new();
    for eff in effects_for_entity {
        if Some(&eff.column) == status_col.as_ref() {
            continue; // transitions が真実源なので無視
        }
        // event 由来の場合はそのイベントを raise する全 UC へ展開する
        let origin_ucs: Vec<UseCaseKey> = match &eff.origin {
            NodeRef::UseCase(k) => vec![*k],
            NodeRef::Event(ek) => model
                .relations
                .iter()
                .filter_map(|rel| {
                    if rel.kind == RelKind::Raises {
                        if let (NodeRef::UseCase(uk), NodeRef::Event(raised_ek)) =
                            (&rel.from, &rel.to)
                        {
                            if raised_ek == ek {
                                return Some(*uk);
                            }
                        }
                    }
                    None
                })
                .collect(),
            _ => continue,
        };
        for origin_uc_key in origin_ucs {
            if let Some(reachable) = buc_reachable {
                if !reachable.contains(&NodeRef::UseCase(origin_uc_key)) {
                    continue;
                }
            }
            uc_effects
                .entry(origin_uc_key)
                .or_default()
                .push((eff.column.clone(), AbstractValue::from_effect(&eff.value)));
        }
    }

    // transitions 由来演算への non-status 効果のマージ
    // transitions がある usecase が持つ non-status 効果は、
    // guard（status==from）と同じ演算に含めることで、
    // 正しい状態からのみ遷移させる。
    let uc_effect_keys: Vec<UseCaseKey> = uc_effects.keys().copied().collect();
    for uc_key in uc_effect_keys {
        if let Some(idxs) = transition_op_idx_of_uc.get(&uc_key) {
            let extra: Vec<_> = uc_effects.remove(&uc_key).unwrap_or_default();
            for &idx in idxs {
                for (col, val) in &extra {
                    // 既に同カラムの効果が無い場合のみ追加
                    if !ops[idx].effects.iter().any(|(c, _)| c == col) {
                        ops[idx].effects.push((col.clone(), val.clone()));
                    }
                }
            }
        }
    }
    // uc_effects の残りは transitions に対応しない usecase の効果（guard なし）

    // CRUD エッジから演算を構築
    // transitions 由来の演算が既にある usecase は status 効果を重複登録しないよう注意
    let mut transitions_usecases: HashSet<UseCaseKey> = HashSet::new();
    for idxs in transition_op_idx_of_uc.values() {
        for &idx in idxs {
            if let Some(uc_key) = model.use_cases.iter().find_map(|(k, uc)| {
                if uc.id == ops[idx].usecase_id {
                    Some(k)
                } else {
                    None
                }
            }) {
                transitions_usecases.insert(uc_key);
            }
        }
    }

    let mut effective_crud: Vec<(OpKind, UseCaseKey)> = Vec::new();
    for rel in &model.relations {
        if rel.to != NodeRef::Entity(ek) {
            continue;
        }
        let op_kind = match rel.kind {
            RelKind::Creates => OpKind::Create,
            RelKind::Updates | RelKind::Writes => OpKind::Update,
            RelKind::Deletes => OpKind::Delete,
            _ => continue,
        };
        match rel.from {
            NodeRef::UseCase(uk) => effective_crud.push((op_kind, uk)),
            NodeRef::Api(ak) => {
                for invoke in &model.relations {
                    if invoke.kind == RelKind::Invokes && invoke.to == NodeRef::Api(ak) {
                        if let NodeRef::UseCase(uk) = invoke.from {
                            effective_crud.push((op_kind, uk));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    effective_crud.sort_by_key(|(op_kind, uc_key)| {
        (
            model.use_cases[*uc_key].id.clone(),
            match op_kind {
                OpKind::Create => 0,
                OpKind::Update => 1,
                OpKind::Delete => 2,
            },
        )
    });
    effective_crud.dedup();

    for (op_kind, uc_key) in effective_crud {
        if let Some(reachable) = buc_reachable {
            if !reachable.contains(&NodeRef::UseCase(uc_key)) {
                continue;
            }
        }

        // この usecase の non-status エフェクト（transitions に未マージのもの）を取得
        let raw_effects = uc_effects.get(&uc_key).cloned().unwrap_or_default();

        // 重複・矛盾効果の診断（同一カラムに異なる値）
        let mut seen: HashMap<String, AbstractValue> = HashMap::new();
        let mut deduped: Vec<(String, AbstractValue)> = Vec::new();
        for (col, val) in &raw_effects {
            if let Some(prev) = seen.get(col) {
                if prev != val {
                    diags.push(StateDiag::ConflictingEffects {
                        usecase: model.use_cases[uc_key].id.clone(),
                        column: col.clone(),
                    });
                }
                // last-wins
                if let Some(entry) = deduped.iter_mut().find(|(c, _)| c == col) {
                    *entry = (col.clone(), val.clone());
                }
            } else {
                seen.insert(col.clone(), val.clone());
                deduped.push((col.clone(), val.clone()));
            }
        }

        let usecase_id = model.use_cases[uc_key].id.clone();
        let buc_id = buc_of_usecase.get(&uc_key).cloned();

        // transitions 由来の演算がある Update usecase は、
        // 既に ops に guard 付きで追加済みなのでスキップ。
        // ただし Create/Delete は transitions に関係なく追加する。
        if op_kind == OpKind::Update && transitions_usecases.contains(&uc_key) {
            // non-status 効果は既にマージ済み。provenance は transitions 演算に記録されているので省略。
            continue;
        }

        ops.push(Operation {
            usecase_id,
            buc_id,
            op_kind,
            guard: vec![],
            effects: deduped,
        });
    }

    apply_trigger_order_guards(model, ek, axes, &status_col, &mut ops, effects_for_entity);

    (ops, status_col)
}

fn apply_trigger_order_guards(
    model: &SemanticModel,
    ek: EntityKey,
    axes: &[StateAxis],
    status_col: &Option<String>,
    ops: &mut [Operation],
    effects_for_entity: &[&ColumnEffect],
) {
    let axis_cols: HashSet<&str> = axes.iter().map(|axis| axis.column.as_str()).collect();
    let event_status_effects = event_status_effects_for_entity(model, ek, status_col);

    for rel in &model.relations {
        let (event, triggered_uc) = match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Triggers, NodeRef::Event(event), NodeRef::UseCase(triggered_uc)) => {
                (*event, *triggered_uc)
            }
            _ => continue,
        };

        let mut guards = Vec::new();
        for upstream_uc in usecases_raising_event(model, event) {
            for effect in effects_for_entity {
                let applies = match effect.origin {
                    NodeRef::UseCase(uc) => uc == upstream_uc,
                    NodeRef::Event(e) => e == event,
                    _ => false,
                };
                if applies {
                    guards.push(AxisConstraint {
                        column: effect.column.clone(),
                        value: AbstractValue::from_effect(&effect.value),
                    });
                }
            }
        }

        if let Some((column, value)) = event_status_effects.get(&event) {
            guards.push(AxisConstraint {
                column: column.clone(),
                value: value.clone(),
            });
        }

        let triggered_id = model.use_cases[triggered_uc].id.as_str();
        for op in ops.iter_mut().filter(|op| op.usecase_id == triggered_id) {
            for guard in &guards {
                if !axis_cols.contains(guard.column.as_str()) {
                    continue;
                }
                if op.effects.iter().any(|(column, _)| column == &guard.column) {
                    continue;
                }
                if op
                    .guard
                    .iter()
                    .any(|existing| existing.column == guard.column)
                {
                    continue;
                }
                op.guard.push(guard.clone());
            }
        }
    }
}

fn event_status_effects_for_entity(
    model: &SemanticModel,
    ek: EntityKey,
    status_col: &Option<String>,
) -> HashMap<crate::model::EventKey, (String, AbstractValue)> {
    let mut effects = HashMap::new();
    let Some(status_col) = status_col else {
        return effects;
    };
    let Some((_, state_variant_map)) = link_states_to_enum(model, ek) else {
        return effects;
    };

    for st in &model.state_transitions {
        let (NodeRef::Event(event), NodeRef::State(to_state)) = (&st.event, &st.to) else {
            continue;
        };
        if let Some(to_variant) = state_variant_map.get(to_state) {
            effects.insert(
                *event,
                (status_col.clone(), AbstractValue::Enum(to_variant.clone())),
            );
        }
    }

    effects
}

fn usecases_raising_event(model: &SemanticModel, event: crate::model::EventKey) -> Vec<UseCaseKey> {
    model
        .relations
        .iter()
        .filter_map(|rel| match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Raises, NodeRef::UseCase(uc), NodeRef::Event(raised)) if *raised == event => {
                Some(*uc)
            }
            _ => None,
        })
        .collect()
}

/// entity 単位の状態パターン導出
fn derive_for_entity(
    model: &SemanticModel,
    ek: EntityKey,
    buc_of_usecase: &HashMap<UseCaseKey, String>,
    buc_reachable: Option<&HashSet<NodeRef>>,
    cap: usize,
) -> EntityStateResult {
    let entity = &model.entities[ek];
    let entity_id = entity.id.clone();
    let entity_label = entity.label.clone();
    let mut diags: Vec<StateDiag> = Vec::new();

    // この entity に対する column_effects を取得
    let effects_for_entity: Vec<&ColumnEffect> = model
        .column_effects
        .iter()
        .filter(|e| e.entity == ek)
        .collect();

    // この entity に対する比較命題 Props を収集（制約述語と proposition_effects から一意化）
    let mut prop_keys_seen: HashSet<String> = HashSet::new();
    let mut proposition_props: Vec<ComparisonProp> = Vec::new();
    for fc in model
        .forbidden_constraints
        .iter()
        .filter(|fc| fc.entity == ek)
    {
        for p in &fc.comparisons {
            if prop_keys_seen.insert(p.axis_key()) {
                proposition_props.push(p.clone());
            }
        }
    }
    for inv in model
        .entity_invariants
        .iter()
        .filter(|inv| inv.entity == ek)
    {
        for p in inv
            .guard_comparisons
            .iter()
            .chain(inv.required_comparisons.iter())
        {
            if prop_keys_seen.insert(p.axis_key()) {
                proposition_props.push(p.clone());
            }
        }
    }
    for required in model
        .required_constraints
        .iter()
        .filter(|required| required.entity == ek)
    {
        for p in &required.comparisons {
            if prop_keys_seen.insert(p.axis_key()) {
                proposition_props.push(p.clone());
            }
        }
    }
    for exclusive in model
        .exclusive_constraints
        .iter()
        .filter(|exclusive| exclusive.entity == ek)
    {
        for p in &exclusive.comparisons {
            if prop_keys_seen.insert(p.axis_key()) {
                proposition_props.push(p.clone());
            }
        }
    }
    for pe in model
        .proposition_effects
        .iter()
        .filter(|pe| pe.entity == ek)
    {
        if prop_keys_seen.insert(pe.prop.axis_key()) {
            proposition_props.push(pe.prop.clone());
        }
    }

    // 状態軸の特定（比較命題軸も含む）
    let axes = identify_axes(&entity.columns, &effects_for_entity, &proposition_props);

    // 軸が無ければ自明な空パターン 1 件で終了
    if axes.is_empty() {
        return EntityStateResult {
            entity_id,
            entity_label,
            axes,
            patterns: vec![ReachablePattern {
                pattern: StatePattern {
                    values: BTreeMap::new(),
                },
                is_initial: true,
                is_terminal: true,
                provenance: Provenance::default(),
            }],
            truncated: false,
            no_creation_path: false,
            diagnostics: diags,
        };
    }

    // 演算の収集
    let (mut ops, status_col) = collect_operations(
        model,
        ek,
        buc_of_usecase,
        buc_reachable,
        &axes,
        &mut diags,
        &effects_for_entity,
    );

    // 比較命題効果を Update 演算として ops に追加
    // sets(origin, entity, <expr>, true/false) → 対応するユースケースの Update に命題軸効果を注入
    for pe in model
        .proposition_effects
        .iter()
        .filter(|pe| pe.entity == ek)
    {
        let axis_col = prop_col_key(&pe.prop);
        if !axes.iter().any(|ax| ax.column == axis_col) {
            continue;
        }
        let effect_val = (axis_col.clone(), AbstractValue::Bool(pe.truth));

        let origin_ucs: Vec<(String, Option<String>)> = match &pe.origin {
            NodeRef::UseCase(uk) => {
                if let Some(reachable) = buc_reachable {
                    if !reachable.contains(&NodeRef::UseCase(*uk)) {
                        vec![]
                    } else {
                        let uid = model.use_cases[*uk].id.clone();
                        let bid = buc_of_usecase.get(uk).cloned();
                        vec![(uid, bid)]
                    }
                } else {
                    let uid = model.use_cases[*uk].id.clone();
                    let bid = buc_of_usecase.get(uk).cloned();
                    vec![(uid, bid)]
                }
            }
            NodeRef::Event(ek_ev) => model
                .relations
                .iter()
                .filter_map(|rel| {
                    if rel.kind == RelKind::Raises {
                        if let (NodeRef::UseCase(uk), NodeRef::Event(raised_ek)) =
                            (&rel.from, &rel.to)
                        {
                            if raised_ek == ek_ev {
                                if let Some(reachable) = buc_reachable {
                                    if !reachable.contains(&NodeRef::UseCase(*uk)) {
                                        return None;
                                    }
                                }
                                let uid = model.use_cases[*uk].id.clone();
                                let bid = buc_of_usecase.get(uk).cloned();
                                return Some((uid, bid));
                            }
                        }
                    }
                    None
                })
                .collect(),
            _ => vec![],
        };

        for (usecase_id, buc_id) in origin_ucs {
            // 既存の同一 usecase 演算に命題軸効果をマージ（同カラムが未設定の場合のみ）。
            // Create と分離すると、同一 UC の status/effect と comparison effect の間に
            // default false の中間パターンが生まれてしまう。
            let mut merged = false;
            for op in ops.iter_mut().filter(|op| op.usecase_id == usecase_id) {
                merged = true;
                if !op.effects.iter().any(|(c, _)| c == &axis_col) {
                    op.effects.push(effect_val.clone());
                }
            }
            if !merged {
                ops.push(Operation {
                    usecase_id,
                    buc_id,
                    op_kind: OpKind::Update,
                    guard: vec![],
                    effects: vec![effect_val.clone()],
                });
            }
        }
    }

    // ── シードパターンの構築 ──────────────────────────────────────────────────
    // 基底値: @default または軸種別ごとのフォールバック
    // 命題軸は実カラムに対応しないため filter_map で自然に除外され、後で手動追加する
    let mut base: BTreeMap<String, AbstractValue> = axes
        .iter()
        .filter_map(|ax| {
            let col = entity.columns.iter().find(|c| c.name == ax.column)?;
            Some((ax.column.clone(), default_value_for_axis(col)))
        })
        .collect();
    // 命題軸のデフォルト値: Bool(false)（比較が成立しない初期状態）
    for ax in &axes {
        if matches!(&ax.kind, AxisKind::Proposition { .. }) {
            base.insert(ax.column.clone(), AbstractValue::Bool(false));
        }
    }
    let base_pattern = StatePattern { values: base };

    // creates 演算のシード
    let create_ops: Vec<&Operation> = ops
        .iter()
        .filter(|op| op.op_kind == OpKind::Create)
        .collect();

    let no_creation_path = create_ops.is_empty();
    if no_creation_path {
        diags.push(StateDiag::NoCreationPath);
    }

    // IndexMap で挿入順を保持しつつ HashSet でデdup
    let mut reached: Vec<ReachablePattern> = Vec::new();
    let mut reached_set: HashSet<StatePattern> = HashSet::new();
    let mut worklist: VecDeque<usize> = VecDeque::new(); // reached のインデックス

    let add_pattern = |pattern: StatePattern,
                       is_initial: bool,
                       prov: Provenance,
                       reached: &mut Vec<ReachablePattern>,
                       reached_set: &mut HashSet<StatePattern>,
                       worklist: &mut VecDeque<usize>|
     -> bool {
        if reached_set.contains(&pattern) {
            // 既存エントリに provenance をマージ
            if let Some(entry) = reached.iter_mut().find(|r| r.pattern == pattern) {
                entry.provenance.via.extend(prov.via);
            }
            return false;
        }
        let idx = reached.len();
        reached_set.insert(pattern.clone());
        reached.push(ReachablePattern {
            pattern,
            is_initial,
            is_terminal: false, // 後で決定
            provenance: prov,
        });
        worklist.push_back(idx);
        true
    };

    if no_creation_path {
        // defaults のみのシード
        let prov = Provenance::default();
        add_pattern(
            base_pattern.clone(),
            true,
            prov,
            &mut reached,
            &mut reached_set,
            &mut worklist,
        );
    } else {
        for cop in &create_ops {
            let seeded = base_pattern.apply_effects(&cop.effects);
            let mut prov = Provenance::default();
            prov.via
                .insert((cop.buc_id.clone(), cop.usecase_id.clone()));
            add_pattern(
                seeded,
                true,
                prov,
                &mut reached,
                &mut reached_set,
                &mut worklist,
            );
        }
    }

    let mut truncated = false;

    // ── BFS で固定点 ─────────────────────────────────────────────────────────
    while let Some(idx) = worklist.pop_front() {
        let current = reached[idx].pattern.clone();
        let prov_base = reached[idx].provenance.via.clone();

        for op in &ops {
            match op.op_kind {
                OpKind::Create => continue, // シード専用
                OpKind::Delete => {
                    // ガード判定
                    if !guard_holds(&op.guard, &current) {
                        continue;
                    }
                    // 削除は後継を生成せず、元パターンを terminal にする
                    let mut prov = Provenance {
                        via: prov_base.clone(),
                    };
                    prov.via.insert((op.buc_id.clone(), op.usecase_id.clone()));
                    reached[idx].is_terminal = true;
                    reached[idx].provenance.via.extend(prov.via);
                    continue;
                }
                OpKind::Update => {
                    if !guard_holds(&op.guard, &current) {
                        continue;
                    }
                    if op.effects.is_empty() {
                        // 効果が無い update → provenance に記録するのみ
                        reached[idx]
                            .provenance
                            .via
                            .insert((op.buc_id.clone(), op.usecase_id.clone()));
                        continue;
                    }
                    let next = current.apply_effects(&op.effects);
                    if next == current {
                        // 同じパターン → ループ、スキップ
                        continue;
                    }

                    if reached.len() >= cap && !reached_set.contains(&next) {
                        truncated = true;
                        continue;
                    }

                    let mut prov = Provenance {
                        via: prov_base.clone(),
                    };
                    prov.via.insert((op.buc_id.clone(), op.usecase_id.clone()));
                    add_pattern(
                        next,
                        false,
                        prov,
                        &mut reached,
                        &mut reached_set,
                        &mut worklist,
                    );
                }
            }
        }
    }

    if truncated {
        let bound = compute_bound(&axes);
        diags.push(StateDiag::PatternCapReached { cap, bound });
    }

    // ── is_terminal の決定（Update/Delete が enabled でないパターン） ────────
    for p in reached.iter_mut() {
        if p.is_terminal {
            continue; // delete により既に terminal
        }
        let has_outgoing = ops.iter().any(|op| {
            (op.op_kind == OpKind::Update || op.op_kind == OpKind::Delete)
                && guard_holds(&op.guard, &p.pattern)
                && (op.op_kind == OpKind::Delete || !op.effects.is_empty())
        });
        if !has_outgoing {
            p.is_terminal = true;
        }
    }

    // ── 到達不能 Enum バリアントの診断 ───────────────────────────────────────
    for ax in &axes {
        if let AxisKind::Enum(variants) = &ax.kind {
            for variant in variants {
                let reached_val = AbstractValue::Enum(variant.clone());
                if !reached
                    .iter()
                    .any(|r| r.pattern.values.get(&ax.column) == Some(&reached_val))
                {
                    diags.push(StateDiag::UnreachableEnumVariant {
                        column: ax.column.clone(),
                        variant: variant.clone(),
                    });
                }
            }
        }
    }

    // ── 禁止状態・不変条件チェック ──────────────────────────────────────────
    check_constraints(model, ek, &reached, &mut diags);

    // status_col の情報は現在未使用（将来の拡張用に保持）
    let _ = status_col;

    EntityStateResult {
        entity_id,
        entity_label,
        axes,
        patterns: reached,
        truncated,
        no_creation_path,
        diagnostics: diags,
    }
}

// ── 制約チェック ─────────────────────────────────────────────────────────────

/// `AbstractValue` を人が読める文字列に変換する（診断メッセージ用）
fn abstract_value_display(val: &AbstractValue) -> String {
    match val {
        AbstractValue::Enum(s) => s.clone(),
        AbstractValue::Bool(b) => b.to_string(),
        AbstractValue::Present => "present".to_string(),
        AbstractValue::Null => "null".to_string(),
    }
}

/// `StatePattern` を "col1=val1, col2=val2" 形式に整形する
fn describe_pattern(pattern: &StatePattern) -> String {
    pattern
        .values
        .iter()
        .map(|(col, val)| format!("{}={}", col, abstract_value_display(val)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn abstract_eq_conditions(conditions: &[(String, EffectValue)]) -> Vec<(String, AbstractValue)> {
    conditions
        .iter()
        .map(|(col, val)| (col.clone(), AbstractValue::from_effect(val)))
        .collect()
}

fn abstract_comparison_conditions(comparisons: &[ComparisonProp]) -> Vec<(String, AbstractValue)> {
    comparisons
        .iter()
        .map(|p| (prop_col_key(p), AbstractValue::Bool(true)))
        .collect()
}

fn condition_display_parts(
    equals: &[(String, AbstractValue)],
    comparisons: &[ComparisonProp],
) -> Vec<String> {
    equals
        .iter()
        .map(|(col, av)| format!("{}={}", col, abstract_value_display(av)))
        .chain(comparisons.iter().map(|p| format!("{}=true", p.display())))
        .collect()
}

fn all_conditions_hold(
    pattern: &StatePattern,
    equals: &[(String, AbstractValue)],
    props: &[(String, AbstractValue)],
) -> bool {
    equals
        .iter()
        .chain(props.iter())
        .all(|(col, av)| pattern.values.get(col) == Some(av))
}

fn matched_condition_display_parts(
    pattern: &StatePattern,
    equals: &[(String, AbstractValue)],
    comparisons: &[ComparisonProp],
    props: &[(String, AbstractValue)],
) -> Vec<String> {
    let mut matched = Vec::new();
    for (col, av) in equals {
        if pattern.values.get(col) == Some(av) {
            matched.push(format!("{}={}", col, abstract_value_display(av)));
        }
    }
    for (idx, (col, av)) in props.iter().enumerate() {
        if pattern.values.get(col) == Some(av) {
            matched.push(format!("{}=true", comparisons[idx].display()));
        }
    }
    matched
}

fn correlated_axis_hint(
    equals: &[(String, AbstractValue)],
    props: &[(String, AbstractValue)],
) -> Option<String> {
    let condition_count = equals.len() + props.len();
    if condition_count < 2 {
        return None;
    }

    if !props.is_empty() {
        Some(
            "multi-axis forbidden witnesses with comparison propositions can be product-space artifacts; drive the comparison true/false in the same usecase that changes the correlated state, or add explicit guards"
                .to_string(),
        )
    } else {
        Some(
            "multi-axis forbidden witnesses can be global-product artifacts; if these axes are correlated, model the transition with one usecase setting all correlated axes or add explicit guards"
                .to_string(),
        )
    }
}

fn triggered_flow_order_hint(model: &SemanticModel, provenance: &Provenance) -> Option<String> {
    let triggered_usecases: HashSet<String> = model
        .relations
        .iter()
        .filter_map(|rel| match (&rel.kind, &rel.to) {
            (RelKind::Triggers, NodeRef::UseCase(uk)) => Some(model.use_cases[*uk].id.clone()),
            _ => None,
        })
        .collect();

    let witnessed_triggered: Vec<_> = provenance
        .via
        .iter()
        .filter_map(|(_, usecase_id)| {
            triggered_usecases
                .contains(usecase_id)
                .then_some(usecase_id.as_str())
        })
        .collect();

    if witnessed_triggered.is_empty() {
        return None;
    }

    Some(format!(
        "triggered flow has no modeled upstream evidence for this invariant; if {} is only valid after an upstream event, set the evidence columns in the raising use case/event or model an explicit state guard",
        witnessed_triggered.join(", ")
    ))
}

/// 到達可能パターン群に対して entity 制約を検査し、違反を `diags` に追加する。
fn check_constraints(
    model: &SemanticModel,
    ek: EntityKey,
    reached: &[ReachablePattern],
    diags: &mut Vec<StateDiag>,
) {
    // ── 禁止状態チェック ─────────────────────────────────────────────────────
    // `conditions`（等値）と `comparisons`（命題=true）に列挙した全条件が
    // 同時に成立するパターンは禁止（AND）。
    for fc in model
        .forbidden_constraints
        .iter()
        .filter(|fc| fc.entity == ek)
    {
        let abs_conds = abstract_eq_conditions(&fc.conditions);
        let abs_props = abstract_comparison_conditions(&fc.comparisons);

        for rp in reached {
            if all_conditions_hold(&rp.pattern, &abs_conds, &abs_props) {
                diags.push(StateDiag::ForbiddenStateViolated {
                    conditions: condition_display_parts(&abs_conds, &fc.comparisons).join(" AND "),
                    pattern_desc: describe_pattern(&rp.pattern),
                    correlation_hint: correlated_axis_hint(&abs_conds, &abs_props),
                });
            }
        }
    }

    // ── 不変条件チェック ─────────────────────────────────────────────────────
    // guards（等値 + 命題=true）が全て成立するパターンで
    // requireds（等値 + 命題=true）のいずれかが不成立なら違反。
    for inv in model
        .entity_invariants
        .iter()
        .filter(|inv| inv.entity == ek)
    {
        let abs_guards = abstract_eq_conditions(&inv.guards);
        let abs_guard_props = abstract_comparison_conditions(&inv.guard_comparisons);
        let abs_requireds = abstract_eq_conditions(&inv.requireds);
        let abs_required_props = abstract_comparison_conditions(&inv.required_comparisons);

        for rp in reached {
            // 全ガード条件（等値 + 命題）が成立するパターンのみ検査
            if !all_conditions_hold(&rp.pattern, &abs_guards, &abs_guard_props) {
                continue;
            }
            // required 条件のいずれかが不成立なら違反
            let req_violated = abs_requireds
                .iter()
                .chain(abs_required_props.iter())
                .any(|(col, av)| rp.pattern.values.get(col) != Some(av));
            if req_violated {
                diags.push(StateDiag::InvariantViolated {
                    guards: condition_display_parts(&abs_guards, &inv.guard_comparisons)
                        .join(" AND "),
                    requireds: condition_display_parts(&abs_requireds, &inv.required_comparisons)
                        .join(" AND "),
                    pattern_desc: describe_pattern(&rp.pattern),
                    flow_order_hint: triggered_flow_order_hint(model, &rp.provenance),
                });
            }
        }
    }

    // ── 常時成立チェック ─────────────────────────────────────────────────────
    // `required` はガードのない invariant: 全ての到達状態で条件が成立する必要がある。
    for required in model
        .required_constraints
        .iter()
        .filter(|required| required.entity == ek)
    {
        let abs_conds = abstract_eq_conditions(&required.conditions);
        let abs_props = abstract_comparison_conditions(&required.comparisons);
        for rp in reached {
            if !all_conditions_hold(&rp.pattern, &abs_conds, &abs_props) {
                diags.push(StateDiag::RequiredStateViolated {
                    conditions: condition_display_parts(&abs_conds, &required.comparisons)
                        .join(" AND "),
                    pattern_desc: describe_pattern(&rp.pattern),
                });
            }
        }
    }

    // ── 相互排他チェック ─────────────────────────────────────────────────────
    // `exclusive` は列挙された条件のうち 2 件以上が同時成立する状態を禁止する。
    for exclusive in model
        .exclusive_constraints
        .iter()
        .filter(|exclusive| exclusive.entity == ek)
    {
        let abs_conds = abstract_eq_conditions(&exclusive.conditions);
        let abs_props = abstract_comparison_conditions(&exclusive.comparisons);
        for rp in reached {
            let matched = matched_condition_display_parts(
                &rp.pattern,
                &abs_conds,
                &exclusive.comparisons,
                &abs_props,
            );
            if matched.len() >= 2 {
                diags.push(StateDiag::ExclusiveStateViolated {
                    conditions: matched.join(" AND "),
                    pattern_desc: describe_pattern(&rp.pattern),
                });
            }
        }
    }
}

// ── クロスエンティティ制約チェック ───────────────────────────────────────────

fn check_cross_entity_constraints(
    model: &SemanticModel,
    results: &mut [EntityStateResult],
    result_index: &HashMap<EntityKey, usize>,
) {
    for constraint in &model.cross_forbidden_constraints {
        let diags = evaluate_cross_forbidden(model, results, result_index, constraint);
        for diag in diags {
            push_cross_diag(&constraint.scope, result_index, results, diag);
        }
    }

    for invariant in &model.cross_entity_invariants {
        let diags = evaluate_cross_invariant(model, results, result_index, invariant);
        for diag in diags {
            push_cross_diag(&invariant.scope, result_index, results, diag);
        }
    }
}

fn check_temporal_assertions(
    model: &SemanticModel,
    results: &mut [EntityStateResult],
    result_index: &HashMap<EntityKey, usize>,
) {
    for assertion in &model.temporal_assertions {
        if let Some(diag) = evaluate_temporal_assertion(model, assertion) {
            push_cross_diag(&assertion.scope, result_index, results, diag);
        }
    }
}

fn check_quantifier_constraints(
    model: &SemanticModel,
    results: &mut [EntityStateResult],
    result_index: &HashMap<EntityKey, usize>,
) {
    for constraint in &model.quantifier_constraints {
        if let Some(diag) = evaluate_quantifier_constraint(model, results, result_index, constraint)
        {
            push_cross_diag(
                &[constraint.anchor, constraint.related],
                result_index,
                results,
                diag,
            );
        }
    }
}

fn evaluate_quantifier_constraint(
    model: &SemanticModel,
    results: &[EntityStateResult],
    result_index: &HashMap<EntityKey, usize>,
    constraint: &QuantifierConstraint,
) -> Option<StateDiag> {
    let anchor_result = results.get(*result_index.get(&constraint.anchor)?)?;
    let related_result = results.get(*result_index.get(&constraint.related)?)?;

    let anchor_witness =
        patterns_satisfy_conditions(model, constraint.anchor, anchor_result, &constraint.guards);
    if !anchor_witness.unwrap_or(false) {
        return None;
    }

    let related_witness = patterns_satisfy_conditions(
        model,
        constraint.related,
        related_result,
        &constraint.related_conditions,
    );
    if matches!(constraint.kind, QuantifierKind::None) && related_witness == Ok(false) {
        return None;
    }

    let kind = match constraint.kind {
        QuantifierKind::Has => "has",
        QuantifierKind::None => "none",
    };
    let constraint_desc = format!(
        "{} when ({}) {} {} where ({})",
        model.entities[constraint.anchor].id,
        cross_conditions_display(model, &constraint.guards),
        kind,
        model.entities[constraint.related].id,
        cross_conditions_display(model, &constraint.related_conditions)
    );
    let reason = match related_witness {
        Err(reason) => reason,
        Ok(true) => "related condition has reachable patterns, but linked-instance cardinality is not represented in states".to_string(),
        Ok(false) => "has/none depends on related row cardinality, which is not represented in states".to_string(),
    };

    Some(StateDiag::QuantifierConstraintNotEvaluated {
        anchor: model.entities[constraint.anchor].id.clone(),
        related: model.entities[constraint.related].id.clone(),
        constraint: constraint_desc,
        reason,
    })
}

fn patterns_satisfy_conditions(
    model: &SemanticModel,
    entity: EntityKey,
    result: &EntityStateResult,
    conditions: &[CrossEntityCondition],
) -> Result<bool, String> {
    for pattern in &result.patterns {
        let combo = [(entity, pattern)];
        match cross_conditions_hold(model, conditions, &combo) {
            Ok(true) => return Ok(true),
            Ok(false) => {}
            Err(reason) => return Err(reason),
        }
    }
    Ok(false)
}

fn evaluate_temporal_assertion(
    model: &SemanticModel,
    assertion: &TemporalAssertion,
) -> Option<StateDiag> {
    let anchor = model.use_cases[assertion.anchor].id.clone();
    let requireds = cross_conditions_display(model, &assertion.requireds);
    let effects = immediate_effects_after_usecase(model, assertion.anchor);

    for condition in &assertion.requireds {
        match condition {
            CrossEntityCondition::Equals { column, value } => {
                let expected = AbstractValue::from_effect(value);
                match effects.get(&(column.entity, column.column.clone())) {
                    Some(actual) if actual == &expected => {}
                    Some(actual) => {
                        return Some(StateDiag::TemporalAssertionViolated {
                            anchor,
                            requireds,
                            actual: format!(
                                "{}={}",
                                qualified_column_display(model, column),
                                abstract_value_display(actual)
                            ),
                        });
                    }
                    None => {
                        return Some(StateDiag::TemporalAssertionViolated {
                            anchor,
                            requireds,
                            actual: format!(
                                "{} has no immediate effect",
                                qualified_column_display(model, column)
                            ),
                        });
                    }
                }
            }
            CrossEntityCondition::Comparison(prop) => {
                return Some(StateDiag::TemporalAssertionNotEvaluated {
                    anchor,
                    requireds,
                    reason: format!(
                        "{} requires comparison evaluation; after(...) currently checks immediate equality effects only",
                        cross_comparison_display(model, prop)
                    ),
                });
            }
        }
    }

    None
}

fn immediate_effects_after_usecase(
    model: &SemanticModel,
    anchor: UseCaseKey,
) -> HashMap<(EntityKey, String), AbstractValue> {
    let mut effects = HashMap::new();
    let raised_events: HashSet<_> = model
        .relations
        .iter()
        .filter_map(|rel| match (&rel.kind, &rel.from, &rel.to) {
            (RelKind::Raises, NodeRef::UseCase(uk), NodeRef::Event(ek)) if *uk == anchor => {
                Some(*ek)
            }
            _ => None,
        })
        .collect();

    for effect in &model.column_effects {
        let applies = match effect.origin {
            NodeRef::UseCase(uk) => uk == anchor,
            NodeRef::Event(ek) => raised_events.contains(&ek),
            _ => false,
        };
        if applies {
            effects.insert(
                (effect.entity, effect.column.clone()),
                AbstractValue::from_effect(&effect.value),
            );
        }
    }

    for st in &model.state_transitions {
        let NodeRef::Event(event) = &st.event else {
            continue;
        };
        if !raised_events.contains(event) {
            continue;
        }
        let NodeRef::State(to_state) = &st.to else {
            continue;
        };
        for entity in model.entities.keys() {
            if let Some((status_col, state_variant_map)) = link_states_to_enum(model, entity) {
                if let Some(to_variant) = state_variant_map.get(to_state) {
                    effects.insert(
                        (entity, status_col),
                        AbstractValue::Enum(to_variant.clone()),
                    );
                }
            }
        }
    }

    effects
}

fn evaluate_cross_forbidden(
    model: &SemanticModel,
    results: &[EntityStateResult],
    result_index: &HashMap<EntityKey, usize>,
    constraint: &CrossForbiddenConstraint,
) -> Vec<StateDiag> {
    let entities = entity_list_display(model, &constraint.scope);
    let constraint_desc = cross_conditions_display(model, &constraint.conditions);
    let Some(scoped_results) = scoped_results(results, result_index, &constraint.scope) else {
        return vec![];
    };

    if let Some(reason) =
        invalid_relation_scope_reason(model, &constraint.scope, &constraint.scope_semantics)
    {
        return vec![StateDiag::CrossConstraintNotEvaluated {
            entities,
            constraint: format!("cross_forbidden({})", constraint_desc),
            reason,
        }];
    }
    let relation_scoped = is_relation_scoped(&constraint.scope_semantics);

    let combo_count = cross_combo_count(&scoped_results);
    if combo_count > CROSS_PATTERN_COMBO_CAP {
        return vec![StateDiag::CrossConstraintNotEvaluated {
            entities,
            constraint: format!("cross_forbidden({})", constraint_desc),
            reason: format!(
                "cross-product has {} combinations, above cap {}",
                combo_count, CROSS_PATTERN_COMBO_CAP
            ),
        }];
    }

    let mut diags = Vec::new();
    let mut unknown_reason = None;
    let mut has_unresolved_linked_witness = false;
    let linked_scope_reason =
        unresolved_linked_witness_reason(model, &constraint.scope, &constraint.scope_semantics);
    let mut current = Vec::new();
    visit_pattern_combinations(&scoped_results, 0, &mut current, &mut |combo| {
        if diags.len() >= CROSS_VIOLATION_DIAG_CAP {
            return;
        }
        match cross_conditions_hold(model, &constraint.conditions, combo) {
            Ok(true) => {
                if relation_scoped
                    && relation_scoped_combo_has_linked_evidence(
                        model,
                        &constraint.scope_semantics,
                        combo,
                    )
                {
                    diags.push(StateDiag::CrossForbiddenViolated {
                        entities: entities.clone(),
                        conditions: constraint_desc.clone(),
                        pattern_desc: describe_cross_pattern_combo(model, combo),
                        scope_hint: None,
                    });
                } else if relation_scoped || linked_scope_reason.is_some() {
                    has_unresolved_linked_witness = true;
                } else {
                    diags.push(StateDiag::CrossForbiddenViolated {
                        entities: entities.clone(),
                        conditions: constraint_desc.clone(),
                        pattern_desc: describe_cross_pattern_combo(model, combo),
                        scope_hint: None,
                    });
                }
            }
            Ok(false) => {}
            Err(reason) => {
                unknown_reason.get_or_insert(reason);
            }
        }
    });

    if has_unresolved_linked_witness {
        diags.push(StateDiag::CrossConstraintNotEvaluated {
            entities,
            constraint: format!("cross_forbidden({})", constraint_desc),
            reason: linked_scope_reason.unwrap_or_else(|| {
                relation_scoped_witness_reason(model, &constraint.scope_semantics)
            }),
        });
    } else if diags.len() >= CROSS_VIOLATION_DIAG_CAP {
        diags.push(StateDiag::CrossConstraintNotEvaluated {
            entities,
            constraint: format!("cross_forbidden({})", constraint_desc),
            reason: format!(
                "additional witness combinations omitted after {} diagnostics",
                CROSS_VIOLATION_DIAG_CAP
            ),
        });
    } else if diags.is_empty() {
        if let Some(reason) = unknown_reason {
            diags.push(StateDiag::CrossConstraintNotEvaluated {
                entities,
                constraint: format!("cross_forbidden({})", constraint_desc),
                reason,
            });
        }
    }

    diags
}

fn evaluate_cross_invariant(
    model: &SemanticModel,
    results: &[EntityStateResult],
    result_index: &HashMap<EntityKey, usize>,
    invariant: &CrossEntityInvariant,
) -> Vec<StateDiag> {
    let entities = entity_list_display(model, &invariant.scope);
    let guards_desc = cross_conditions_display(model, &invariant.guards);
    let requireds_desc = cross_conditions_display(model, &invariant.requireds);
    let Some(scoped_results) = scoped_results(results, result_index, &invariant.scope) else {
        return vec![];
    };

    if let Some(reason) =
        invalid_relation_scope_reason(model, &invariant.scope, &invariant.scope_semantics)
    {
        return vec![StateDiag::CrossConstraintNotEvaluated {
            entities,
            constraint: format!(
                "cross_invariant when ({}) then ({})",
                guards_desc, requireds_desc
            ),
            reason,
        }];
    }
    let relation_scoped = is_relation_scoped(&invariant.scope_semantics);

    let combo_count = cross_combo_count(&scoped_results);
    if combo_count > CROSS_PATTERN_COMBO_CAP {
        return vec![StateDiag::CrossConstraintNotEvaluated {
            entities,
            constraint: format!(
                "cross_invariant when ({}) then ({})",
                guards_desc, requireds_desc
            ),
            reason: format!(
                "cross-product has {} combinations, above cap {}",
                combo_count, CROSS_PATTERN_COMBO_CAP
            ),
        }];
    }

    let mut diags = Vec::new();
    let mut unknown_reason = None;
    let mut has_unresolved_linked_witness = false;
    let linked_scope_reason =
        unresolved_linked_witness_reason(model, &invariant.scope, &invariant.scope_semantics);
    let mut current = Vec::new();
    visit_pattern_combinations(&scoped_results, 0, &mut current, &mut |combo| {
        if diags.len() >= CROSS_VIOLATION_DIAG_CAP {
            return;
        }

        match cross_conditions_hold(model, &invariant.guards, combo) {
            Ok(false) => return,
            Ok(true) => {}
            Err(reason) => {
                unknown_reason.get_or_insert(reason);
                return;
            }
        }

        match cross_conditions_hold(model, &invariant.requireds, combo) {
            Ok(true) => {}
            Ok(false) => {
                if relation_scoped
                    && relation_scoped_combo_has_linked_evidence(
                        model,
                        &invariant.scope_semantics,
                        combo,
                    )
                {
                    diags.push(StateDiag::CrossInvariantViolated {
                        entities: entities.clone(),
                        guards: guards_desc.clone(),
                        requireds: requireds_desc.clone(),
                        pattern_desc: describe_cross_pattern_combo(model, combo),
                        scope_hint: None,
                    });
                } else if relation_scoped || linked_scope_reason.is_some() {
                    has_unresolved_linked_witness = true;
                } else {
                    diags.push(StateDiag::CrossInvariantViolated {
                        entities: entities.clone(),
                        guards: guards_desc.clone(),
                        requireds: requireds_desc.clone(),
                        pattern_desc: describe_cross_pattern_combo(model, combo),
                        scope_hint: None,
                    });
                }
            }
            Err(reason) => {
                unknown_reason.get_or_insert(reason);
            }
        }
    });

    if has_unresolved_linked_witness {
        diags.push(StateDiag::CrossConstraintNotEvaluated {
            entities,
            constraint: format!(
                "cross_invariant when ({}) then ({})",
                guards_desc, requireds_desc
            ),
            reason: linked_scope_reason.unwrap_or_else(|| {
                relation_scoped_witness_reason(model, &invariant.scope_semantics)
            }),
        });
    } else if diags.len() >= CROSS_VIOLATION_DIAG_CAP {
        diags.push(StateDiag::CrossConstraintNotEvaluated {
            entities,
            constraint: format!(
                "cross_invariant when ({}) then ({})",
                guards_desc, requireds_desc
            ),
            reason: format!(
                "additional witness combinations omitted after {} diagnostics",
                CROSS_VIOLATION_DIAG_CAP
            ),
        });
    } else if diags.is_empty() {
        if let Some(reason) = unknown_reason {
            diags.push(StateDiag::CrossConstraintNotEvaluated {
                entities,
                constraint: format!(
                    "cross_invariant when ({}) then ({})",
                    guards_desc, requireds_desc
                ),
                reason,
            });
        }
    }

    diags
}

fn scoped_results<'a>(
    results: &'a [EntityStateResult],
    result_index: &HashMap<EntityKey, usize>,
    scope: &[EntityKey],
) -> Option<Vec<(EntityKey, &'a EntityStateResult)>> {
    let mut scoped = Vec::new();
    for entity in scope {
        let idx = *result_index.get(entity)?;
        scoped.push((*entity, &results[idx]));
    }
    Some(scoped)
}

fn cross_combo_count(scoped_results: &[(EntityKey, &EntityStateResult)]) -> usize {
    scoped_results.iter().fold(1usize, |acc, (_, result)| {
        acc.saturating_mul(result.patterns.len().max(1))
    })
}

fn is_relation_scoped(scope_semantics: &CrossConstraintScope) -> bool {
    matches!(scope_semantics, CrossConstraintScope::RelationPath(_))
}

fn unresolved_linked_witness_reason(
    model: &SemanticModel,
    scope: &[EntityKey],
    scope_semantics: &CrossConstraintScope,
) -> Option<String> {
    match scope_semantics {
        CrossConstraintScope::RelationPath(path) => {
            Some(relation_scoped_witness_reason_for_path(model, path))
        }
        CrossConstraintScope::GlobalProduct => {
            if scope.len() < 2
                || !scope
                    .windows(2)
                    .all(|pair| has_relate_edge(model, pair[0], pair[1]))
            {
                return None;
            }

            Some(format!(
                "global cross-product has a witness across related entities, but states tracks per-entity patterns only; use .along({}) for linked-instance intent or remove the relate path for a true global-product rule",
                entity_list_display(model, scope)
            ))
        }
    }
}

fn invalid_relation_scope_reason(
    model: &SemanticModel,
    scope: &[EntityKey],
    scope_semantics: &CrossConstraintScope,
) -> Option<String> {
    let CrossConstraintScope::RelationPath(path) = scope_semantics else {
        return None;
    };

    if path.len() < 2 {
        return Some("along(...) requires at least two entities in a relation path".to_string());
    }

    let outside_path: Vec<String> = scope
        .iter()
        .filter(|entity| !path.contains(entity))
        .map(|entity| model.entities[*entity].id.clone())
        .collect();
    if !outside_path.is_empty() {
        return Some(format!(
            "along({}) does not cover scoped entity {}",
            entity_list_display(model, path),
            outside_path.join(", ")
        ));
    }

    for pair in path.windows(2) {
        let from = pair[0];
        let to = pair[1];
        if !has_relate_edge(model, from, to) {
            return Some(format!(
                "along({}) references no relate path between {} and {}",
                entity_list_display(model, path),
                model.entities[from].id,
                model.entities[to].id
            ));
        }
    }

    None
}

fn relation_scoped_witness_reason(
    model: &SemanticModel,
    scope_semantics: &CrossConstraintScope,
) -> String {
    let CrossConstraintScope::RelationPath(path) = scope_semantics else {
        return "constraint is not relation-scoped".to_string();
    };

    relation_scoped_witness_reason_for_path(model, path)
}

fn relation_scoped_witness_reason_for_path(model: &SemanticModel, path: &[EntityKey]) -> String {
    format!(
        "along({}) has a global-product witness, but states currently tracks per-entity patterns only; linked instance reachability is not yet evaluated",
        entity_list_display(model, path)
    )
}

fn relation_scoped_combo_has_linked_evidence(
    model: &SemanticModel,
    scope_semantics: &CrossConstraintScope,
    combo: &[(EntityKey, &ReachablePattern)],
) -> bool {
    let CrossConstraintScope::RelationPath(path) = scope_semantics else {
        return false;
    };
    if path.len() < 2 {
        return false;
    }

    path.windows(2).all(|pair| {
        has_relate_edge(model, pair[0], pair[1])
            && linked_pair_shares_usecase_provenance(pair[0], pair[1], combo)
    })
}

fn linked_pair_shares_usecase_provenance(
    left: EntityKey,
    right: EntityKey,
    combo: &[(EntityKey, &ReachablePattern)],
) -> bool {
    let Some(left_pattern) = combo
        .iter()
        .find_map(|(entity, pattern)| (*entity == left).then_some(*pattern))
    else {
        return false;
    };
    let Some(right_pattern) = combo
        .iter()
        .find_map(|(entity, pattern)| (*entity == right).then_some(*pattern))
    else {
        return false;
    };

    left_pattern.provenance.via.iter().any(|(_, usecase_id)| {
        right_pattern
            .provenance
            .via
            .iter()
            .any(|(_, other_usecase_id)| other_usecase_id == usecase_id)
    })
}

fn has_relate_edge(model: &SemanticModel, left: EntityKey, right: EntityKey) -> bool {
    model.relations.iter().any(|rel| {
        matches!(
            rel.kind,
            RelKind::RelateOneToOne
                | RelKind::RelateOneToMany
                | RelKind::RelateManyToOne
                | RelKind::RelateManyToMany
        ) && matches!(
            (&rel.from, &rel.to),
            (NodeRef::Entity(from), NodeRef::Entity(to))
                if (*from == left && *to == right) || (*from == right && *to == left)
        )
    })
}

fn visit_pattern_combinations<'a, F>(
    scoped_results: &[(EntityKey, &'a EntityStateResult)],
    idx: usize,
    current: &mut Vec<(EntityKey, &'a ReachablePattern)>,
    visit: &mut F,
) where
    F: FnMut(&[(EntityKey, &'a ReachablePattern)]),
{
    if idx == scoped_results.len() {
        visit(current);
        return;
    }

    let (entity, result) = scoped_results[idx];
    for pattern in &result.patterns {
        current.push((entity, pattern));
        visit_pattern_combinations(scoped_results, idx + 1, current, visit);
        current.pop();
    }
}

fn push_cross_diag(
    scope: &[EntityKey],
    result_index: &HashMap<EntityKey, usize>,
    results: &mut [EntityStateResult],
    diag: StateDiag,
) {
    for entity in scope {
        if let Some(idx) = result_index.get(entity) {
            results[*idx].diagnostics.push(diag.clone());
        }
    }
}

fn cross_conditions_hold(
    model: &SemanticModel,
    conditions: &[CrossEntityCondition],
    combo: &[(EntityKey, &ReachablePattern)],
) -> Result<bool, String> {
    let mut unknown_reason = None;
    for condition in conditions {
        match eval_cross_condition(model, condition, combo) {
            Ok(true) => {}
            Ok(false) => return Ok(false),
            Err(reason) => {
                unknown_reason.get_or_insert(reason);
            }
        }
    }
    if let Some(reason) = unknown_reason {
        Err(reason)
    } else {
        Ok(true)
    }
}

fn eval_cross_condition(
    model: &SemanticModel,
    condition: &CrossEntityCondition,
    combo: &[(EntityKey, &ReachablePattern)],
) -> Result<bool, String> {
    match condition {
        CrossEntityCondition::Equals { column, value } => {
            let actual = cross_column_value(model, column, combo)?;
            Ok(actual == AbstractValue::from_effect(value))
        }
        CrossEntityCondition::Comparison(prop) => eval_cross_comparison(model, prop, combo),
    }
}

fn eval_cross_comparison(
    model: &SemanticModel,
    prop: &CrossComparisonProp,
    combo: &[(EntityKey, &ReachablePattern)],
) -> Result<bool, String> {
    let lhs = cross_column_value(model, &prop.lhs, combo)?;
    match &prop.rhs {
        CrossCmpRhs::Column(rhs_col) => {
            let rhs = cross_column_value(model, rhs_col, combo)?;
            match prop.op {
                crate::model::CmpOpModel::Eq => Ok(lhs == rhs),
                crate::model::CmpOpModel::Ne => Ok(lhs != rhs),
                _ => Err(format!(
                    "{} uses an order operator, but state patterns only contain abstract values",
                    cross_comparison_display(model, prop)
                )),
            }
        }
        CrossCmpRhs::IntLit(_) | CrossCmpRhs::Now => Err(format!(
            "{} compares against a non-state value that is not present in state patterns",
            cross_comparison_display(model, prop)
        )),
    }
}

fn cross_column_value(
    model: &SemanticModel,
    column: &QualifiedModelColumnRef,
    combo: &[(EntityKey, &ReachablePattern)],
) -> Result<AbstractValue, String> {
    let Some((_, pattern)) = combo.iter().find(|(entity, _)| *entity == column.entity) else {
        return Err(format!(
            "{} is outside the evaluated entity combination",
            qualified_column_display(model, column)
        ));
    };
    pattern
        .pattern
        .values
        .get(&column.column)
        .cloned()
        .ok_or_else(|| {
            format!(
                "{} is not a state axis in the derived patterns",
                qualified_column_display(model, column)
            )
        })
}

fn entity_list_display(model: &SemanticModel, scope: &[EntityKey]) -> String {
    scope
        .iter()
        .map(|entity| model.entities[*entity].id.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

fn qualified_column_display(model: &SemanticModel, column: &QualifiedModelColumnRef) -> String {
    format!("{}.{}", model.entities[column.entity].id, column.column)
}

fn cross_rhs_display(model: &SemanticModel, rhs: &CrossCmpRhs) -> String {
    match rhs {
        CrossCmpRhs::Column(column) => qualified_column_display(model, column),
        CrossCmpRhs::IntLit(value) => value.to_string(),
        CrossCmpRhs::Now => "now".to_string(),
    }
}

fn cross_comparison_display(model: &SemanticModel, prop: &CrossComparisonProp) -> String {
    format!(
        "{} {} {}",
        qualified_column_display(model, &prop.lhs),
        prop.op.as_str(),
        cross_rhs_display(model, &prop.rhs)
    )
}

fn cross_condition_display(model: &SemanticModel, condition: &CrossEntityCondition) -> String {
    match condition {
        CrossEntityCondition::Equals { column, value } => format!(
            "{}={}",
            qualified_column_display(model, column),
            abstract_value_display(&AbstractValue::from_effect(value))
        ),
        CrossEntityCondition::Comparison(prop) => {
            format!("{}=true", cross_comparison_display(model, prop))
        }
    }
}

fn cross_conditions_display(model: &SemanticModel, conditions: &[CrossEntityCondition]) -> String {
    conditions
        .iter()
        .map(|condition| cross_condition_display(model, condition))
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn describe_cross_pattern_combo(
    model: &SemanticModel,
    combo: &[(EntityKey, &ReachablePattern)],
) -> String {
    combo
        .iter()
        .map(|(entity, pattern)| {
            format!(
                "{}({})",
                model.entities[*entity].id,
                describe_pattern(&pattern.pattern)
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

// ── ヘルパー ─────────────────────────────────────────────────────────────────

fn guard_holds(guard: &[AxisConstraint], pattern: &StatePattern) -> bool {
    guard
        .iter()
        .all(|c| pattern.values.get(&c.column) == Some(&c.value))
}

fn compute_bound(axes: &[StateAxis]) -> usize {
    axes.iter().fold(1usize, |acc, ax| {
        let factor = match &ax.kind {
            AxisKind::Enum(v) => v.len(),
            AxisKind::Bool => 2,
            AxisKind::Nullable { .. } => 2,
            AxisKind::Proposition { .. } => 2,
        };
        acc.saturating_mul(factor)
    })
}

// ── テスト ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, errors) = parse(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let (model, diags) = build_model(&ast);
        let errs: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errs.is_empty(), "model errors: {:?}", errs);
        model
    }

    // ── 軸なし: 自明なパターン1件 ───────────────────────────────────────────

    #[test]
    fn test_no_state_axes_single_trivial_pattern() {
        let model = model_from(
            r#"
entity Product "商品" { id: Int @pk  name: String  price: Decimal }
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Product").unwrap();
        assert_eq!(r.axes.len(), 0);
        assert_eq!(r.patterns.len(), 1);
        assert!(r.patterns[0].is_initial);
        assert!(r.patterns[0].is_terminal);
    }

    #[test]
    fn test_api_crud_carries_usecase_sets_effects() {
        let model = model_from(
            r#"
entity Store "店舗" {
  id: Int @pk
  status: Enum(open, closed) @default(open)
}
usecase CloseStore "店舗を閉じる"
api StoreAdminApi "店舗管理API"
invokes(CloseStore, StoreAdminApi)
updates(StoreAdminApi, Store)
sets(CloseStore, Store, "status", "closed")
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Store").unwrap();
        let values: Vec<_> = r
            .patterns
            .iter()
            .map(|p| p.pattern.values.get("status").unwrap().clone())
            .collect();
        assert!(values.contains(&AbstractValue::Enum("open".to_string())));
        assert!(values.contains(&AbstractValue::Enum("closed".to_string())));
    }

    #[test]
    fn temporal_assertion_passes_on_transition_immediate_effect() {
        let model = model_from(
            r#"
usecase ExecuteCertIssue "Execute Cert Issue"
event CertIssued "Cert Issued"
entity CertificateOrder "Certificate Order" {
  id: Int @pk
  status: Enum(requested, executed) @default(requested)
}
state Requested "Requested"
state Executed "Executed"
creates(ExecuteCertIssue, CertificateOrder)
raises(ExecuteCertIssue, CertIssued)
transitions(CertIssued, Requested, Executed)
after(ExecuteCertIssue).assert(CertificateOrder.status == executed)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results
            .iter()
            .find(|result| result.entity_id == "CertificateOrder")
            .unwrap();
        assert!(!r
            .diagnostics
            .iter()
            .any(|diag| matches!(diag, StateDiag::TemporalAssertionViolated { .. })));
    }

    #[test]
    fn temporal_assertion_reports_missing_immediate_effect() {
        let model = model_from(
            r#"
usecase ExecuteCertIssue "Execute Cert Issue"
entity CertificateOrder "Certificate Order" {
  id: Int @pk
  status: Enum(requested, executed) @default(requested)
}
creates(ExecuteCertIssue, CertificateOrder)
after(ExecuteCertIssue).assert(CertificateOrder.status == executed)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results
            .iter()
            .find(|result| result.entity_id == "CertificateOrder")
            .unwrap();
        assert!(r
            .diagnostics
            .iter()
            .any(|diag| matches!(diag, StateDiag::TemporalAssertionViolated { .. })));
    }

    #[test]
    fn quantifier_none_reports_not_evaluated_when_related_condition_reachable() {
        let model = model_from(
            r#"
usecase RevokeCert "Revoke Cert"
usecase AssignTerminal "Assign Terminal"
entity ClientCertificate "Client Certificate" {
  id: Int @pk
  status: Enum(active, revoked) @default(active)
}
entity TerminalCertAssignment "Terminal Cert Assignment" {
  id: Int @pk
  status: Enum(active, inactive) @default(active)
}
creates(RevokeCert, ClientCertificate)
creates(AssignTerminal, TerminalCertAssignment)
sets(RevokeCert, ClientCertificate, "status", "revoked")
sets(AssignTerminal, TerminalCertAssignment, "status", "active")
forbidden_when(ClientCertificate, (status, revoked))
  .none(TerminalCertAssignment, (status, active))
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results
            .iter()
            .find(|result| result.entity_id == "ClientCertificate")
            .unwrap();
        assert!(r
            .diagnostics
            .iter()
            .any(|diag| matches!(diag, StateDiag::QuantifierConstraintNotEvaluated { .. })));
    }

    #[test]
    fn quantifier_none_passes_when_related_condition_unreachable() {
        let model = model_from(
            r#"
usecase RevokeCert "Revoke Cert"
usecase AssignTerminal "Assign Terminal"
entity ClientCertificate "Client Certificate" {
  id: Int @pk
  status: Enum(active, revoked) @default(active)
}
entity TerminalCertAssignment "Terminal Cert Assignment" {
  id: Int @pk
  status: Enum(active, inactive) @default(inactive)
}
creates(RevokeCert, ClientCertificate)
creates(AssignTerminal, TerminalCertAssignment)
sets(RevokeCert, ClientCertificate, "status", "revoked")
forbidden_when(ClientCertificate, (status, revoked))
  .none(TerminalCertAssignment, (status, active))
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results
            .iter()
            .find(|result| result.entity_id == "ClientCertificate")
            .unwrap();
        assert!(!r
            .diagnostics
            .iter()
            .any(|diag| matches!(diag, StateDiag::QuantifierConstraintNotEvaluated { .. })));
    }

    // ── Enum 直線連鎖 ───────────────────────────────────────────────────────

    #[test]
    fn test_enum_linear_chain() {
        let model = model_from(
            r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid, cancelled) @default(pending) }
usecase Place  "注文確定"
usecase Pay    "支払い"
usecase Cancel "キャンセル"
event EvPaid   "支払い完了"
event EvCancel "キャンセル"
creates(Place, Order)
updates(usecase::Pay,    Order)
updates(usecase::Cancel, Order)
raises(usecase::Pay,    EvPaid)
raises(usecase::Cancel, EvCancel)
state Pending   "注文受付"
state Paid      "支払済"
state Cancelled "キャンセル済"
transitions(EvPaid,   Pending,   Paid)
transitions(EvCancel, Pending,   Cancelled)
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Order").unwrap();
        let values: Vec<_> = r
            .patterns
            .iter()
            .map(|p| p.pattern.values.get("status").unwrap().clone())
            .collect();
        // pending がシード
        assert!(values.contains(&AbstractValue::Enum("pending".to_string())));
        // paid, cancelled に到達
        assert!(values.contains(&AbstractValue::Enum("paid".to_string())));
        assert!(values.contains(&AbstractValue::Enum("cancelled".to_string())));
        // 到達不能バリアントは無い
        let unreachable_diags: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| matches!(d, StateDiag::UnreachableEnumVariant { .. }))
            .collect();
        assert!(unreachable_diags.is_empty());
    }

    // ── Nullable 軸: set される前は除外される ───────────────────────────────

    #[test]
    fn test_nullable_axis_excluded_until_set() {
        let model = model_from(
            r#"
entity Order "注文" {
  id:           Int @pk
  status:       Enum(pending, delivered) @default(pending)
  delivered_at: DateTime @null
}
usecase Place      "注文確定"
usecase DeliverUc  "配達確認"
event EvDeliver    "配達完了"
creates(Place,         Order)
updates(usecase::DeliverUc, Order)
raises(usecase::DeliverUc, EvDeliver)
state Pending   "受付中"
state Delivered "配達完了"
transitions(EvDeliver, Pending, Delivered)
sets(usecase::DeliverUc, Order, "delivered_at", "present")
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Order").unwrap();

        // (pending, null) はシード
        let pending_null = StatePattern {
            values: BTreeMap::from([
                (
                    "status".to_string(),
                    AbstractValue::Enum("pending".to_string()),
                ),
                ("delivered_at".to_string(), AbstractValue::Null),
            ]),
        };
        assert!(
            r.patterns.iter().any(|p| p.pattern == pending_null),
            "(pending, null) が存在すべき"
        );

        // (pending, present) は到達不能
        let pending_present = StatePattern {
            values: BTreeMap::from([
                (
                    "status".to_string(),
                    AbstractValue::Enum("pending".to_string()),
                ),
                ("delivered_at".to_string(), AbstractValue::Present),
            ]),
        };
        assert!(
            !r.patterns.iter().any(|p| p.pattern == pending_present),
            "(pending, present) は到達不能"
        );

        // (delivered, present) は到達可能
        let delivered_present = StatePattern {
            values: BTreeMap::from([
                (
                    "status".to_string(),
                    AbstractValue::Enum("delivered".to_string()),
                ),
                ("delivered_at".to_string(), AbstractValue::Present),
            ]),
        };
        assert!(
            r.patterns.iter().any(|p| p.pattern == delivered_present),
            "(delivered, present) が存在すべき"
        );

        // (delivered, null) は到達不能（Deliver が status と delivered_at を同時に変える）
        let delivered_null = StatePattern {
            values: BTreeMap::from([
                (
                    "status".to_string(),
                    AbstractValue::Enum("delivered".to_string()),
                ),
                ("delivered_at".to_string(), AbstractValue::Null),
            ]),
        };
        assert!(
            !r.patterns.iter().any(|p| p.pattern == delivered_null),
            "(delivered, null) は到達不能"
        );
    }

    // ── event 由来 sets: UC ではなくイベントが Nullable 軸を動かす ────────────

    #[test]
    fn test_event_origin_sets_nullable_axis() {
        // sets の第1引数がイベントの場合、そのイベントを raise する UC の
        // operations に展開されること（Phase 2）を検証する。
        let model = model_from(
            r#"
entity Order "注文" {
  id:           Int @pk
  status:       Enum(pending, delivered) @default(pending)
  delivered_at: DateTime @null
}
usecase Place      "注文確定"
usecase DeliverUc  "配達確認"
event EvDeliver    "配達完了"
creates(Place,              Order)
updates(usecase::DeliverUc, Order)
raises(usecase::DeliverUc, EvDeliver)
state Pending   "受付中"
state Delivered "配達完了"
transitions(EvDeliver, Pending, Delivered)
sets(event::EvDeliver, Order, "delivered_at", "present")
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Order").unwrap();

        // (delivered, present) に到達できるか — event 由来 sets が展開されて
        // DeliverUc の transition 演算に delivered_at=present が乗ること
        let delivered_present = StatePattern {
            values: BTreeMap::from([
                (
                    "status".to_string(),
                    AbstractValue::Enum("delivered".to_string()),
                ),
                ("delivered_at".to_string(), AbstractValue::Present),
            ]),
        };
        assert!(
            r.patterns.iter().any(|p| p.pattern == delivered_present),
            "(delivered, present) が到達可能なはず: {:?}",
            r.patterns.iter().map(|p| &p.pattern).collect::<Vec<_>>()
        );

        // (delivered, null) は到達不能（event が status と delivered_at を同時に変える）
        let delivered_null = StatePattern {
            values: BTreeMap::from([
                (
                    "status".to_string(),
                    AbstractValue::Enum("delivered".to_string()),
                ),
                ("delivered_at".to_string(), AbstractValue::Null),
            ]),
        };
        assert!(
            !r.patterns.iter().any(|p| p.pattern == delivered_null),
            "(delivered, null) は到達不能なはず"
        );
    }

    #[test]
    fn derive_for_entity_merges_transition_and_event_effects_directly() {
        let model = model_from(
            r#"
entity Order "注文" {
  id:           Int @pk
  status:       Enum(pending, delivered) @default(pending)
  delivered_at: DateTime @null
}
usecase Place      "注文確定"
usecase DeliverUc  "配達確認"
event EvDeliver    "配達完了"
creates(Place,              Order)
updates(usecase::DeliverUc, Order)
raises(usecase::DeliverUc, EvDeliver)
state Pending   "受付中"
state Delivered "配達完了"
transitions(EvDeliver, Pending, Delivered)
sets(event::EvDeliver, Order, "delivered_at", "present")
"#,
        );
        let entity = model
            .entities
            .iter()
            .find_map(|(key, entity)| (entity.id == "Order").then_some(key))
            .unwrap();
        let buc_of_usecase = build_buc_of_usecase(&model);

        let r = derive_for_entity(&model, entity, &buc_of_usecase, None, DEFAULT_PATTERN_CAP);

        let delivered_present = StatePattern {
            values: BTreeMap::from([
                (
                    "status".to_string(),
                    AbstractValue::Enum("delivered".to_string()),
                ),
                ("delivered_at".to_string(), AbstractValue::Present),
            ]),
        };
        let delivered_null = StatePattern {
            values: BTreeMap::from([
                (
                    "status".to_string(),
                    AbstractValue::Enum("delivered".to_string()),
                ),
                ("delivered_at".to_string(), AbstractValue::Null),
            ]),
        };
        assert!(r.patterns.iter().any(|p| p.pattern == delivered_present));
        assert!(!r.patterns.iter().any(|p| p.pattern == delivered_null));
    }

    #[test]
    fn derive_for_entity_honors_buc_reachability_filter_directly() {
        let model = model_from(
            r#"
entity Ticket "チケット" {
  id:     Int @pk
  status: Enum(open, closed) @default(open)
}
buc Included "対象BUC"
buc Excluded "対象外BUC"
usecase OpenTicket  "作成"
usecase CloseTicket "終了"
creates(OpenTicket, Ticket)
updates(CloseTicket, Ticket)
sets(CloseTicket, Ticket, "status", "closed")
contains(Included, OpenTicket)
contains(Excluded, CloseTicket)
"#,
        );
        let entity = model
            .entities
            .iter()
            .find_map(|(key, entity)| (entity.id == "Ticket").then_some(key))
            .unwrap();
        let buc_of_usecase = build_buc_of_usecase(&model);
        let reachable = crate::resolver::reachable_from_bucs(&model, &["Included".to_string()]);

        let r = derive_for_entity(
            &model,
            entity,
            &buc_of_usecase,
            Some(&reachable),
            DEFAULT_PATTERN_CAP,
        );

        let values: Vec<_> = r
            .patterns
            .iter()
            .map(|p| p.pattern.values.get("status").unwrap().clone())
            .collect();
        assert!(values.contains(&AbstractValue::Enum("open".to_string())));
        assert!(!values.contains(&AbstractValue::Enum("closed".to_string())));
    }

    // ── creates なし → defaults シード ─────────────────────────────────────

    #[test]
    fn test_no_creates_seeds_from_defaults() {
        let model = model_from(
            r#"
entity Flag "フラグ" { id: Int @pk  active: Bool }
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Flag").unwrap();
        assert!(r.no_creation_path);
        assert_eq!(r.patterns.len(), 1);
        assert!(r.patterns[0].is_initial);
        // Bool のデフォルトは false
        assert_eq!(
            r.patterns[0].pattern.values.get("active"),
            Some(&AbstractValue::Bool(false))
        );
    }

    // ── cap truncation ──────────────────────────────────────────────────────

    #[test]
    fn test_cap_truncation() {
        // 4 nullable 軸 → 最大 2^4 = 16 パターン、cap=5 で truncated
        let src = r#"
entity Multi "マルチ" {
  id: Int @pk
  a: String @null
  b: String @null
  c: String @null
  d: String @null
}
usecase Create "生成"
usecase SetA   "Aセット"
usecase SetB   "Bセット"
usecase SetC   "Cセット"
usecase SetD   "Dセット"
creates(Create, Multi)
updates(SetA, Multi)
updates(SetB, Multi)
updates(SetC, Multi)
updates(SetD, Multi)
sets(usecase::SetA, Multi, "a", "present")
sets(usecase::SetB, Multi, "b", "present")
sets(usecase::SetC, Multi, "c", "present")
sets(usecase::SetD, Multi, "d", "present")
"#;
        let model = model_from(src);
        let results = derive_state_patterns(&model, &[], 5);
        let r = results.iter().find(|r| r.entity_id == "Multi").unwrap();
        assert!(r.truncated);
        assert!(r.patterns.len() <= 5);
    }

    // ── TypedPresent: PG 型名が到達判定では Present と同値 ─────────────────

    #[test]
    fn test_typed_present_is_treated_as_present() {
        let model = model_from(
            r#"
entity Doc "文書" {
  id:       Int @pk
  metadata: String @null
}
usecase Create  "作成"
usecase Publish "公開"
creates(Create,  Doc)
updates(Publish, Doc)
sets(usecase::Publish, Doc, "metadata", "jsonb")
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Doc").unwrap();
        // (metadata=null) がシード
        let null_p = StatePattern {
            values: BTreeMap::from([("metadata".to_string(), AbstractValue::Null)]),
        };
        assert!(r.patterns.iter().any(|p| p.pattern == null_p));
        // (metadata=present) に到達
        let present_p = StatePattern {
            values: BTreeMap::from([("metadata".to_string(), AbstractValue::Present)]),
        };
        assert!(r.patterns.iter().any(|p| p.pattern == present_p));
    }

    // ── Bool 軸の積 ─────────────────────────────────────────────────────────

    #[test]
    fn test_bool_axis() {
        let model = model_from(
            r#"
entity Switch "スイッチ" {
  id:       Int @pk
  enabled:  Bool
}
usecase Create "作成"
usecase Enable "有効化"
creates(Create, Switch)
updates(Enable, Switch)
sets(usecase::Enable, Switch, "enabled", "true")
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Switch").unwrap();
        // (enabled=false) がシード
        let false_p = StatePattern {
            values: BTreeMap::from([("enabled".to_string(), AbstractValue::Bool(false))]),
        };
        assert!(r.patterns.iter().any(|p| p.pattern == false_p));
        // (enabled=true) に到達
        let true_p = StatePattern {
            values: BTreeMap::from([("enabled".to_string(), AbstractValue::Bool(true))]),
        };
        assert!(r.patterns.iter().any(|p| p.pattern == true_p));
    }

    // ── delete → terminal ───────────────────────────────────────────────────

    #[test]
    fn test_delete_marks_terminal() {
        let model = model_from(
            r#"
entity Item "アイテム" {
  id:     Int @pk
  status: Enum(active, inactive) @default(active)
}
usecase Create   "作成"
usecase Deactive "非アクティブ化"
usecase Remove   "削除"
event Deactivate "非アクティブ化イベント"
creates(Create, Item)
updates(Deactive, Item)
deletes(Remove,   Item)
raises(Deactive, Deactivate)
state Active   "有効"
state Inactive "無効"
transitions(Deactivate, Active, Inactive)
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Item").unwrap();
        // (active) はシード, deletes が enabled → terminal
        let active_p = StatePattern {
            values: BTreeMap::from([(
                "status".to_string(),
                AbstractValue::Enum("active".to_string()),
            )]),
        };
        let entry = r.patterns.iter().find(|p| p.pattern == active_p).unwrap();
        assert!(entry.is_terminal);
    }

    // ── forbidden: 単一タプルで到達可能バリアントを禁止 ─────────────────────────

    #[test]
    fn test_forbidden_state_violated() {
        // タプル構文: forbidden(Order, (status, cancelled))
        let model = model_from(
            r#"
entity Order "注文" {
  id:     Int @pk
  status: Enum(pending, cancelled) @default(pending)
}
usecase Place  "注文確定"
usecase Cancel "キャンセル"
event EvCancel "キャンセルイベント"
creates(Place,  Order)
updates(Cancel, Order)
raises(Cancel, EvCancel)
state Pending   "受付中"
state Cancelled "キャンセル済"
transitions(EvCancel, Pending, Cancelled)
forbidden(Order, (status, cancelled))
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Order").unwrap();

        // cancelled は到達可能なので ForbiddenStateViolated が発生する
        let violated: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| matches!(d, StateDiag::ForbiddenStateViolated { .. }))
            .collect();
        assert!(
            !violated.is_empty(),
            "ForbiddenStateViolated が発生すべき: {:?}",
            r.diagnostics
        );
    }

    // ── forbidden: 複数タプルのAND組合せ禁止 ────────────────────────────────────

    #[test]
    fn test_forbidden_multi_condition_and() {
        // (status=delivered) かつ (refunded=true) の組合せのみ禁止
        // → (delivered, false) は OK、(pending, true) も OK
        let model = model_from(
            r#"
entity Order "注文" {
  id:       Int @pk
  status:   Enum(pending, delivered) @default(pending)
  refunded: Bool
}
usecase Place   "注文確定"
usecase Deliver "配達"
usecase Refund  "返金"
event EvDeliver "配達"
creates(Place,   Order)
updates(Deliver, Order)
updates(Refund,  Order)
raises(Deliver, EvDeliver)
state Pending   "受付中"
state Delivered "配達完了"
transitions(EvDeliver, Pending, Delivered)
sets(usecase::Refund, Order, "refunded", "true")
forbidden(Order, (status, delivered), (refunded, true))
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Order").unwrap();

        // (delivered, true) は到達可能なので違反
        let violated: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| matches!(d, StateDiag::ForbiddenStateViolated { .. }))
            .collect();
        assert!(
            !violated.is_empty(),
            "ForbiddenStateViolated が発生すべき: {:?}",
            r.diagnostics
        );
        let hint = violated.iter().find_map(|d| match d {
            StateDiag::ForbiddenStateViolated {
                correlation_hint: Some(hint),
                ..
            } => Some(hint),
            _ => None,
        });
        assert!(
            hint.is_some_and(|hint| hint.contains("multi-axis forbidden witnesses")),
            "multi-axis forbidden should include a correlation hint: {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn correlated_multi_axis_usecase_effects_do_not_expand_to_forbidden_product() {
        let model = model_from(
            r#"
entity DispenseReception "調剤受付" {
  id: Int @pk
  validity: Enum(active, cancelled) @default(active)
  progress: Enum(open, completed) @default(open)
  correction: Enum(none, correcting) @default(none)
  recalc: Bool
}
usecase ReceiveDispense "受付"
usecase CompleteAccounting "会計確定"
usecase CancelReception "取消"
usecase StartCorrection "訂正開始"
creates(ReceiveDispense, DispenseReception)
updates(CompleteAccounting, DispenseReception)
updates(CancelReception, DispenseReception)
updates(StartCorrection, DispenseReception)
sets(CompleteAccounting, DispenseReception, "validity", "active")
sets(CompleteAccounting, DispenseReception, "progress", "completed")
sets(CompleteAccounting, DispenseReception, "correction", "none")
sets(CompleteAccounting, DispenseReception, "recalc", "false")
sets(CancelReception, DispenseReception, "validity", "cancelled")
sets(CancelReception, DispenseReception, "progress", "open")
sets(CancelReception, DispenseReception, "correction", "none")
sets(CancelReception, DispenseReception, "recalc", "false")
sets(StartCorrection, DispenseReception, "validity", "active")
sets(StartCorrection, DispenseReception, "progress", "open")
sets(StartCorrection, DispenseReception, "correction", "correcting")
sets(StartCorrection, DispenseReception, "recalc", "true")
forbidden(DispenseReception, (validity, cancelled), (progress, completed))
forbidden(DispenseReception, (validity, cancelled), (correction, correcting))
forbidden(DispenseReception, (progress, completed), (correction, correcting))
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results
            .iter()
            .find(|r| r.entity_id == "DispenseReception")
            .unwrap();

        assert!(
            !r.diagnostics
                .iter()
                .any(|d| matches!(d, StateDiag::ForbiddenStateViolated { .. })),
            "correlated multi-axis effects should stay tuple-correlated: {:?}",
            r.diagnostics
        );
    }

    // ── forbidden: 到達不能な状態を禁止 → 違反なし ──────────────────────────────

    #[test]
    fn test_forbidden_state_no_violation_when_unreachable() {
        // inactive は creates のみで到達不能 → 違反なし
        let model = model_from(
            r#"
entity Item "アイテム" {
  id:     Int @pk
  status: Enum(active, inactive) @default(active)
}
usecase Create "作成"
creates(Create, Item)
forbidden(Item, (status, inactive))
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Item").unwrap();
        let violated: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| matches!(d, StateDiag::ForbiddenStateViolated { .. }))
            .collect();
        assert!(
            violated.is_empty(),
            "到達不能バリアントで ForbiddenStateViolated は発生しないはず: {:?}",
            r.diagnostics
        );
    }

    // ── invariant: チェーン構文で不変条件違反を検出 ──────────────────────────────

    #[test]
    fn test_invariant_violated() {
        // invariant(Order).when(status, delivered).then(delivered_at, present)
        // Deliver が delivered_at を sets しないので (delivered, null) が到達可能 → 違反
        let model = model_from(
            r#"
entity Order "注文" {
  id:           Int @pk
  status:       Enum(pending, delivered) @default(pending)
  delivered_at: DateTime @null
}
usecase Place    "注文確定"
usecase Deliver  "配達完了"
event EvDeliver  "配達イベント"
creates(Place,   Order)
updates(Deliver, Order)
raises(Deliver,  EvDeliver)
state Pending   "受付中"
state Delivered "配達完了"
transitions(EvDeliver, Pending, Delivered)
invariant(Order)
  .when(status, delivered)
  .then(delivered_at, present)
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Order").unwrap();
        let violated: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| matches!(d, StateDiag::InvariantViolated { .. }))
            .collect();
        assert!(
            !violated.is_empty(),
            "InvariantViolated が発生すべき: {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn triggered_usecase_requires_upstream_event_effect_guard() {
        let model = model_from(
            r#"
entity Terminal "端末" {
  id: Int @pk
  status: Enum(active, retired, deregistered) @default(active)
  retired_at: DateTime @null
}
usecase RegisterTerminal "登録する"
usecase RetireTerminal "退役する"
usecase DeregisterTerminal "登録解除する"
event TerminalRetired "退役済み"
creates(RegisterTerminal, Terminal)
updates(RetireTerminal, Terminal)
updates(DeregisterTerminal, Terminal)
sets(RetireTerminal, Terminal, "retired_at", "timestamptz")
sets(RetireTerminal, Terminal, "status", "retired")
sets(DeregisterTerminal, Terminal, "status", "deregistered")
raises(RetireTerminal, TerminalRetired)
triggers(TerminalRetired, DeregisterTerminal)
invariant(Terminal)
  .when(status, deregistered)
  .then(retired_at, present)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Terminal").unwrap();
        assert!(
            !r.diagnostics
                .iter()
                .any(|d| matches!(d, StateDiag::InvariantViolated { .. })),
            "triggered use case should inherit upstream evidence as a guard: {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn triggered_usecase_without_upstream_event_effect_still_violates_invariant() {
        let model = model_from(
            r#"
entity Terminal "端末" {
  id: Int @pk
  status: Enum(active, retired, deregistered) @default(active)
  retired_at: DateTime @null
}
usecase RegisterTerminal "登録する"
usecase RetireTerminal "退役する"
usecase DeregisterTerminal "登録解除する"
event TerminalRetired "退役済み"
creates(RegisterTerminal, Terminal)
updates(RetireTerminal, Terminal)
updates(DeregisterTerminal, Terminal)
sets(RetireTerminal, Terminal, "status", "retired")
sets(DeregisterTerminal, Terminal, "status", "deregistered")
raises(RetireTerminal, TerminalRetired)
triggers(TerminalRetired, DeregisterTerminal)
invariant(Terminal)
  .when(status, deregistered)
  .then(retired_at, present)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Terminal").unwrap();
        assert!(
            r.diagnostics
                .iter()
                .any(|d| matches!(d, StateDiag::InvariantViolated { .. })),
            "missing upstream evidence should still violate the invariant: {:?}",
            r.diagnostics
        );
    }

    // ── invariant: 複数 when (AND guard) を持つ不変条件 ─────────────────────────

    #[test]
    fn test_invariant_multi_guard_violated() {
        // .when(status, delivered).when(refunded, false).then(refund_id, null)
        // (delivered, false, present) が到達可能 → 違反
        let model = model_from(
            r#"
entity Order "注文" {
  id:        Int @pk
  status:    Enum(pending, delivered) @default(pending)
  refunded:  Bool
  refund_id: String @null
}
usecase Place   "注文確定"
usecase Deliver "配達"
usecase Refund  "返金"
event EvDeliver "配達"
creates(Place,   Order)
updates(Deliver, Order)
updates(Refund,  Order)
raises(Deliver, EvDeliver)
state Pending   "受付中"
state Delivered "配達完了"
transitions(EvDeliver, Pending, Delivered)
sets(usecase::Refund, Order, "refunded",  "true")
sets(usecase::Refund, Order, "refund_id", "present")
invariant(Order)
  .when(status, delivered)
  .when(refunded, false)
  .then(refund_id, null)
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Order").unwrap();
        // (delivered, false, present) のパターンは存在しない（refund_id はデフォルトnull）
        // ただし (delivered, false, null) は不変条件を満たす → 違反なし
        let violated: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| matches!(d, StateDiag::InvariantViolated { .. }))
            .collect();
        assert!(
            violated.is_empty(),
            "ガード(delivered AND false)かつ required(refund_id=null)を満たすので違反なし: {:?}",
            r.diagnostics
        );
    }

    // ── required: 全到達状態で成立すべき条件 ─────────────────────────────────

    #[test]
    fn test_required_state_violated() {
        let model = model_from(
            r#"
entity Order "注文" {
  id:     Int @pk
  status: Enum(pending, paid) @default(pending)
}
usecase Place "注文確定"
creates(Place, Order)
required(Order, (status, paid))
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Order").unwrap();
        let violated: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| matches!(d, StateDiag::RequiredStateViolated { .. }))
            .collect();
        assert!(
            !violated.is_empty(),
            "RequiredStateViolated が発生すべき: {:?}",
            r.diagnostics
        );
    }

    // ── exclusive: 条件どうしの同時成立禁止 ──────────────────────────────────

    #[test]
    fn test_exclusive_state_violated() {
        let model = model_from(
            r#"
entity Document "文書" {
  id:       Int @pk
  approved: Bool
  rejected: Bool
}
usecase Create  "作成"
usecase Approve "承認"
usecase Reject  "却下"
creates(Create, Document)
updates(Approve, Document)
updates(Reject,  Document)
sets(Approve, Document, "approved", "true")
sets(Reject,  Document, "rejected", "true")
exclusive(Document, (approved, true), (rejected, true))
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Document").unwrap();
        let violated: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| matches!(d, StateDiag::ExclusiveStateViolated { .. }))
            .collect();
        assert!(
            !violated.is_empty(),
            "ExclusiveStateViolated が発生すべき: {:?}",
            r.diagnostics
        );
    }

    // ── 比較命題軸テスト ─────────────────────────────────────────────────────

    /// 比較命題を含まないモデルで既存テストが変わらないこと
    #[test]
    fn test_no_comparison_props_unaffected() {
        let model = model_from(
            r#"
entity Order "注文" { id: Int @pk  status: Enum(pending, paid) @default(pending) }
usecase Pay "支払い"
creates(Pay, Order)
sets(Pay, Order, "status", "paid")
forbidden(Order, (status, paid))
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Order").unwrap();
        // Proposition 軸が増えていないこと
        assert!(
            r.axes
                .iter()
                .all(|ax| !matches!(ax.kind, AxisKind::Proposition { .. })),
            "比較命題のないモデルに Proposition 軸が現れた"
        );
        // forbidden が違反を検出すること
        let forbidden_diags: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| matches!(d, StateDiag::ForbiddenStateViolated { .. }))
            .collect();
        assert!(
            !forbidden_diags.is_empty(),
            "forbidden 違反が検出されなかった"
        );
    }

    /// 比較命題軸が追加されること、および BFS 後に制約チェックが正しく機能すること
    #[test]
    fn test_comparison_proposition_axis_and_violation() {
        let model = model_from(
            r#"
entity Stock "在庫" {
  id:      Int @pk
  status:  Enum(on_sale, suspended) @default(on_sale)
  stock:   Int
  selling: Int
}
usecase Open   "販売開始"
usecase Sell   "販売"
usecase Refund "返品"
buc Sales "販売業務"
contains(Sales, Open)
contains(Sales, Sell)
contains(Sales, Refund)
creates(Open,   Stock)
updates(Sell,   Stock)
updates(Refund, Stock)
sets(Open,   Stock, "status", "on_sale")
sets(Sell,   Stock, stock < selling, true)
sets(Refund, Stock, stock < selling, false)
forbidden(Stock, (status, on_sale), stock < selling)
invariant(Stock)
  .when(status, on_sale)
  .then(stock < selling)
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Stock").unwrap();

        // Proposition 軸が1本追加されていること
        let prop_axes: Vec<_> = r
            .axes
            .iter()
            .filter(|ax| matches!(&ax.kind, AxisKind::Proposition { .. }))
            .collect();
        assert_eq!(prop_axes.len(), 1, "Proposition 軸が1本あるはず");
        if let AxisKind::Proposition { axis_key } = &prop_axes[0].kind {
            assert_eq!(axis_key, "stock<selling");
        }

        // forbidden 違反が検出されること（status=on_sale かつ stock<selling=true）
        let forbidden_diags: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| matches!(d, StateDiag::ForbiddenStateViolated { .. }))
            .collect();
        assert!(
            !forbidden_diags.is_empty(),
            "forbidden 違反が検出されなかった"
        );
        let hint = forbidden_diags.iter().find_map(|d| match d {
            StateDiag::ForbiddenStateViolated {
                correlation_hint: Some(hint),
                ..
            } => Some(hint),
            _ => None,
        });
        assert!(
            hint.is_some_and(|hint| {
                hint.contains("comparison propositions")
                    && hint.contains("drive the comparison true/false in the same usecase")
            }),
            "comparison-proposition forbidden violation should include a specific hint: {:?}",
            r.diagnostics
        );

        // invariant 違反が検出されること（status=on_sale で stock<selling=false の到達可能状態）
        let invariant_diags: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| matches!(d, StateDiag::InvariantViolated { .. }))
            .collect();
        assert!(
            !invariant_diags.is_empty(),
            "invariant 違反が検出されなかった"
        );
    }

    #[test]
    fn comparison_proposition_effect_merges_with_create_operation() {
        let model = model_from(
            r#"
entity Stock "在庫" {
  id: Int @pk
  stock: Int
  selling: Int
  status: Enum(draft, on_sale) @default(draft)
}
usecase ListItem "出品"
creates(ListItem, Stock)
sets(ListItem, Stock, "status", "on_sale")
sets(ListItem, Stock, stock < selling, true)
invariant(Stock)
  .when(status, on_sale)
  .then(stock < selling)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Stock").unwrap();
        assert!(!r
            .diagnostics
            .iter()
            .any(|diag| matches!(diag, StateDiag::InvariantViolated { .. })));
    }

    /// `now` 比較命題が正しく Proposition 軸として追加されること
    #[test]
    fn test_now_comparison_proposition() {
        let model = model_from(
            r#"
entity Coupon "クーポン" {
  id:         Int @pk
  status:     Enum(usable, expired) @default(usable)
  expired_at: DateTime @null
}
usecase Expire "期限切れ"
buc CouponMgmt "管理"
contains(CouponMgmt, Expire)
creates(Expire, Coupon)
updates(Expire, Coupon)
sets(Expire, Coupon, expired_at < now, true)
forbidden(Coupon, (status, usable), expired_at < now)
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Coupon").unwrap();

        let prop_axes: Vec<_> = r
            .axes
            .iter()
            .filter(|ax| matches!(&ax.kind, AxisKind::Proposition { .. }))
            .collect();
        assert_eq!(prop_axes.len(), 1, "Proposition 軸が1本あるはず");
        if let AxisKind::Proposition { axis_key } = &prop_axes[0].kind {
            assert_eq!(axis_key, "expired_at<now");
        }

        // forbidden 違反が検出されること
        let forbidden_diags: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| matches!(d, StateDiag::ForbiddenStateViolated { .. }))
            .collect();
        assert!(
            !forbidden_diags.is_empty(),
            "forbidden 違反が検出されなかった"
        );
    }

    #[test]
    fn test_invariant_satisfied() {
        // delivered になるときは必ず delivered_at も sets → 不変条件を満たす
        let model = model_from(
            r#"
entity Order "注文" {
  id:           Int @pk
  status:       Enum(pending, delivered) @default(pending)
  delivered_at: DateTime @null
}
usecase Place    "注文確定"
usecase Deliver  "配達完了"
event EvDeliver  "配達イベント"
creates(Place,   Order)
updates(Deliver, Order)
raises(Deliver,  EvDeliver)
state Pending   "受付中"
state Delivered "配達完了"
transitions(EvDeliver, Pending, Delivered)
sets(usecase::Deliver, Order, "delivered_at", "present")
invariant(Order)
  .when(status, delivered)
  .then(delivered_at, present)
"#,
        );
        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        let r = results.iter().find(|r| r.entity_id == "Order").unwrap();
        let violated: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| matches!(d, StateDiag::InvariantViolated { .. }))
            .collect();
        assert!(
            violated.is_empty(),
            "不変条件を満たしているので InvariantViolated は発生しないはず: {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn test_cross_forbidden_is_reported_on_each_entity_result() {
        let model = model_from(
            r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(paid, cancelled) @default(cancelled)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured) @default(pending)
}
cross_forbidden(Order, Payment,
  (Order.status, cancelled),
  (Payment.status, pending))
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for entity_id in ["Order", "Payment"] {
            let r = results.iter().find(|r| r.entity_id == entity_id).unwrap();
            assert!(
                r.diagnostics
                    .iter()
                    .any(|d| matches!(d, StateDiag::CrossForbiddenViolated { .. })),
                "cross forbidden violation should be reported on {entity_id}: {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn test_cross_invariant_is_reported_on_each_entity_result() {
        let model = model_from(
            r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid) @default(paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured) @default(pending)
}
cross_invariant(Order, Payment)
  .when(Order.status, paid)
  .then(Payment.status, captured)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for entity_id in ["Order", "Payment"] {
            let r = results.iter().find(|r| r.entity_id == entity_id).unwrap();
            assert!(
                r.diagnostics
                    .iter()
                    .any(|d| matches!(d, StateDiag::CrossInvariantViolated { .. })),
                "cross invariant violation should be reported on {entity_id}: {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn test_global_cross_invariant_on_related_entities_is_not_evaluated() {
        let model = model_from(
            r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid) @default(paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured) @default(pending)
}
relate(Payment, Order, "1:1")
cross_invariant(Order, Payment)
  .when(Order.status, paid)
  .then(Payment.status, captured)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for entity_id in ["Order", "Payment"] {
            let r = results.iter().find(|r| r.entity_id == entity_id).unwrap();
            let reason = r.diagnostics.iter().find_map(|d| match d {
                StateDiag::CrossConstraintNotEvaluated { reason, .. } => Some(reason),
                _ => None,
            });
            assert!(
                reason.is_some_and(|reason| {
                    reason.contains("global cross-product has a witness across related entities")
                        && reason.contains("use .along(Order, Payment)")
                }),
                "related global-product witness should be not evaluated on {entity_id}: {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn test_global_cross_forbidden_on_related_entities_is_not_evaluated() {
        let model = model_from(
            r#"
entity Terminal "端末" {
  id: Int @pk
  status: Enum(active, deregistered) @default(deregistered)
}
entity ClientCertificate "証明書" {
  id: Int @pk
  status: Enum(active, revoked) @default(active)
}
relate(Terminal, ClientCertificate, "1:N")
cross_forbidden(Terminal, ClientCertificate,
  (Terminal.status, deregistered),
  (ClientCertificate.status, active))
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for entity_id in ["Terminal", "ClientCertificate"] {
            let r = results.iter().find(|r| r.entity_id == entity_id).unwrap();
            let reason = r.diagnostics.iter().find_map(|d| match d {
                StateDiag::CrossConstraintNotEvaluated { reason, .. } => Some(reason),
                _ => None,
            });
            assert!(
                reason.is_some_and(|reason| {
                    reason.contains("global cross-product has a witness across related entities")
                        && reason.contains("use .along(Terminal, ClientCertificate)")
                }),
                "related global-product witness should be not evaluated on {entity_id}: {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn test_cross_constraint_with_non_state_comparison_is_reported_as_not_evaluated() {
        let model = model_from(
            r#"
entity Order "注文" {
  id: Int @pk
  total: Decimal
}
entity Payment "支払い" {
  id: Int @pk
  amount: Decimal
}
cross_forbidden(Order, Payment, Payment.amount > Order.total)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for entity_id in ["Order", "Payment"] {
            let r = results.iter().find(|r| r.entity_id == entity_id).unwrap();
            assert!(
                r.diagnostics
                    .iter()
                    .any(|d| matches!(d, StateDiag::CrossConstraintNotEvaluated { .. })),
                "non-state cross comparison should be reported on {entity_id}: {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn test_relation_scoped_cross_invariant_is_reported_as_not_evaluated() {
        let model = model_from(
            r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid) @default(paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured) @default(pending)
}
relate(Payment, Order, "1:1")
cross_invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status, paid)
  .then(Payment.status, captured)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for entity_id in ["Order", "Payment"] {
            let r = results.iter().find(|r| r.entity_id == entity_id).unwrap();
            assert!(
                r.diagnostics.iter().any(|d| matches!(
                    d,
                    StateDiag::CrossConstraintNotEvaluated { reason, .. }
                        if reason.contains("linked instance reachability is not yet evaluated")
                )),
                "relation-scoped cross invariant should avoid global-product violation on {entity_id}: {:?}",
                r.diagnostics
            );
            assert!(
                !r.diagnostics
                    .iter()
                    .any(|d| matches!(d, StateDiag::CrossInvariantViolated { .. })),
                "relation-scoped cross invariant must not fall back to global-product evaluation on {entity_id}: {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn test_relation_scoped_cross_invariant_reports_operation_linked_violation() {
        let model = model_from(
            r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid) @default(paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured) @default(pending)
}
usecase RegisterPayment "登録"
creates(RegisterPayment, Order)
creates(RegisterPayment, Payment)
relate(Payment, Order, "1:1")
cross_invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status, paid)
  .then(Payment.status, captured)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for entity_id in ["Order", "Payment"] {
            let r = results.iter().find(|r| r.entity_id == entity_id).unwrap();
            assert!(
                r.diagnostics
                    .iter()
                    .any(|d| matches!(d, StateDiag::CrossInvariantViolated { .. })),
                "operation-linked relation-scoped invariant should report violation on {entity_id}: {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn test_relation_scoped_cross_forbidden_reports_operation_linked_violation() {
        let model = model_from(
            r#"
entity Terminal "端末" {
  id: Int @pk
  status: Enum(active, deregistered) @default(deregistered)
}
entity ClientCertificate "証明書" {
  id: Int @pk
  status: Enum(active, suspended) @default(active)
}
usecase RegisterTerminalCertificate "登録"
creates(RegisterTerminalCertificate, Terminal)
creates(RegisterTerminalCertificate, ClientCertificate)
relate(Terminal, ClientCertificate, "1:N")
cross_forbidden(Terminal, ClientCertificate,
  (Terminal.status, deregistered),
  (ClientCertificate.status, active))
  .along(Terminal, ClientCertificate)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for entity_id in ["Terminal", "ClientCertificate"] {
            let r = results.iter().find(|r| r.entity_id == entity_id).unwrap();
            assert!(
                r.diagnostics
                    .iter()
                    .any(|d| matches!(d, StateDiag::CrossForbiddenViolated { .. })),
                "operation-linked relation-scoped forbidden should report violation on {entity_id}: {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn test_relation_scoped_cross_invariant_requires_declared_relate_path() {
        let model = model_from(
            r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid) @default(paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured) @default(pending)
}
cross_invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status, paid)
  .then(Payment.status, captured)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for entity_id in ["Order", "Payment"] {
            let r = results.iter().find(|r| r.entity_id == entity_id).unwrap();
            assert!(
                r.diagnostics.iter().any(|d| matches!(
                    d,
                    StateDiag::CrossConstraintNotEvaluated { reason, .. }
                        if reason.contains("references no relate path")
                )),
                "missing relation path should be reported on {entity_id}: {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn test_relation_scoped_cross_invariant_passes_when_global_product_has_no_violation() {
        let model = model_from(
            r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid) @default(paid)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured) @default(captured)
}
relate(Payment, Order, "1:1")
cross_invariant(Order, Payment)
  .along(Order, Payment)
  .when(Order.status, paid)
  .then(Payment.status, captured)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for entity_id in ["Order", "Payment"] {
            let r = results.iter().find(|r| r.entity_id == entity_id).unwrap();
            assert!(
                !r.diagnostics.iter().any(|d| matches!(
                    d,
                    StateDiag::CrossInvariantViolated { .. }
                        | StateDiag::CrossConstraintNotEvaluated { .. }
                )),
                "relation-scoped invariant should be known satisfied on {entity_id}: {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn test_relation_scoped_cross_forbidden_passes_when_global_product_has_no_witness() {
        let model = model_from(
            r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid) @default(open)
}
entity Payment "支払い" {
  id: Int @pk
  status: Enum(pending, captured) @default(pending)
}
relate(Payment, Order, "1:1")
cross_forbidden(Order, Payment,
  (Order.status, paid),
  (Payment.status, pending))
  .along(Order, Payment)
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for entity_id in ["Order", "Payment"] {
            let r = results.iter().find(|r| r.entity_id == entity_id).unwrap();
            assert!(
                !r.diagnostics.iter().any(|d| matches!(
                    d,
                    StateDiag::CrossForbiddenViolated { .. }
                        | StateDiag::CrossConstraintNotEvaluated { .. }
                )),
                "relation-scoped forbidden should be known satisfied on {entity_id}: {:?}",
                r.diagnostics
            );
        }
    }

    #[test]
    fn test_cross_unknown_condition_does_not_mask_false_state_condition() {
        let model = model_from(
            r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(open, paid) @default(open)
  total: Decimal
}
entity Payment "支払い" {
  id: Int @pk
  amount: Decimal
}
cross_forbidden(Order, Payment,
  Payment.amount > Order.total,
  (Order.status, paid))
"#,
        );

        let results = derive_state_patterns(&model, &[], DEFAULT_PATTERN_CAP);
        for entity_id in ["Order", "Payment"] {
            let r = results.iter().find(|r| r.entity_id == entity_id).unwrap();
            assert!(
                !r.diagnostics.iter().any(|d| matches!(
                    d,
                    StateDiag::CrossForbiddenViolated { .. }
                        | StateDiag::CrossConstraintNotEvaluated { .. }
                )),
                "false state-axis condition should make the cross constraint known-false for {entity_id}: {:?}",
                r.diagnostics
            );
        }
    }
}
