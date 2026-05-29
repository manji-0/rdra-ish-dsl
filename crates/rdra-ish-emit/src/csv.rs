//! CSV emitters: ActorList, EntityList, RelationMatrix.

use crate::{EmitError, Emitter, View};
use rdra_ish_core::model::{ColumnType, NodeRef, RelKind, SemanticModel, UseCaseKey};

// ── ActorListCsvEmitter ────────────────────────────────────────────────────────

pub struct ActorListCsvEmitter;

impl Emitter for ActorListCsvEmitter {
    fn emit(&self, model: &SemanticModel, _view: &View) -> Result<String, EmitError> {
        let mut wtr = csv::Writer::from_writer(vec![]);
        wtr.write_record(["id", "label"])?;

        let mut actors: Vec<_> = model.actors.iter().collect();
        actors.sort_by_key(|(_, a)| &a.id);

        for (_, actor) in &actors {
            wtr.write_record([&actor.id, &actor.label])?;
        }

        let data = wtr
            .into_inner()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        Ok(String::from_utf8(data).unwrap_or_default())
    }
}

// ── EntityListCsvEmitter ──────────────────────────────────────────────────────

pub struct EntityListCsvEmitter;

impl Emitter for EntityListCsvEmitter {
    fn emit(&self, model: &SemanticModel, _view: &View) -> Result<String, EmitError> {
        let mut wtr = csv::Writer::from_writer(vec![]);
        wtr.write_record([
            "entity_id",
            "entity_label",
            "column_name",
            "column_type",
            "is_pk",
            "is_fk",
            "fk_target",
            "is_nullable",
            "default_val",
        ])?;

        let mut entities: Vec<_> = model.entities.iter().collect();
        entities.sort_by_key(|(_, e)| &e.id);

        for (_, ent) in &entities {
            for col in &ent.columns {
                let col_type = col_type_to_str(&col.col_type);
                let fk_target = col.fk_target.as_deref().unwrap_or("").to_string();
                let default_val = col.default_val.as_deref().unwrap_or("").to_string();
                let is_pk = if col.is_pk { "true" } else { "false" };
                let is_fk = if col.is_fk { "true" } else { "false" };
                let is_nullable = if col.is_nullable { "true" } else { "false" };
                wtr.write_record([
                    ent.id.as_str(),
                    ent.label.as_str(),
                    col.name.as_str(),
                    col_type,
                    is_pk,
                    is_fk,
                    fk_target.as_str(),
                    is_nullable,
                    default_val.as_str(),
                ])?;
            }
        }

        let data = wtr
            .into_inner()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        Ok(String::from_utf8(data).unwrap_or_default())
    }
}

// ── RelationMatrixCsvEmitter ──────────────────────────────────────────────────

/// 行=UseCase, 列=Entity, セル値=CRUD文字
pub struct RelationMatrixCsvEmitter;

