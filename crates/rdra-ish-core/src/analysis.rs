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
            Some(vec![vec!["usecase"], vec!["entity"]])
        }
        "displays" => Some(vec![vec!["usecase"], vec!["screen"]]),
        "shows" => Some(vec![vec!["screen"], vec!["entity"]]),
        "raises" => Some(vec![vec!["usecase"], vec!["event"]]),
        "triggers" => Some(vec![vec!["event"], vec!["usecase"]]),
        "contains" => Some(vec![vec!["buc"], vec!["usecase"]]),
        "belongs" => Some(vec![vec!["buc"], vec!["business"]]),
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
            });
            NodeRef::Actor(k)
        }
        Kind::ExtSystem => {
            let k = model.ext_systems.insert(ExtSystem {
                id: inst.id.clone(),
                label: inst.label.clone(),
            });
            NodeRef::ExtSystem(k)
        }
        Kind::Requirement => {
            let k = model.requirements.insert(Requirement {
                id: inst.id.clone(),
                label: inst.label.clone(),
            });
            NodeRef::Requirement(k)
        }
        Kind::Business => {
            let k = model.businesses.insert(Business {
                id: inst.id.clone(),
                label: inst.label.clone(),
            });
            NodeRef::Business(k)
        }
        Kind::Buc => {
            let k = model.bucs.insert(Buc {
                id: inst.id.clone(),
                label: inst.label.clone(),
            });
            NodeRef::Buc(k)
        }
        Kind::UsageScene => {
            let k = model.usage_scenes.insert(UsageScene {
                id: inst.id.clone(),
                label: inst.label.clone(),
            });
            NodeRef::UsageScene(k)
        }
        Kind::UseCase => {
            let k = model.use_cases.insert(UseCase {
                id: inst.id.clone(),
                label: inst.label.clone(),
            });
            NodeRef::UseCase(k)
        }
        Kind::Screen => {
            let k = model.screens.insert(Screen {
                id: inst.id.clone(),
                label: inst.label.clone(),
            });
            NodeRef::Screen(k)
        }
        Kind::Event => {
            let k = model.events.insert(Event {
                id: inst.id.clone(),
                label: inst.label.clone(),
            });
            NodeRef::Event(k)
        }
        Kind::Entity => {
            let columns = inst.columns.iter().map(ast_column_to_model).collect();
            let k = model.entities.insert(Entity {
                id: inst.id.clone(),
                label: inst.label.clone(),
                columns,
            });
            NodeRef::Entity(k)
        }
        Kind::State => {
            let k = model.states.insert(State {
                id: inst.id.clone(),
                label: inst.label.clone(),
            });
            NodeRef::State(k)
        }
        Kind::Condition => {
            let k = model.conditions.insert(Condition {
                id: inst.id.clone(),
                label: inst.label.clone(),
            });
            NodeRef::Condition(k)
        }
        Kind::Variation => {
            let k = model.variations.insert(Variation {
                id: inst.id.clone(),
                label: inst.label.clone(),
            });
            NodeRef::Variation(k)
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

fn process_predicate(model: &mut SemanticModel, pred: &PredicateCall, diags: &mut Vec<Diagnostic>) {
    let sig = predicate_signature(&pred.name);
    let Some(sig) = sig else {
        // 未知述語はスキップ
        return;
    };

    // 引数を解決
    let resolved: Vec<Option<NodeRef>> = pred
        .args
        .iter()
        .map(|arg| resolve_arg(model, arg, diags))
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

    // relate 以外のリレーション登録
    if pred.name == "transitions" {
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
        // sets(usecase/event, entity, "col_name", "value")
        if let (
            Some(Some(origin)),
            Some(Some(entity_ref)),
            Some(PredicateArg::Lit(col_name)),
            Some(PredicateArg::Lit(val_lit)),
        ) = (
            resolved.first(),
            resolved.get(1),
            pred.args.get(2),
            pred.args.get(3),
        ) {
            let entity_key = match entity_ref {
                NodeRef::Entity(k) => *k,
                _ => return, // 型検査で既にエラーが出ているはず
            };

            // カラムを entity の columns リストから名前で解決（SymbolTable には無い）
            let col = model.entities[entity_key]
                .columns
                .iter()
                .find(|c| &c.name == col_name)
                .cloned();

            let Some(col) = col else {
                diags.push(Diagnostic::error(RdraError::UnknownColumn {
                    entity: model.entities[entity_key].id.clone(),
                    col: col_name.clone(),
                }));
                return;
            };

            match parse_effect_value(&col, val_lit) {
                Ok(value) => {
                    model.column_effects.push(ColumnEffect {
                        origin: origin.clone(),
                        entity: entity_key,
                        column: col_name.clone(),
                        value,
                    });
                }
                Err(e) => {
                    diags.push(Diagnostic::error(e));
                }
            }
        }
    } else if pred.name == "forbidden" {
        // forbidden(entity, "col_name", "value")
        // forbidden(entity, (col, val), ...) — 可変長タプルで条件AND組合せを禁止
        let entity_key = match resolved.first() {
            Some(Some(NodeRef::Entity(k))) => *k,
            _ => return,
        };
        let entity_id = model.entities[entity_key].id.clone();
        let mut conditions: Vec<(String, EffectValue)> = Vec::new();

        for arg in pred.args.iter().skip(1) {
            let Some((col_str, val_str)) = tuple_pair(arg) else {
                // タプル以外の引数は無視（将来的に診断可能）
                continue;
            };
            let col = model.entities[entity_key]
                .columns
                .iter()
                .find(|c| c.name == col_str)
                .cloned();
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

        if !conditions.is_empty() {
            model.forbidden_constraints.push(ForbiddenConstraint {
                entity: entity_key,
                conditions,
            });
        }
    } else if pred.name == "invariant" {
        // invariant(entity).when(col, val).then(col, val) — チェーン形式
        let entity_key = match resolved.first() {
            Some(Some(NodeRef::Entity(k))) => *k,
            _ => return,
        };
        let entity_id = model.entities[entity_key].id.clone();
        let mut guards: Vec<(String, EffectValue)> = Vec::new();
        let mut requireds: Vec<(String, EffectValue)> = Vec::new();

        for cc in &pred.chain {
            let is_guard = cc.name == "when";
            let is_required = cc.name == "then";
            if !is_guard && !is_required {
                continue; // 未知のチェーンメソッドは無視
            }
            if cc.args.len() != 2 {
                continue; // 引数数が不正なら無視
            }
            let Some(col_str) = arg_as_str(&cc.args[0]) else {
                continue;
            };
            let Some(val_str) = arg_as_str(&cc.args[1]) else {
                continue;
            };
            let col = model.entities[entity_key]
                .columns
                .iter()
                .find(|c| c.name == col_str)
                .cloned();
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
        }

        // guards と requireds の両方が揃っている場合のみ登録
        if !guards.is_empty() && !requireds.is_empty() {
            model.entity_invariants.push(EntityInvariant {
                entity: entity_key,
                guards,
                requireds,
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
                "motivates" => RelKind::Motivates,
                _ => return,
            };
            model.relations.push(Relation {
                from: from.clone(),
                to: to.clone(),
                kind,
            });
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
actor Customer "顧客"
entity Order "注文" { id: Int @pk }
entity Customer_profile "顧客情報" { id: Int @pk  name: String }
usecase Browse "商品を探す"
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

        let order = model
            .entities
            .values()
            .find(|e| e.id == "Order")
            .expect("Order entity not found");

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
}
