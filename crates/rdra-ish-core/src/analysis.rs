use crate::diagnostics::*;
use crate::model::*;
use rdra_ish_syntax::ast::*;

// ── PostgreSQL 特殊型の語彙 ────────────────────────────────────────────────────

/// PostgreSQL の特殊型名かどうかを判定するホワイトリスト。
/// `sets(...)` の値引数としてこれらの型名を許可し、`TypedPresent(型名)` に変換する。
/// nullable カラムのみに適用される。
fn is_pg_special_type(s: &str) -> bool {
    matches!(
        s,
        "json"
            | "jsonb"
            | "uuid"
            | "timestamptz"
            | "timestamp"
            | "timetz"
            | "time"
            | "date"
            | "interval"
            | "inet"
            | "cidr"
            | "macaddr"
            | "macaddr8"
            | "bytea"
            | "tsvector"
            | "tsquery"
            | "xml"
            | "point"
            | "line"
            | "lseg"
            | "box"
            | "path"
            | "polygon"
            | "circle"
            | "money"
            | "bit"
            | "varbit"
            | "int4range"
            | "int8range"
            | "numrange"
            | "tsrange"
            | "tstzrange"
            | "daterange"
    )
}

// ── sets(...)  値パース ────────────────────────────────────────────────────────

/// `sets(...)` 述語の第4引数（値 lit）を `EffectValue` に変換する。
/// - `null` / `present` / PG 特殊型名 → nullable カラムのみ許可
/// - enum バリアント → Enum カラムのみ許可
/// - `true` / `false` → Bool カラムのみ許可
/// - それ以外 → エラー
fn parse_effect_value(col: &ModelColumn, lit: &str) -> Result<EffectValue, RdraError> {
    // ── null/present ──────────────────────────────────────────────────────────
    match lit {
        "null" => {
            return if col.is_nullable {
                Ok(EffectValue::Null)
            } else {
                Err(RdraError::NullOnNonNullable {
                    col: col.name.clone(),
                })
            };
        }
        "present" => {
            return if col.is_nullable {
                Ok(EffectValue::Present)
            } else {
                Err(RdraError::NullOnNonNullable {
                    col: col.name.clone(),
                })
            };
        }
        _ => {}
    }

    // ── PostgreSQL 特殊型名 ────────────────────────────────────────────────────
    if is_pg_special_type(lit) {
        return if col.is_nullable {
            Ok(EffectValue::TypedPresent(lit.to_string()))
        } else {
            Err(RdraError::NullOnNonNullable {
                col: col.name.clone(),
            })
        };
    }

    // ── カラム型によるバリアント判定 ──────────────────────────────────────────
    match &col.col_type {
        ColumnType::Enum(variants) => {
            if variants.iter().any(|v| v == lit) {
                Ok(EffectValue::EnumVariant(lit.to_string()))
            } else {
                Err(RdraError::InvalidEnumVariant {
                    col: col.name.clone(),
                    value: lit.to_string(),
                    allowed: variants.join(", "),
                })
            }
        }
        ColumnType::Bool => match lit {
            "true" => Ok(EffectValue::Bool(true)),
            "false" => Ok(EffectValue::Bool(false)),
            _ => Err(RdraError::InvalidBoolValue {
                col: col.name.clone(),
                value: lit.to_string(),
            }),
        },
        _ => {
            // nullable でも Enum/Bool でもない非状態カラム
            Err(RdraError::EffectOnNonStateColumn {
                col: col.name.clone(),
                col_type: format!("{:?}", col.col_type),
            })
        }
    }
}

/// 各述語が期待する引数の「kind文字列」
fn predicate_signature(pred: &str) -> Option<Vec<Vec<&'static str>>> {
    match pred {
        "performs" => Some(vec![vec!["actor"], vec!["usecase", "buc"]]),
        "uses" => Some(vec![vec!["actor"], vec!["extsystem"]]),
        "reads" | "writes" | "creates" | "updates" | "deletes" => {
            Some(vec![vec!["usecase", "api"], vec!["entity"]])
        }
        "invokes" => Some(vec![vec!["usecase"], vec!["api"]]),
        "displays" => Some(vec![vec!["usecase"], vec!["screen"]]),
        "shows" => Some(vec![vec!["screen"], vec!["entity"]]),
        "raises" => Some(vec![vec!["usecase"], vec!["event"]]),
        "triggers" => Some(vec![vec!["event"], vec!["usecase", "buc"]]),
        "contains" => Some(vec![vec!["buc", "system"], vec!["usecase", "api"]]),
        "coordinates" => Some(vec![vec!["usecase"], vec!["entity"], vec!["entity"]]),
        "belongs" => Some(vec![vec!["buc"], vec!["business"]]),
        "has_permission" => Some(vec![vec!["actor"], vec!["permission"]]),
        "requires_permission" => Some(vec![vec!["usecase", "api"], vec!["permission"]]),
        "requires_medium" => Some(vec![vec!["usecase", "api"], vec!["medium"]]),
        "motivates" => Some(vec![vec!["requirement"], vec!["buc"]]),
        "transitions" => Some(vec![vec!["event"], vec!["state"], vec!["state"]]),
        "relate" => Some(vec![vec!["entity"], vec!["entity"], vec!["_card"]]),
        // sets(usecase/event, entity, "col_name", "value") — 第3・第4引数はリテラル
        "sets" => Some(vec![
            vec!["usecase", "event"],
            vec!["entity"],
            vec!["_col"],
            vec!["_val"],
        ]),
        // forbidden(entity, (col, val), ...) — 条件AND組合せへの到達を禁止する
        // invariant(entity).when(col, val).then(col, val) — チェーン形式
        // どちらも第1引数の entity のみ型検査する
        "forbidden" | "invariant" => Some(vec![vec!["entity"]]),
        _ => None,
    }
}

