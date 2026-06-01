//! CSV emitters: ActorList, EntityList, RelationMatrix, ApiList, ApiEntityMatrix.

use crate::{EmitError, Emitter, View};
use rdra_ish_core::{
    derive_actor_input_inferences, derive_actor_permission_audit, derive_permission_callables,
    derive_screen_constraint_patterns,
    model::{ApiKey, ColumnType, EntityKey, NodeRef, RelKind, SemanticModel, UseCaseKey},
    ActorInputSource,
};

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
            .map_err(|e| std::io::Error::other(e.to_string()))?;
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
            .map_err(|e| std::io::Error::other(e.to_string()))?;
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
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(String::from_utf8(data).unwrap_or_default())
    }
}

// ── ApiListCsvEmitter ─────────────────────────────────────────────────────────

/// 行=api, 列=id,label
pub struct ApiListCsvEmitter;

impl Emitter for ApiListCsvEmitter {
    fn emit(&self, model: &SemanticModel, _view: &View) -> Result<String, EmitError> {
        let mut wtr = csv::Writer::from_writer(vec![]);
        wtr.write_record(["id", "label"])?;

        let mut apis: Vec<_> = model.apis.iter().collect();
        apis.sort_by_key(|(_, a)| &a.id);

        for (_, api) in &apis {
            wtr.write_record([&api.id, &api.label])?;
        }

        let data = wtr
            .into_inner()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(String::from_utf8(data).unwrap_or_default())
    }
}

// ── ApiEntityMatrixCsvEmitter ─────────────────────────────────────────────────

/// 行=Api, 列=Entity, セル値=CRUD文字
pub struct ApiEntityMatrixCsvEmitter;

