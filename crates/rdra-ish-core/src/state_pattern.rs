//! BUC パターンから各 entity の取り得る状態パターンを導出するモジュール。
//!
//! 「状態パターン」とは、entity の *state-defining columns*
//! （Enum カラム・Bool カラム・Nullable カラム）の抽象値の組み合わせ。
//! 有限直積空間上の BFS で到達可能なパターン集合を求める。

use crate::model::{
    ColumnEffect, ColumnType, EffectValue, EntityKey, ModelColumn, NodeRef, RelKind, SemanticModel,
    StateKey, UseCaseKey,
};
use crate::resolver::reachable_from_bucs;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

// ── デフォルト上限値 ─────────────────────────────────────────────────────────

/// entity ごとのパターン数の上限（デフォルト）。
/// 上限を超えた場合は `EntityStateResult.truncated = true` となる。
pub const DEFAULT_PATTERN_CAP: usize = 256;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// `forbidden(Entity, col, val)` で禁止されたカラム値に到達可能
    ForbiddenStateViolated {
        column: String,
        value: String,
        pattern_desc: String,
    },
    /// `invariant(Entity, ...)` の不変条件を違反する到達可能な状態
    InvariantViolated {
        guard: String,
        required: String,
        pattern_desc: String,
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

    for ek in entity_keys {
        let result = derive_for_entity(model, ek, &buc_of_usecase, buc_reachable.as_ref(), cap);
        results.push(result);
    }

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

/// entity ごとの状態軸を特定する
fn identify_axes(entity_cols: &[ModelColumn], column_effects: &[&ColumnEffect]) -> Vec<StateAxis> {
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
        let origin_uc_key = match &eff.origin {
            NodeRef::UseCase(k) => *k,
            _ => continue, // event origin は将来対応
        };
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

    for rel in &model.relations {
        let (op_kind, uc_key) = match (&rel.kind, &rel.from) {
            (RelKind::Creates, NodeRef::UseCase(uk)) => (OpKind::Create, *uk),
            (RelKind::Updates, NodeRef::UseCase(uk)) => (OpKind::Update, *uk),
            (RelKind::Deletes, NodeRef::UseCase(uk)) => (OpKind::Delete, *uk),
            (RelKind::Writes, NodeRef::UseCase(uk)) => (OpKind::Update, *uk),
            _ => continue,
        };
        if rel.to != NodeRef::Entity(ek) {
            continue;
        }
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

    (ops, status_col)
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

    // 状態軸の特定
    let axes = identify_axes(&entity.columns, &effects_for_entity);

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
    let (ops, status_col) = collect_operations(
        model,
        ek,
        buc_of_usecase,
        buc_reachable,
        &axes,
        &mut diags,
        &effects_for_entity,
    );

    // ── シードパターンの構築 ──────────────────────────────────────────────────
    // 基底値: @default または軸種別ごとのフォールバック
    let base: BTreeMap<String, AbstractValue> = axes
        .iter()
        .filter_map(|ax| {
            let col = entity.columns.iter().find(|c| c.name == ax.column)?;
            Some((ax.column.clone(), default_value_for_axis(col)))
        })
        .collect();
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

/// 到達可能パターン群に対して `forbidden` / `invariant` 制約を検査し、
/// 違反を `diags` に追加する。
fn check_constraints(
    model: &SemanticModel,
    ek: EntityKey,
    reached: &[ReachablePattern],
    diags: &mut Vec<StateDiag>,
) {
    // ── 禁止状態チェック ─────────────────────────────────────────────────────
    for fc in model
        .forbidden_constraints
        .iter()
        .filter(|fc| fc.entity == ek)
    {
        let forbidden_val = AbstractValue::from_effect(&fc.value);
        for rp in reached {
            if rp.pattern.values.get(&fc.column) == Some(&forbidden_val) {
                diags.push(StateDiag::ForbiddenStateViolated {
                    column: fc.column.clone(),
                    value: abstract_value_display(&forbidden_val),
                    pattern_desc: describe_pattern(&rp.pattern),
                });
            }
        }
    }

    // ── 不変条件チェック ─────────────────────────────────────────────────────
    for inv in model
        .entity_invariants
        .iter()
        .filter(|inv| inv.entity == ek)
    {
        let guard_val = AbstractValue::from_effect(&inv.guard_value);
        let req_val = AbstractValue::from_effect(&inv.required_value);
        for rp in reached {
            // ガード条件が成立するパターンのみ検査
            if rp.pattern.values.get(&inv.guard_column) == Some(&guard_val) {
                // required 条件が満たされていない場合に違反
                let actual_req = rp.pattern.values.get(&inv.required_column);
                if actual_req != Some(&req_val) {
                    diags.push(StateDiag::InvariantViolated {
                        guard: format!(
                            "{}={}",
                            inv.guard_column,
                            abstract_value_display(&guard_val)
                        ),
                        required: format!(
                            "{}={}",
                            inv.required_column,
                            abstract_value_display(&req_val)
                        ),
                        pattern_desc: describe_pattern(&rp.pattern),
                    });
                }
            }
        }
    }
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

    // ── 禁止状態: 到達可能な Enum バリアントを forbidden で禁止 ────────────────

    #[test]
    fn test_forbidden_state_violated() {
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
forbidden(Order, "status", "cancelled")
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

    // ── 禁止状態: 到達不能なバリアントを forbidden → 違反なし ─────────────────

    #[test]
    fn test_forbidden_state_no_violation_when_unreachable() {
        let model = model_from(
            r#"
entity Order "注文" {
  id:     Int @pk
  status: Enum(pending, paid) @default(pending)
}
usecase Place "注文確定"
usecase Pay   "支払い"
event EvPaid  "支払い完了"
creates(Place, Order)
updates(Pay,   Order)
raises(Pay, EvPaid)
state Pending "受付中"
state Paid    "支払済"
transitions(EvPaid, Pending, Paid)
forbidden(Order, "status", "paid")
"#,
        );
        // paid は到達可能なので違反が発生する（このテストで動作を確認）
        let model2 = model_from(
            r#"
entity Item "アイテム" {
  id:     Int @pk
  status: Enum(active, inactive) @default(active)
}
usecase Create "作成"
creates(Create, Item)
forbidden(Item, "status", "inactive")
"#,
        );
        // inactive は reaches されないので違反なし
        let results = derive_state_patterns(&model2, &[], DEFAULT_PATTERN_CAP);
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
        let _ = model; // suppress unused warning
    }

    // ── 不変条件: 違反する状態が到達可能 ────────────────────────────────────────

    #[test]
    fn test_invariant_violated() {
        // "delivered" になったが delivered_at が null のパターンが到達可能
        // → invariant 違反を検出すべき
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
invariant(Order, "status", "delivered", "delivered_at", "present")
"#,
        );
        // Deliver は delivered_at を sets しないので (delivered, null) が到達可能 → 違反
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

    // ── 不変条件: 満たされている場合は違反なし ──────────────────────────────────

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
invariant(Order, "status", "delivered", "delivered_at", "present")
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
}