pub fn build_model(ast: &Ast) -> (SemanticModel, Vec<Diagnostic>) {
    let mut model = SemanticModel::default();
    let mut diags: Vec<Diagnostic> = vec![];

    // Pass 1: インスタンス宣言 → モデルへ登録
    for item in &ast.items {
        if let Item::Instance(inst) = item {
            register_instance(&mut model, inst, &mut diags);
        }
    }

    // Pass 2: 述語呼び出し → 型検査 + リレーション登録
    for item in &ast.items {
        if let Item::Predicate(pred) = item {
            process_predicate(&mut model, pred, &mut diags);
        }
    }

    // Pass 3: FK生成（relate N:1 / 1:N）
    generate_fks(&mut model, &mut diags);

    (model, diags)
}

fn register_instance(model: &mut SemanticModel, inst: &InstanceDecl, diags: &mut Vec<Diagnostic>) {
    let node = match inst.kind {
        Kind::Actor => {
            let k = model.actors.insert(Actor {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Actor(k)
        }
        Kind::ExtSystem => {
            let k = model.ext_systems.insert(ExtSystem {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::ExtSystem(k)
        }
        Kind::System => {
            let k = model.systems.insert(System {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::System(k)
        }
        Kind::Requirement => {
            let k = model.requirements.insert(Requirement {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Requirement(k)
        }
        Kind::Business => {
            let k = model.businesses.insert(Business {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Business(k)
        }
        Kind::Buc => {
            let k = model.bucs.insert(Buc {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Buc(k)
        }
        Kind::UsageScene => {
            let k = model.usage_scenes.insert(UsageScene {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::UsageScene(k)
        }
        Kind::UseCase => {
            let k = model.use_cases.insert(UseCase {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::UseCase(k)
        }
        Kind::Screen => {
            let k = model.screens.insert(Screen {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Screen(k)
        }
        Kind::Event => {
            let k = model.events.insert(Event {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Event(k)
        }
        Kind::Entity => {
            let columns = inst.columns.iter().map(ast_column_to_model).collect();
            let k = model.entities.insert(Entity {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
                columns,
            });
            NodeRef::Entity(k)
        }
        Kind::State => {
            let k = model.states.insert(State {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::State(k)
        }
        Kind::Condition => {
            let k = model.conditions.insert(Condition {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Condition(k)
        }
        Kind::Variation => {
            let k = model.variations.insert(Variation {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Variation(k)
        }
        Kind::Api => {
            let k = model.apis.insert(Api {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Api(k)
        }
        Kind::Location => {
            let k = model.locations.insert(Location {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Location(k)
        }
        Kind::Timing => {
            let k = model.timings.insert(Timing {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Timing(k)
        }
        Kind::Medium => {
            let k = model.media.insert(Medium {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Medium(k)
        }
        Kind::Permission => {
            let k = model.permissions.insert(Permission {
                id: inst.id.clone(),
                label: inst.label.clone(),
                description: inst.description.clone(),
            });
            NodeRef::Permission(k)
        }
    };

    if model.symbols.insert(inst.id.clone(), node) {
        diags.push(Diagnostic::error(RdraError::DuplicateDefinition {
            id: inst.id.clone(),
        }));
    }
}

fn ast_column_to_model(col: &Column) -> ModelColumn {
    let col_type = match &col.col_type {
        ColType::Int => ColumnType::Int,
        ColType::String => ColumnType::String,
        ColType::Money => ColumnType::Money,
        ColType::DateTime => ColumnType::DateTime,
        ColType::Date => ColumnType::Date,
        ColType::Bool => ColumnType::Bool,
        ColType::Decimal => ColumnType::Decimal,
        ColType::Enum(vs) => ColumnType::Enum(vs.clone()),
    };
    let mut mc = ModelColumn {
        name: col.name.clone(),
        col_type,
        is_pk: false,
        is_unique: false,
        is_nullable: false,
        default_val: None,
        label: None,
        is_fk: false,
        fk_target: None,
    };
    for ann in &col.annotations {
        match ann {
            Annotation::Pk | Annotation::PkComposite(_) => mc.is_pk = true,
            Annotation::Unique => mc.is_unique = true,
            Annotation::Null => mc.is_nullable = true,
            Annotation::Default(v) => mc.default_val = Some(v.clone()),
            Annotation::Label(l) => mc.label = Some(l.clone()),
        }
    }
    mc
}

fn resolve_arg(
    model: &SemanticModel,
    arg: &PredicateArg,
    diags: &mut Vec<Diagnostic>,
) -> Option<NodeRef> {
    match arg {
        PredicateArg::Lit(_) => None,
        PredicateArg::Tuple(_) => None, // タプルはシンボル解決しない
        PredicateArg::Expr(_) => None,  // 比較式はシンボル解決しない
        PredicateArg::Ref(qref) => {
            let id = qref.parts.last().unwrap();

            if let Some(kind) = &qref.kind_qualifier {
                // Kind-qualified: `usecase::Foo` — exact lookup
                model
                    .symbols
                    .lookup_qualified(kind, id)
                    .cloned()
                    .or_else(|| {
                        diags.push(Diagnostic::error(RdraError::UndefinedSymbol {
                            id: format!("{}::{}", kind.name(), id),
                        }));
                        None
                    })
            } else {
                // Unqualified: `Foo` or `a.Foo`
                match model.symbols.lookup(id) {
                    LookupResult::Found(n) => Some(n.clone()),
                    LookupResult::NotFound => {
                        diags.push(Diagnostic::error(RdraError::UndefinedSymbol {
                            id: id.clone(),
                        }));
                        None
                    }
                    LookupResult::Ambiguous(kinds) => {
                        diags.push(Diagnostic::error(RdraError::AmbiguousReference {
                            id: id.clone(),
                            kinds: kinds.join(", "),
                        }));
                        None
                    }
                }
            }
        }
    }
}

// ── 制約述語用ヘルパー ────────────────────────────────────────────────────────

/// `Lit(s)` または kind修飾なし1セグメントの `Ref` から文字列を取り出す。
/// `when(status, delivered)` の裸ident引数と `sets(...)` の引用符付きリテラル
/// 引数の両方を許容するための統一抽出。
fn arg_as_str(arg: &PredicateArg) -> Option<String> {
    match arg {
        PredicateArg::Lit(s) => Some(s.clone()),
        PredicateArg::Ref(qref) if qref.kind_qualifier.is_none() && qref.parts.len() == 1 => {
            Some(qref.parts[0].clone())
        }
        _ => None,
    }
}

/// `Tuple([a, b])` から (col文字列, val文字列) を取り出す。
/// 要素数が2でない場合、または atom が文字列化できない場合は `None`。
fn tuple_pair(arg: &PredicateArg) -> Option<(String, String)> {
    match arg {
        PredicateArg::Tuple(elems) if elems.len() == 2 => {
            let col = arg_as_str(&elems[0])?;
            let val = arg_as_str(&elems[1])?;
            Some((col, val))
        }
        _ => None,
    }
}

fn context_value_from_arg(
    model: &SemanticModel,
    arg: &PredicateArg,
    expected_kind: &str,
    diags: &mut Vec<Diagnostic>,
) -> Option<BusinessMappingContextValue> {
    match arg {
        PredicateArg::Lit(s) => Some(BusinessMappingContextValue::Text(s.clone())),
        PredicateArg::Ref(_) => {
            let node = resolve_arg(model, arg, diags)?;
            let actual = node_kind_tag_str(&node);
            if actual != expected_kind {
                diags.push(Diagnostic::error(RdraError::TypeMismatch {
                    pred: "belongs context".to_string(),
                    id: context_arg_id(arg),
                    actual: actual.to_string(),
                    expected: expected_kind.to_string(),
                }));
                return None;
            }
            Some(BusinessMappingContextValue::Ref(node))
        }
        PredicateArg::Tuple(_) | PredicateArg::Expr(_) => None,
    }
}

fn context_arg_id(arg: &PredicateArg) -> String {
    match arg {
        PredicateArg::Ref(q) => {
            let id = q.parts.last().cloned().unwrap_or_default();
            match &q.kind_qualifier {
                Some(k) => format!("{}::{}", k.name(), id),
                None => id,
            }
        }
        PredicateArg::Lit(s) => s.clone(),
        PredicateArg::Tuple(_) => "<tuple>".to_string(),
        PredicateArg::Expr(_) => "<expr>".to_string(),
    }
}

// ── 比較式の型整合チェック・モデル変換 ────────────────────────────────────────

/// `ColumnType` が「比較に使える型カテゴリ」を返す。
/// - `"numeric"`: Int/Money/Decimal
/// - `"temporal"`: Date/DateTime
/// - `"equality"`: それ以外（等値比較 == / != のみ許容）
/// - `"none"`: 比較不可（比較を拒否）
fn type_category(col_type: &ColumnType) -> &'static str {
    match col_type {
        ColumnType::Int | ColumnType::Money | ColumnType::Decimal => "numeric",
        ColumnType::Date | ColumnType::DateTime => "temporal",
        ColumnType::String | ColumnType::Bool | ColumnType::Enum(_) => "equality",
    }
}

/// `CmpOp` が順序比較か（`<`, `>`, `<=`, `>=`）。
fn is_order_op(op: &CmpOp) -> bool {
    matches!(op, CmpOp::Lt | CmpOp::Gt | CmpOp::Le | CmpOp::Ge)
}

/// `ast::CmpOp` → `model::CmpOpModel` への変換。
fn to_model_op(op: &CmpOp) -> CmpOpModel {
    match op {
        CmpOp::Lt => CmpOpModel::Lt,
        CmpOp::Gt => CmpOpModel::Gt,
        CmpOp::Le => CmpOpModel::Le,
        CmpOp::Ge => CmpOpModel::Ge,
        CmpOp::Eq => CmpOpModel::Eq,
        CmpOp::Ne => CmpOpModel::Ne,
    }
}

/// 比較式 `Comparison` を解析して `ComparisonProp` に変換する。
///
/// - 左辺はカラム参照必須。
/// - 演算子と右辺の型整合を検査する。
/// - 型不整合があれば `diags` にエラーを push し `None` を返す。
fn resolve_comparison(
    entity_cols: &[ModelColumn],
    entity_id: &str,
    cmp: &Comparison,
    diags: &mut Vec<Diagnostic>,
) -> Option<ComparisonProp> {
    // ── 左辺はカラム参照のみ ──────────────────────────────────────────────────
    let lhs_col_name = match &cmp.lhs {
        Operand::Column(name) => name.clone(),
        _ => {
            diags.push(Diagnostic::error(RdraError::ComparisonLhsMustBeColumn));
            return None;
        }
    };

    // 左辺カラムを解決
    let lhs_col = match entity_cols.iter().find(|c| c.name == lhs_col_name) {
        Some(c) => c,
        None => {
            diags.push(Diagnostic::error(RdraError::UnknownColumn {
                entity: entity_id.to_string(),
                col: lhs_col_name.clone(),
            }));
            return None;
        }
    };

    let lhs_cat = type_category(&lhs_col.col_type);

    // 順序比較演算子が使えない型か確認
    if is_order_op(&cmp.op) && lhs_cat == "equality" {
        diags.push(Diagnostic::error(RdraError::ComparisonOpNotOrdered {
            col: lhs_col_name.clone(),
            col_type: format!("{:?}", lhs_col.col_type),
            op: cmp.op.as_str().to_string(),
        }));
        return None;
    }

    // ── 右辺の解決と型整合チェック ────────────────────────────────────────────
    let rhs = match &cmp.rhs {
        Operand::Column(rhs_name) => {
            let rhs_col = match entity_cols.iter().find(|c| &c.name == rhs_name) {
                Some(c) => c,
                None => {
                    diags.push(Diagnostic::error(RdraError::ComparisonRhsColumnUnknown {
                        entity: entity_id.to_string(),
                        col: rhs_name.clone(),
                    }));
                    return None;
                }
            };
            let rhs_cat = type_category(&rhs_col.col_type);
            // 型カテゴリが一致しなければエラー
            if lhs_cat != rhs_cat {
                diags.push(Diagnostic::error(RdraError::ComparisonTypeMismatch {
                    lhs: lhs_col_name.clone(),
                    lhs_type: format!("{:?}", lhs_col.col_type),
                    rhs: rhs_name.clone(),
                    rhs_type: format!("{:?}", rhs_col.col_type),
                }));
                return None;
            }
            CmpRhs::Column(rhs_name.clone())
        }
        Operand::IntLit(s) => {
            // 左辺が数値カテゴリか確認
            if lhs_cat != "numeric" {
                diags.push(Diagnostic::error(RdraError::ComparisonTypeMismatch {
                    lhs: lhs_col_name.clone(),
                    lhs_type: format!("{:?}", lhs_col.col_type),
                    rhs: s.clone(),
                    rhs_type: "integer_literal".to_string(),
                }));
                return None;
            }
            match s.parse::<i64>() {
                Ok(n) => CmpRhs::IntLit(n),
                Err(_) => {
                    diags.push(Diagnostic::error(RdraError::ComparisonInvalidIntLit {
                        lit: s.clone(),
                    }));
                    return None;
                }
            }
        }
        Operand::Now => {
            // 左辺が時間カテゴリか確認
            if lhs_cat != "temporal" {
                diags.push(Diagnostic::error(
                    RdraError::ComparisonNowRequiresTemporal {
                        col: lhs_col_name.clone(),
                        col_type: format!("{:?}", lhs_col.col_type),
                    },
                ));
                return None;
            }
            CmpRhs::Now
        }
    };

    Some(ComparisonProp {
        lhs_column: lhs_col_name,
        op: to_model_op(&cmp.op),
        rhs,
    })
}

fn process_predicate(model: &mut SemanticModel, pred: &PredicateCall, diags: &mut Vec<Diagnostic>) {
    let sig = predicate_signature(&pred.name);
    let Some(sig) = sig else {
        // 未知述語はスキップ
        return;
    };

    // 引数を解決（_card / _col / _val はリテラル位置なのでシンボル解決しない）
    let resolved: Vec<Option<NodeRef>> = pred
        .args
        .iter()
        .enumerate()
        .map(|(i, arg)| {
            if let Some(kinds) = sig.get(i) {
                if matches!(kinds.as_slice(), ["_card"] | ["_col"] | ["_val"]) {
                    return None;
                }
            }
            resolve_arg(model, arg, diags)
        })
        .collect();

    // 型検査（_card / _col / _val はリテラル引数なのでスキップ）
    for (i, expected_kinds) in sig.iter().enumerate() {
        if matches!(expected_kinds.as_slice(), ["_card"] | ["_col"] | ["_val"]) {
            continue;
        }
        if let Some(Some(node)) = resolved.get(i) {
            let actual = node_kind_tag_str(node);
            if !expected_kinds.contains(&actual) {
                let arg_id = match &pred.args[i] {
                    PredicateArg::Ref(q) => {
                        let id = q.parts.last().cloned().unwrap_or_default();
                        match &q.kind_qualifier {
                            Some(k) => format!("{}::{}", k.name(), id),
                            None => id,
                        }
                    }
                    PredicateArg::Lit(s) => s.clone(),
                    PredicateArg::Tuple(_) => "<tuple>".to_string(),
                    PredicateArg::Expr(_) => "<expr>".to_string(),
                };
                diags.push(Diagnostic::error(RdraError::TypeMismatch {
                    pred: pred.name.clone(),
                    id: arg_id,
                    actual: actual.to_string(),
                    expected: expected_kinds.join("|"),
                }));
            }
        }
    }
    if pred.name == "contains" {
        if let (Some(Some(from)), Some(Some(to))) = (resolved.first(), resolved.get(1)) {
            let valid = matches!(
                (from, to),
                (NodeRef::Buc(_), NodeRef::UseCase(_)) | (NodeRef::System(_), NodeRef::Api(_))
            );
            if !valid {
                diags.push(Diagnostic::error(RdraError::TypeMismatch {
                    pred: pred.name.clone(),
                    id: "contains pair".to_string(),
                    actual: format!("{} -> {}", node_kind_tag_str(from), node_kind_tag_str(to)),
                    expected: "buc->usecase|system->api".to_string(),
                }));
                return;
            }
        }
    }

    // relate 以外のリレーション登録
    if pred.name == "coordinates" {
        if let (Some(Some(usecase)), Some(Some(left)), Some(Some(right))) =
            (resolved.first(), resolved.get(1), resolved.get(2))
        {
            if let (NodeRef::UseCase(uk), NodeRef::Entity(left_ek), NodeRef::Entity(right_ek)) =
                (usecase, left, right)
            {
                model
                    .boundary_coordinations
                    .push(crate::model::BoundaryCoordination {
                        usecase: *uk,
                        left: *left_ek,
                        right: *right_ek,
                    });
            }
        }
    } else if pred.name == "transitions" {
        if let (Some(Some(event)), Some(Some(state_before)), Some(Some(state_after))) =
            (resolved.first(), resolved.get(1), resolved.get(2))
        {
            model.state_transitions.push(crate::model::StateTransition {
                event: event.clone(),
                from: state_before.clone(),
                to: state_after.clone(),
            });
            model.relations.push(Relation {
                from: state_before.clone(),
                to: state_after.clone(),
                kind: RelKind::Transitions,
            });
        }
    } else if pred.name == "sets" {
        // sets(usecase/event, entity, "col_name", "value")  — 既存の等値カラム効果
        // sets(usecase/event, entity, <comparison_expr>, true/false) — 比較命題の真偽駆動
        let (Some(Some(origin)), Some(Some(entity_ref))) = (resolved.first(), resolved.get(1))
        else {
            return;
        };
        let entity_key = match entity_ref {
            NodeRef::Entity(k) => *k,
            _ => return,
        };

        match pred.args.get(2) {
            // ── 比較命題効果: 第3引数が比較式
            Some(PredicateArg::Expr(Expr::Cmp(cmp))) => {
                // 第4引数は真偽値 ("true" / "false")
                let truth_str = match pred.args.get(3) {
                    Some(PredicateArg::Ref(q))
                        if q.kind_qualifier.is_none() && q.parts.len() == 1 =>
                    {
                        q.parts[0].as_str().to_string()
                    }
                    Some(PredicateArg::Lit(s)) => s.clone(),
                    _ => {
                        // 第4引数が無いか真偽値でない → スキップ
                        return;
                    }
                };
                if truth_str != "true" && truth_str != "false" {
                    return;
                }
                let truth = truth_str == "true";
                let entity_id = model.entities[entity_key].id.clone();
                let entity_cols = model.entities[entity_key].columns.clone();
                if let Some(prop) = resolve_comparison(&entity_cols, &entity_id, cmp, diags) {
                    model.proposition_effects.push(PropositionEffect {
                        origin: origin.clone(),
                        entity: entity_key,
                        prop,
                        truth,
                    });
                }
            }

            // ── 等値カラム効果: 第3引数が文字列リテラル
            Some(PredicateArg::Lit(col_name)) => {
                let col_name = col_name.clone();
                let val_lit = match pred.args.get(3) {
                    Some(PredicateArg::Lit(s)) => s.clone(),
                    _ => return,
                };
                let col = model.entities[entity_key]
                    .columns
                    .iter()
                    .find(|c| c.name == col_name)
                    .cloned();
                let Some(col) = col else {
                    diags.push(Diagnostic::error(RdraError::UnknownColumn {
                        entity: model.entities[entity_key].id.clone(),
                        col: col_name,
                    }));
                    return;
                };
                match parse_effect_value(&col, &val_lit) {
                    Ok(value) => {
                        model.column_effects.push(ColumnEffect {
                            origin: origin.clone(),
                            entity: entity_key,
                            column: col_name,
                            value,
                        });
                    }
                    Err(e) => {
                        diags.push(Diagnostic::error(e));
                    }
                }
            }

            _ => {} // その他は無視
        }
    } else if pred.name == "forbidden" {
        // forbidden(entity, (col, val), ...) — 等値条件 AND 組合せで状態禁止
        // forbidden(entity, <expr>, ...)      — 比較命題条件 AND 組合せで状態禁止
        // 両者は混在可
        let entity_key = match resolved.first() {
            Some(Some(NodeRef::Entity(k))) => *k,
            _ => return,
        };
        let entity_id = model.entities[entity_key].id.clone();
        let entity_cols = model.entities[entity_key].columns.clone();
        let mut conditions: Vec<(String, EffectValue)> = Vec::new();
        let mut comparisons: Vec<ComparisonProp> = Vec::new();

        for arg in pred.args.iter().skip(1) {
            match arg {
                PredicateArg::Expr(Expr::Cmp(cmp)) => {
                    // 比較命題条件
                    if let Some(prop) = resolve_comparison(&entity_cols, &entity_id, cmp, diags) {
                        comparisons.push(prop);
                    }
                }
                _ => {
                    // 等値タプル条件
                    let Some((col_str, val_str)) = tuple_pair(arg) else {
                        continue; // タプル以外は無視
                    };
                    let col = entity_cols.iter().find(|c| c.name == col_str).cloned();
                    let Some(col) = col else {
                        diags.push(Diagnostic::error(RdraError::UnknownColumn {
                            entity: entity_id.clone(),
                            col: col_str,
                        }));
                        return;
                    };
                    match parse_effect_value(&col, &val_str) {
                        Ok(value) => conditions.push((col_str, value)),
                        Err(e) => {
                            diags.push(Diagnostic::error(e));
                            return;
                        }
                    }
                }
            }
        }

        // 等値条件か比較命題が少なくとも1つあれば登録
        if !conditions.is_empty() || !comparisons.is_empty() {
            model.forbidden_constraints.push(ForbiddenConstraint {
                entity: entity_key,
                conditions,
                comparisons,
            });
        }
    } else if pred.name == "invariant" {
        // invariant(entity).when(col, val).then(col, val) — チェーン等値形式
        // invariant(entity).when(<expr>).then(<expr>)      — チェーン比較式形式
        // 両者は混在可
        let entity_key = match resolved.first() {
            Some(Some(NodeRef::Entity(k))) => *k,
            _ => return,
        };
        let entity_id = model.entities[entity_key].id.clone();
        let entity_cols = model.entities[entity_key].columns.clone();
        let mut guards: Vec<(String, EffectValue)> = Vec::new();
        let mut guard_comparisons: Vec<ComparisonProp> = Vec::new();
        let mut requireds: Vec<(String, EffectValue)> = Vec::new();
        let mut required_comparisons: Vec<ComparisonProp> = Vec::new();

        for cc in &pred.chain {
            let is_guard = cc.name == "when";
            let is_required = cc.name == "then";
            if !is_guard && !is_required {
                continue; // 未知のチェーンメソッドは無視
            }

            // チェーン引数を走査: Expr → 比較命題、その他 → 等値ペア（2引数）
            let mut processed_eq = false;
            for arg in &cc.args {
                if processed_eq {
                    break; // 等値ペアは1回のみ
                }
                match arg {
                    PredicateArg::Expr(Expr::Cmp(cmp)) => {
                        if let Some(prop) = resolve_comparison(&entity_cols, &entity_id, cmp, diags)
                        {
                            if is_guard {
                                guard_comparisons.push(prop);
                            } else {
                                required_comparisons.push(prop);
                            }
                        }
                    }
                    _ => {
                        // 等値ペア: チェーン全体を (args[0], args[1]) として処理
                        if cc.args.len() < 2 {
                            break;
                        }
                        let Some(col_str) = arg_as_str(&cc.args[0]) else {
                            break;
                        };
                        let Some(val_str) = arg_as_str(&cc.args[1]) else {
                            break;
                        };
                        let col = entity_cols.iter().find(|c| c.name == col_str).cloned();
                        let Some(col) = col else {
                            diags.push(Diagnostic::error(RdraError::UnknownColumn {
                                entity: entity_id.clone(),
                                col: col_str,
                            }));
                            return;
                        };
                        match parse_effect_value(&col, &val_str) {
                            Ok(value) => {
                                if is_guard {
                                    guards.push((col_str, value));
                                } else {
                                    requireds.push((col_str, value));
                                }
                            }
                            Err(e) => {
                                diags.push(Diagnostic::error(e));
                                return;
                            }
                        }
                        processed_eq = true; // 等値ペアはこのチェーンで1回のみ
                    }
                }
            }
        }

        // ガードと必要条件の両方が（等値か比較命題で）揃っている場合のみ登録
        let has_guards = !guards.is_empty() || !guard_comparisons.is_empty();
        let has_requireds = !requireds.is_empty() || !required_comparisons.is_empty();
        if has_guards && has_requireds {
            model.entity_invariants.push(EntityInvariant {
                entity: entity_key,
                guards,
                guard_comparisons,
                requireds,
                required_comparisons,
            });
        }
    } else if pred.name != "relate" {
        if let (Some(Some(from)), Some(Some(to))) = (resolved.first(), resolved.get(1)) {
            let kind = match pred.name.as_str() {
                "performs" => RelKind::Performs,
                "uses" => RelKind::Uses,
                "reads" => RelKind::Reads,
                "writes" => RelKind::Writes,
                "creates" => RelKind::Creates,
                "updates" => RelKind::Updates,
                "deletes" => RelKind::Deletes,
                "displays" => RelKind::Displays,
                "shows" => RelKind::Shows,
                "raises" => RelKind::Raises,
                "triggers" => RelKind::Triggers,
                "contains" => RelKind::Contains,
                "belongs" => RelKind::Belongs,
                "has_permission" => RelKind::HasPermission,
                "requires_permission" => RelKind::RequiresPermission,
                "requires_medium" => RelKind::RequiresMedium,
                "motivates" => RelKind::Motivates,
                "invokes" => RelKind::Invokes,
                _ => return,
            };
            model.relations.push(Relation {
                from: from.clone(),
                to: to.clone(),
                kind,
            });

            if pred.name == "belongs" {
                let (NodeRef::Buc(buc), NodeRef::Business(business)) = (from, to) else {
                    return;
                };
                let mut whens = Vec::new();
                let mut wheres = Vec::new();
                let mut bys = Vec::new();

                for cc in &pred.chain {
                    let (target, expected_kind) = match cc.name.as_str() {
                        "when" => (&mut whens, "timing"),
                        "where" => (&mut wheres, "location"),
                        "by" => (&mut bys, "medium"),
                        _ => continue,
                    };
                    for arg in &cc.args {
                        if let Some(value) =
                            context_value_from_arg(model, arg, expected_kind, diags)
                        {
                            target.push(value);
                        }
                    }
                }

                if !whens.is_empty() || !wheres.is_empty() || !bys.is_empty() {
                    model
                        .business_mapping_contexts
                        .push(BusinessMappingContext {
                            buc: *buc,
                            business: *business,
                            whens,
                            wheres,
                            bys,
                        });
                }
            }
        }
    } else {
        // relate(From, To, Card)
        if let (Some(Some(from)), Some(Some(to)), Some(PredicateArg::Lit(card))) =
            (resolved.first(), resolved.get(1), pred.args.get(2))
        {
            let kind = match card.as_str() {
                "1:1" => RelKind::RelateOneToOne,
                "1:N" => RelKind::RelateOneToMany,
                "N:1" => RelKind::RelateManyToOne,
                "N:M" => {
                    let from_id = match from {
                        NodeRef::Entity(k) => model.entities[*k].id.clone(),
                        _ => "?".into(),
                    };
                    let to_id = match to {
                        NodeRef::Entity(k) => model.entities[*k].id.clone(),
                        _ => "?".into(),
                    };
                    diags.push(Diagnostic::warning(RdraError::NMRelation {
                        from: from_id,
                        to: to_id,
                    }));
                    RelKind::RelateManyToMany
                }
                _ => return,
            };
            model.relations.push(Relation {
                from: from.clone(),
                to: to.clone(),
                kind,
            });
        }
    }
}

fn node_kind_tag_str(node: &NodeRef) -> &'static str {
    match node {
        NodeRef::Actor(_) => "actor",
        NodeRef::ExtSystem(_) => "extsystem",
        NodeRef::System(_) => "system",
        NodeRef::Requirement(_) => "requirement",
        NodeRef::Business(_) => "business",
        NodeRef::Buc(_) => "buc",
        NodeRef::UsageScene(_) => "usagescene",
        NodeRef::UseCase(_) => "usecase",
        NodeRef::Screen(_) => "screen",
        NodeRef::Event(_) => "event",
        NodeRef::Entity(_) => "entity",
        NodeRef::State(_) => "state",
        NodeRef::Condition(_) => "condition",
        NodeRef::Variation(_) => "variation",
        NodeRef::Api(_) => "api",
        NodeRef::Location(_) => "location",
        NodeRef::Timing(_) => "timing",
        NodeRef::Medium(_) => "medium",
        NodeRef::Permission(_) => "permission",
    }
}

fn generate_fks(model: &mut SemanticModel, diags: &mut Vec<Diagnostic>) {
    let rels: Vec<_> = model
        .relations
        .iter()
        .filter(|r| matches!(r.kind, RelKind::RelateManyToOne | RelKind::RelateOneToMany))
        .map(|r| (r.from.clone(), r.to.clone(), r.kind.clone()))
        .collect();

    for (from, to, kind) in rels {
        let (many_key, one_key) = match kind {
            RelKind::RelateManyToOne => {
                if let (NodeRef::Entity(fk), NodeRef::Entity(tk)) = (&from, &to) {
                    (*fk, *tk)
                } else {
                    continue;
                }
            }
            RelKind::RelateOneToMany => {
                if let (NodeRef::Entity(ok), NodeRef::Entity(mk)) = (&from, &to) {
                    (*mk, *ok)
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        let (one_id, pk_type) = {
            let one = &model.entities[one_key];
            let pk = one.columns.iter().find(|c| c.is_pk);
            match pk {
                Some(col) => (one.id.clone(), col.col_type.clone()),
                None => {
                    diags.push(Diagnostic::error(RdraError::MissingPk {
                        entity: one.id.clone(),
                    }));
                    continue;
                }
            }
        };

        let fk_col_name = format!("{}_id", one_id.to_lowercase());

        let many_entity_id = model.entities[many_key].id.clone();
        if model.entities[many_key]
            .columns
            .iter()
            .any(|c| c.name == fk_col_name)
        {
            diags.push(Diagnostic::error(RdraError::FkConflict {
                entity: many_entity_id,
                col: fk_col_name.clone(),
            }));
            continue;
        }

        let fk_col = ModelColumn {
            name: fk_col_name,
            col_type: pk_type,
            is_pk: false,
            is_unique: false,
            is_nullable: false,
            default_val: None,
            label: None,
            is_fk: true,
            fk_target: Some(one_id),
        };
        model.entities[many_key].columns.push(fk_col);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_syntax::parse;

    #[test]
    fn test_build_model_basic() {
        let src = r#"
actor Customer "顧客" description "商品を購入する顧客"
entity Order "注文" description "受注情報" { id: Int @pk }
entity Customer_profile "顧客情報" { id: Int @pk  name: String }
usecase Browse "商品を探す" description "商品一覧を参照する"
performs(Customer, Browse)
relate(Order, Customer_profile, "N:1")
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);

        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors.is_empty(),
            "unexpected errors: {:?}",
            errors
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );

        assert_eq!(model.actors.len(), 1);
        let actor = model.actors.values().next().unwrap();
        assert_eq!(actor.id, "Customer");
        assert_eq!(actor.label, "顧客");
        assert_eq!(actor.description.as_deref(), Some("商品を購入する顧客"));
        let use_case = model.use_cases.values().next().unwrap();
        assert_eq!(use_case.description.as_deref(), Some("商品一覧を参照する"));

        let order = model
            .entities
            .values()
            .find(|e| e.id == "Order")
            .expect("Order entity not found");
        assert_eq!(order.description.as_deref(), Some("受注情報"));

        let fk_col = order
            .columns
            .iter()
            .find(|c| c.name == "customer_profile_id")
            .expect("customer_profile_id FK column not found");

        assert!(fk_col.is_fk);
        assert_eq!(fk_col.fk_target.as_deref(), Some("Customer_profile"));
        assert_eq!(fk_col.col_type, ColumnType::Int);
    }

    #[test]
    fn test_duplicate_definition_same_kind() {
        let src = r#"
actor Customer "顧客"
actor Customer "重複"
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty());
        assert!(errors[0].error.to_string().contains("duplicate definition"));
    }

    #[test]
    fn test_same_name_different_kind_allowed() {
        // `actor Add` and `usecase Add` must coexist without error when
        // references are qualified.
        let src = r#"
actor   Add "追加アクター"
usecase Add "追加UC"
performs(actor::Add, usecase::Add)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors.is_empty(),
            "unexpected errors: {:?}",
            errors
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );

        assert_eq!(model.actors.len(), 1);
        assert_eq!(model.use_cases.len(), 1);
        assert_eq!(model.relations.len(), 1);
    }

    #[test]
    fn test_ambiguous_unqualified_reference() {
        let src = r#"
actor   Add "追加アクター"
usecase Add "追加UC"
performs(Add, Add)
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty());
        assert!(errors[0].error.to_string().contains("ambiguous reference"));
    }

    #[test]
    fn test_type_mismatch() {
        let src = r#"
actor Customer "顧客"
usecase Browse "商品を探す"
performs(Browse, Customer)
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty());
        assert!(errors[0].error.to_string().contains("type mismatch"));
    }

    #[test]
    fn test_nm_relation_warning() {
        let src = r#"
entity A "A" { id: Int @pk }
entity B "B" { id: Int @pk }
relate(A, B, "N:M")
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let warnings: Vec<_> = diags.iter().filter(|d| d.is_warning).collect();
        assert!(!warnings.is_empty());
        assert!(warnings[0].error.to_string().contains("N:M relation"));
    }

    #[test]
    fn test_missing_pk_error() {
        let src = r#"
entity A "A" { name: String }
entity B "B" { id: Int @pk }
relate(B, A, "N:1")
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty());
        assert!(errors[0].error.to_string().contains("missing @pk"));
    }

    #[test]
    fn test_one_to_many_fk_on_to_side() {
        let src = r#"
entity Customer "顧客" { id: Int @pk }
entity Order "注文" { id: Int @pk }
relate(Customer, Order, "1:N")
"#;
        let (ast, _) = parse(src);
        let (model, diags) = build_model(&ast);

        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors.is_empty(),
            "unexpected errors: {:?}",
            errors
                .iter()
                .map(|d| d.error.to_string())
                .collect::<Vec<_>>()
        );

        let order = model.entities.values().find(|e| e.id == "Order").unwrap();
        let fk = order.columns.iter().find(|c| c.name == "customer_id");
        assert!(fk.is_some(), "customer_id FK not found in Order");
        assert!(fk.unwrap().is_fk);
    }

    #[test]
    fn test_api_declaration_and_invokes() {
        let src = r#"
usecase PlaceOrder "注文する"
api OrderApi "注文API" description "注文を永続化するAPI"
invokes(PlaceOrder, OrderApi)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.apis.len(), 1);
        let api = model.apis.values().next().unwrap();
        assert_eq!(api.id, "OrderApi");
        assert_eq!(api.label, "注文API");
        assert_eq!(api.description.as_deref(), Some("注文を永続化するAPI"));

        let invokes_rel = model.relations.iter().find(|r| r.kind == RelKind::Invokes);
        assert!(invokes_rel.is_some(), "Invokes relation should exist");
    }

    #[test]
    fn test_belongs_when_where_context() {
        let src = r#"
business ClinicOps "Clinic Operations"
buc BucAppointmentScheduling "Appointment Scheduling"
location FrontDesk "Front Desk"
timing AppointmentRequested "Appointment Requested"
medium FrontDeskTerminal "Front Desk Terminal"
belongs(BucAppointmentScheduling, ClinicOps)
  .when("patient requests a booking")
  .when(AppointmentRequested)
  .where(FrontDesk)
  .where("patient portal")
  .by(FrontDeskTerminal)
  .by("tablet")
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        let rel = model.relations.iter().find(|r| r.kind == RelKind::Belongs);
        assert!(rel.is_some(), "Belongs relation should still exist");

        assert_eq!(model.business_mapping_contexts.len(), 1);
        let ctx = &model.business_mapping_contexts[0];
        assert_eq!(model.bucs[ctx.buc].id, "BucAppointmentScheduling");
        assert_eq!(model.businesses[ctx.business].id, "ClinicOps");
        assert_eq!(ctx.whens.len(), 2);
        assert_eq!(ctx.wheres.len(), 2);
        assert_eq!(ctx.bys.len(), 2);

        assert!(matches!(
            &ctx.whens[0],
            BusinessMappingContextValue::Text(s) if s == "patient requests a booking"
        ));
        assert!(matches!(
            &ctx.whens[1],
            BusinessMappingContextValue::Ref(NodeRef::Timing(_))
        ));
        assert!(matches!(
            &ctx.wheres[0],
            BusinessMappingContextValue::Ref(NodeRef::Location(_))
        ));
        assert!(matches!(
            &ctx.wheres[1],
            BusinessMappingContextValue::Text(s) if s == "patient portal"
        ));
        assert!(matches!(
            &ctx.bys[0],
            BusinessMappingContextValue::Ref(NodeRef::Medium(_))
        ));
        assert!(matches!(
            &ctx.bys[1],
            BusinessMappingContextValue::Text(s) if s == "tablet"
        ));
    }

    #[test]
    fn test_actor_permission_attachment() {
        let src = r#"
actor Staff "Staff"
permission ManageSchedule "Manage Schedule"
has_permission(Staff, ManageSchedule)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.permissions.len(), 1);
        let permission = model.permissions.values().next().unwrap();
        assert_eq!(permission.id, "ManageSchedule");
        assert_eq!(permission.label, "Manage Schedule");

        let rel = model
            .relations
            .iter()
            .find(|r| r.kind == RelKind::HasPermission)
            .expect("HasPermission relation should exist");
        assert!(matches!(rel.from, NodeRef::Actor(_)));
        assert!(matches!(rel.to, NodeRef::Permission(_)));
    }

    #[test]
    fn test_screen_constraint_patterns_derive_from_usecase_and_api() {
        let src = r#"
usecase BookAppointment "Book Appointment"
screen BookingScreen "Booking Screen"
api BookingApi "Booking API"
permission ScheduleWrite "Schedule Write"
permission PatientRead "Patient Read"
medium StaffTerminal "Staff Terminal"
medium SecureChannel "Secure Channel"
displays(BookAppointment, BookingScreen)
invokes(BookAppointment, BookingApi)
requires_permission(BookAppointment, ScheduleWrite)
requires_medium(BookAppointment, StaffTerminal)
requires_permission(BookingApi, PatientRead)
requires_medium(BookingApi, SecureChannel)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        let patterns = crate::derive_screen_constraint_patterns(&model);
        assert_eq!(patterns.len(), 1);

        let pattern = &patterns[0];
        assert_eq!(model.screens[pattern.screen].id, "BookingScreen");
        assert_eq!(model.use_cases[pattern.usecase].id, "BookAppointment");
        assert_eq!(
            model.apis[pattern.api.expect("api should be part of the path")].id,
            "BookingApi"
        );

        let permission_ids: Vec<_> = pattern
            .permissions
            .iter()
            .map(|key| model.permissions[*key].id.as_str())
            .collect();
        assert_eq!(permission_ids, vec!["ScheduleWrite", "PatientRead"]);

        let medium_ids: Vec<_> = pattern
            .media
            .iter()
            .map(|key| model.media[*key].id.as_str())
            .collect();
        assert_eq!(medium_ids, vec!["StaffTerminal", "SecureChannel"]);
    }

    #[test]
    fn test_api_crud_type_check_ok() {
        let src = r#"
api OrderApi "注文API"
entity Order "注文" { id: Int @pk }
creates(OrderApi, Order)
"#;
        let (ast, _) = parse(src);
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        let creates_rel = model.relations.iter().find(|r| r.kind == RelKind::Creates);
        assert!(creates_rel.is_some());
    }

    #[test]
    fn test_invokes_type_mismatch() {
        // invokes(uc, entity) は TypeMismatch になるはず
        let src = r#"
usecase PlaceOrder "注文する"
entity Order "注文" { id: Int @pk }
invokes(PlaceOrder, Order)
"#;
        let (ast, _) = parse(src);
        let (_, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(!errors.is_empty(), "type mismatch expected");
        assert!(errors[0].error.to_string().contains("type mismatch"));
    }

    #[test]
    fn test_usecase_crud_still_allowed() {
        // 後方互換: usecase が直接 entity を creates しても OK
        let src = r#"
usecase PlaceOrder "注文する"
entity Order "注文" { id: Int @pk }
creates(PlaceOrder, Order)
"#;
        let (ast, _) = parse(src);
        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(
            errors.is_empty(),
            "legacy creates(uc, entity) should still work"
        );
        assert_eq!(
            model
                .relations
                .iter()
                .filter(|r| r.kind == RelKind::Creates)
                .count(),
            1
        );
    }

    #[test]
    fn test_sets_comparison_registers_proposition_effect() {
        let src = r#"
usecase Sell "販売する"
entity Stock "在庫" {
  id: Int @pk
  stock: Int
  selling: Int
}
updates(Sell, Stock)
sets(Sell, Stock, stock < selling, true)
"#;
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

        let (model, diags) = build_model(&ast);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

        assert_eq!(model.proposition_effects.len(), 1);
        let effect = &model.proposition_effects[0];
        assert_eq!(effect.prop.axis_key(), "stock<selling");
        assert!(effect.truth);
        assert!(matches!(effect.origin, NodeRef::UseCase(_)));
    }
}