impl Emitter for ApiEntityMatrixCsvEmitter {
    fn emit(&self, model: &SemanticModel, _view: &View) -> Result<String, EmitError> {
        use std::collections::HashMap;

        let mut apis: Vec<_> = model.apis.iter().collect();
        apis.sort_by_key(|(_, a)| &a.id);

        let mut entities: Vec<_> = model.entities.iter().collect();
        entities.sort_by_key(|(_, e)| &e.id);

        let mut matrix: HashMap<ApiKey, HashMap<EntityKey, u8>> = HashMap::new();

        for rel in &model.relations {
            let (ak, ek, bit) = match &rel.kind {
                RelKind::Reads => {
                    if let (NodeRef::Api(ak), NodeRef::Entity(ek)) = (&rel.from, &rel.to) {
                        (*ak, *ek, 0b00010u8)
                    } else {
                        continue;
                    }
                }
                RelKind::Writes => {
                    if let (NodeRef::Api(ak), NodeRef::Entity(ek)) = (&rel.from, &rel.to) {
                        (*ak, *ek, 0b10000u8)
                    } else {
                        continue;
                    }
                }
                RelKind::Creates => {
                    if let (NodeRef::Api(ak), NodeRef::Entity(ek)) = (&rel.from, &rel.to) {
                        (*ak, *ek, 0b00001u8)
                    } else {
                        continue;
                    }
                }
                RelKind::Updates => {
                    if let (NodeRef::Api(ak), NodeRef::Entity(ek)) = (&rel.from, &rel.to) {
                        (*ak, *ek, 0b00100u8)
                    } else {
                        continue;
                    }
                }
                RelKind::Deletes => {
                    if let (NodeRef::Api(ak), NodeRef::Entity(ek)) = (&rel.from, &rel.to) {
                        (*ak, *ek, 0b01000u8)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };

            *matrix.entry(ak).or_default().entry(ek).or_insert(0) |= bit;
        }

        let mut wtr = csv::Writer::from_writer(vec![]);

        let mut header = vec!["Api".to_string()];
        for (_, ent) in &entities {
            header.push(ent.id.clone());
        }
        wtr.write_record(&header)?;

        for (api_key, api) in &apis {
            let mut row = vec![api.id.clone()];
            for (ent_key, _) in &entities {
                let bits = matrix
                    .get(api_key)
                    .and_then(|m| m.get(ent_key))
                    .copied()
                    .unwrap_or(0);
                row.push(bits_to_crud(bits));
            }
            wtr.write_record(&row)?;
        }

        let data = wtr
            .into_inner()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(String::from_utf8(data).unwrap_or_default())
    }
}

// ── ScreenConstraintCsvEmitter ───────────────────────────────────────────────

pub struct ScreenConstraintCsvEmitter;

impl Emitter for ScreenConstraintCsvEmitter {
    fn emit(&self, model: &SemanticModel, _view: &View) -> Result<String, EmitError> {
        let mut wtr = csv::Writer::from_writer(vec![]);
        wtr.write_record([
            "screen_id",
            "usecase_id",
            "api_id",
            "required_permissions",
            "required_media",
        ])?;

        for pattern in derive_screen_constraint_patterns(model) {
            let screen_id = model.screens[pattern.screen].id.as_str();
            let usecase_id = model.use_cases[pattern.usecase].id.as_str();
            let api_id = pattern
                .api
                .map(|key| model.apis[key].id.as_str())
                .unwrap_or("");
            let permissions = pattern
                .permissions
                .iter()
                .map(|key| model.permissions[*key].id.as_str())
                .collect::<Vec<_>>()
                .join("|");
            let media = pattern
                .media
                .iter()
                .map(|key| model.media[*key].id.as_str())
                .collect::<Vec<_>>()
                .join("|");

            wtr.write_record([screen_id, usecase_id, api_id, &permissions, &media])?;
        }

        let data = wtr
            .into_inner()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(String::from_utf8(data).unwrap_or_default())
    }
}

// ── PermissionCallableCsvEmitter ─────────────────────────────────────────────

pub struct PermissionCallableCsvEmitter;

impl Emitter for PermissionCallableCsvEmitter {
    fn emit(&self, model: &SemanticModel, _view: &View) -> Result<String, EmitError> {
        let mut wtr = csv::Writer::from_writer(vec![]);
        wtr.write_record([
            "permission_id",
            "permission_label",
            "usecase_ids",
            "api_ids",
        ])?;

        for entry in derive_permission_callables(model) {
            let permission = &model.permissions[entry.permission];
            let usecase_ids = entry
                .usecases
                .iter()
                .map(|key| model.use_cases[*key].id.as_str())
                .collect::<Vec<_>>()
                .join("|");
            let api_ids = entry
                .apis
                .iter()
                .map(|key| model.apis[*key].id.as_str())
                .collect::<Vec<_>>()
                .join("|");

            wtr.write_record([
                permission.id.as_str(),
                permission.label.as_str(),
                &usecase_ids,
                &api_ids,
            ])?;
        }

        let data = wtr
            .into_inner()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(String::from_utf8(data).unwrap_or_default())
    }
}

// ── ActorPermissionAuditCsvEmitter ───────────────────────────────────────────

pub struct ActorPermissionAuditCsvEmitter;

impl Emitter for ActorPermissionAuditCsvEmitter {
    fn emit(&self, model: &SemanticModel, _view: &View) -> Result<String, EmitError> {
        let mut wtr = csv::Writer::from_writer(vec![]);
        wtr.write_record([
            "actor_id",
            "actor_label",
            "permission_id",
            "permission_label",
            "assigned",
            "required",
            "status",
            "required_usecase_ids",
            "required_api_paths",
        ])?;

        for entry in derive_actor_permission_audit(model) {
            let actor = &model.actors[entry.actor];
            let permission = &model.permissions[entry.permission];
            let required_usecase_ids = required_usecase_ids(model, &entry.sources);
            let required_api_paths = required_api_paths(model, &entry.sources);

            wtr.write_record([
                actor.id.as_str(),
                actor.label.as_str(),
                permission.id.as_str(),
                permission.label.as_str(),
                bool_str(entry.assigned),
                bool_str(entry.required),
                entry.status.as_str(),
                required_usecase_ids.as_str(),
                required_api_paths.as_str(),
            ])?;
        }

        let data = wtr
            .into_inner()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(String::from_utf8(data).unwrap_or_default())
    }
}

// ── BusinessInputCsvEmitter ──────────────────────────────────────────────────

pub struct BusinessInputCsvEmitter;

impl Emitter for BusinessInputCsvEmitter {
    fn emit(&self, model: &SemanticModel, _view: &View) -> Result<String, EmitError> {
        let mut wtr = csv::Writer::from_writer(vec![]);
        wtr.write_record([
            "actor_id",
            "actor_label",
            "buc_id",
            "usecase_id",
            "source_type",
            "source_id",
            "entity_id",
            "column_name",
            "column_type",
            "operation",
            "reason",
        ])?;

        for entry in derive_actor_input_inferences(model) {
            let actor = &model.actors[entry.actor];
            let buc_id = entry
                .buc
                .map(|key| model.bucs[key].id.as_str())
                .unwrap_or("");
            let usecase = &model.use_cases[entry.usecase];
            let entity = &model.entities[entry.entity];
            let column_type = entity
                .columns
                .iter()
                .find(|column| column.name == entry.column)
                .map(|column| col_type_to_str(&column.col_type))
                .unwrap_or("");
            let (source_type, source_id) = match entry.source {
                ActorInputSource::UseCase => ("usecase", usecase.id.as_str()),
                ActorInputSource::Api(api) => ("api", model.apis[api].id.as_str()),
            };

            wtr.write_record([
                actor.id.as_str(),
                actor.label.as_str(),
                buc_id,
                usecase.id.as_str(),
                source_type,
                source_id,
                entity.id.as_str(),
                entry.column.as_str(),
                column_type,
                entry.operation.as_str(),
                entry.reason.as_str(),
            ])?;
        }

        let data = wtr
            .into_inner()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(String::from_utf8(data).unwrap_or_default())
    }
}

// ── ヘルパー ──────────────────────────────────────────────────────────────────

fn required_usecase_ids(
    model: &SemanticModel,
    sources: &[rdra_ish_core::ActorPermissionRequirementSource],
) -> String {
    let mut ids: Vec<&str> = sources
        .iter()
        .filter(|source| source.api.is_none())
        .map(|source| model.use_cases[source.usecase].id.as_str())
        .collect();
    ids.sort();
    ids.dedup();
    ids.join("|")
}

fn required_api_paths(
    model: &SemanticModel,
    sources: &[rdra_ish_core::ActorPermissionRequirementSource],
) -> String {
    let mut paths: Vec<String> = sources
        .iter()
        .filter_map(|source| {
            source.api.map(|api| {
                format!(
                    "{}->{}",
                    model.use_cases[source.usecase].id, model.apis[api].id
                )
            })
        })
        .collect();
    paths.sort();
    paths.dedup();
    paths.join("|")
}

fn bool_str(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

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
    fn test_api_list_csv() {
        let src = r#"
api OrderApi "注文API"
api AuthApi "認証API"
"#;
        let model = model_from(src);
        let view = View::whole();
        let result = ApiListCsvEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("id,label"));
        assert!(result.contains("OrderApi,注文API"));
        assert!(result.contains("AuthApi,認証API"));
    }

    #[test]
    fn test_api_entity_matrix_csv() {
        let src = r#"
api OrderApi "注文API"
entity Order "注文" { id: Int @pk }
entity Cart "カート" { id: Int @pk }
creates(OrderApi, Order)
reads(OrderApi, Cart)
"#;
        let model = model_from(src);
        let view = View::whole();
        let result = ApiEntityMatrixCsvEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("Api"));
        assert!(result.contains("OrderApi"));
        assert!(result.contains('C')); // creates → C
        assert!(result.contains('R')); // reads  → R
    }