impl Emitter for RelationMatrixCsvEmitter {
    fn emit(&self, model: &SemanticModel, _view: &View) -> Result<String, EmitError> {
        // ソートされたUseCase一覧
        let mut use_cases: Vec<_> = model.use_cases.iter().collect();
        use_cases.sort_by_key(|(_, u)| &u.id);

        // ソートされたEntity一覧
        let mut entities: Vec<_> = model.entities.iter().collect();
        entities.sort_by_key(|(_, e)| &e.id);

        // UseCase → Entity → CRUD bits
        // bits: C=1, R=2, U=4, D=8, W=16
        use std::collections::HashMap;
        let mut matrix: HashMap<UseCaseKey, HashMap<rdra_ish_core::model::EntityKey, u8>> =
            HashMap::new();

        for rel in &model.relations {
            let (uc_key, ent_key, bit) = match &rel.kind {
                RelKind::Reads => {
                    if let (NodeRef::UseCase(uk), NodeRef::Entity(ek)) = (&rel.from, &rel.to) {
                        (*uk, *ek, 0b00010u8) // R
                    } else {
                        continue;
                    }
                }
                RelKind::Writes => {
                    if let (NodeRef::UseCase(uk), NodeRef::Entity(ek)) = (&rel.from, &rel.to) {
                        (*uk, *ek, 0b10000u8) // W
                    } else {
                        continue;
                    }
                }
                RelKind::Creates => {
                    if let (NodeRef::UseCase(uk), NodeRef::Entity(ek)) = (&rel.from, &rel.to) {
                        (*uk, *ek, 0b00001u8) // C
                    } else {
                        continue;
                    }
                }
                RelKind::Updates => {
                    if let (NodeRef::UseCase(uk), NodeRef::Entity(ek)) = (&rel.from, &rel.to) {
                        (*uk, *ek, 0b00100u8) // U
                    } else {
                        continue;
                    }
                }
                RelKind::Deletes => {
                    if let (NodeRef::UseCase(uk), NodeRef::Entity(ek)) = (&rel.from, &rel.to) {
                        (*uk, *ek, 0b01000u8) // D
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            *matrix
                .entry(uc_key)
                .or_default()
                .entry(ent_key)
                .or_insert(0) |= bit;
        }

        let mut wtr = csv::Writer::from_writer(vec![]);

        // header: UseCase, entity1, entity2, ...
        let mut header = vec!["UseCase".to_string()];
        for (_, ent) in &entities {
            header.push(ent.id.clone());
        }
        wtr.write_record(&header)?;

        // rows
        for (uc_key, uc) in &use_cases {
            let mut row = vec![uc.id.clone()];
            for (ent_key, _) in &entities {
                let bits = matrix
                    .get(uc_key)
                    .and_then(|m| m.get(ent_key))
                    .copied()
                    .unwrap_or(0);
                row.push(bits_to_crud(bits));
            }
            wtr.write_record(&row)?;
        }

        let data = wtr
            .into_inner()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        Ok(String::from_utf8(data).unwrap_or_default())
    }
}

// ── ヘルパー ──────────────────────────────────────────────────────────────────

fn col_type_to_str(ct: &ColumnType) -> &'static str {
    match ct {
        ColumnType::Int => "Int",
        ColumnType::String => "String",
        ColumnType::Money => "Money",
        ColumnType::DateTime => "DateTime",
        ColumnType::Date => "Date",
        ColumnType::Bool => "Bool",
        ColumnType::Decimal => "Decimal",
        ColumnType::Enum(_) => "Enum",
    }
}

fn bits_to_crud(bits: u8) -> String {
    let mut s = String::new();
    if bits & 0b00001 != 0 {
        s.push('C');
    }
    if bits & 0b00010 != 0 {
        s.push('R');
    }
    if bits & 0b00100 != 0 {
        s.push('U');
    }
    if bits & 0b01000 != 0 {
        s.push('D');
    }
    if bits & 0b10000 != 0 {
        s.push('W');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);
        model
    }

    #[test]
    fn test_actor_list_csv() {
        let src = r#"
actor Customer "顧客"
actor Staff "スタッフ"
"#;
        let model = model_from(src);
        let view = View::whole();
        let result = ActorListCsvEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("id,label"));
        assert!(result.contains("Customer,顧客"));
        assert!(result.contains("Staff,スタッフ"));
    }

    #[test]
    fn test_entity_list_csv() {
        let src = r#"
entity Order "注文" { id: Int @pk  total: Money }
entity Customer "顧客" { id: Int @pk  name: String }
relate(Order, Customer, "N:1")
"#;
        let model = model_from(src);
        let view = View::whole();
        let result = EntityListCsvEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("entity_id"));
        assert!(result.contains("customer_id"));
        assert!(result.contains("true")); // is_fk
    }

    #[test]
    fn test_relation_matrix_csv() {
        let src = r#"
usecase Browse "商品を探す"
usecase Order "注文する"
entity Product "商品" { id: Int @pk }
entity OrderEnt "注文エンティティ" { id: Int @pk }
reads(Browse, Product)
creates(Order, OrderEnt)
"#;
        let model = model_from(src);
        let view = View::whole();
        let result = RelationMatrixCsvEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("UseCase"));
        assert!(result.contains("Browse"));
        assert!(result.contains("Order"));
        assert!(result.contains('R'));
        assert!(result.contains('C'));
    }

    #[test]
    fn test_entity_csv_snapshot() {
        let src = r#"
entity Customer "顧客" { id: Int @pk  name: String  email: String @null }
entity Order "注文" { id: Int @pk  total: Money }
relate(Order, Customer, "N:1")
"#;
        let (ast, _) = parse(src);
        let (model, _) = build_model(&ast);
        let result = EntityListCsvEmitter.emit(&model, &View::whole()).unwrap();
        insta::assert_snapshot!(result);
    }
}