    #[test]
    fn test_screen_constraint_csv() {
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
        let model = model_from(src);
        let view = View::whole();
        let result = ScreenConstraintCsvEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("screen_id,usecase_id,api_id,required_permissions,required_media"));
        assert!(result.contains(
            "BookingScreen,BookAppointment,BookingApi,ScheduleWrite|PatientRead,StaffTerminal|SecureChannel"
        ));
    }

    #[test]
    fn test_permission_callable_csv() {
        let src = r#"
usecase BookAppointment "Book Appointment"
usecase CancelAppointment "Cancel Appointment"
api BookingApi "Booking API"
api CancelApi "Cancel API"
permission ScheduleWrite "Schedule Write"
permission PatientRead "Patient Read"
requires_permission(BookAppointment, ScheduleWrite)
requires_permission(BookAppointment, PatientRead)
requires_permission(CancelAppointment, ScheduleWrite)
requires_permission(BookingApi, PatientRead)
requires_permission(CancelApi, ScheduleWrite)
"#;
        let model = model_from(src);
        let view = View::whole();
        let result = PermissionCallableCsvEmitter.emit(&model, &view).unwrap();
        assert!(result.contains("permission_id,permission_label,usecase_ids,api_ids"));
        assert!(result
            .contains("ScheduleWrite,Schedule Write,BookAppointment|CancelAppointment,CancelApi"));
        assert!(result.contains("PatientRead,Patient Read,BookAppointment,BookingApi"));
    }

    #[test]
    fn test_actor_permission_audit_csv() {
        let src = r#"
actor Staff "Staff"
usecase BookAppointment "Book Appointment"
api BookingApi "Booking API"
permission ScheduleWrite "Schedule Write"
permission LegacyAdmin "Legacy Admin"
performs(Staff, BookAppointment)
has_permission(Staff, LegacyAdmin)
requires_permission(BookAppointment, ScheduleWrite)
invokes(BookAppointment, BookingApi)
requires_permission(BookingApi, ScheduleWrite)
"#;
        let model = model_from(src);
        let view = View::whole();
        let result = ActorPermissionAuditCsvEmitter.emit(&model, &view).unwrap();
        assert!(result.contains(
            "actor_id,actor_label,permission_id,permission_label,assigned,required,status,required_usecase_ids,required_api_paths"
        ));
        assert!(result.contains("Staff,Staff,LegacyAdmin,Legacy Admin,true,false,excess,,"));
        assert!(result.contains(
            "Staff,Staff,ScheduleWrite,Schedule Write,false,true,missing,BookAppointment,BookAppointment->BookingApi"
        ));
    }

    #[test]
    fn test_actor_input_inference_csv() {
        let src = r#"
actor Staff "Staff"
buc BucScheduling "Scheduling"
usecase BookAppointment "Book Appointment"
api BookingApi "Booking API"
entity Appointment "Appointment" { id: Int @pk  patient_name: String  scheduled_at: DateTime }
performs(Staff, BookAppointment)
contains(BucScheduling, BookAppointment)
invokes(BookAppointment, BookingApi)
creates(BookingApi, Appointment)
"#;
        let model = model_from(src);
        let view = View::whole();
        let result = BusinessInputCsvEmitter.emit(&model, &view).unwrap();
        assert!(result.contains(
            "actor_id,actor_label,buc_id,usecase_id,source_type,source_id,entity_id,column_name,column_type,operation,reason"
        ));
        assert!(result.contains(
            "Staff,Staff,BucScheduling,BookAppointment,api,BookingApi,Appointment,patient_name,String,create"
        ));
        assert!(result.contains(
            "Staff,Staff,BucScheduling,BookAppointment,api,BookingApi,Appointment,scheduled_at,DateTime,create"
        ));
        assert!(!result.contains(",id,"));
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
