//! 概念モデル要素と entity の `maps_to` 対応の集約 API。

use crate::model::{ConceptMapping, ConceptualRef, EntityKey, SemanticModel};

/// モデル内の全 `maps_to` 対応を収集する（source id → entity id の辞書順）。
pub fn collect_concept_mappings(model: &SemanticModel) -> Vec<ConceptMapping> {
    let mut mappings = model.concept_mappings.clone();
    mappings.sort_by_key(|mapping| {
        (
            conceptual_id(model, &mapping.source),
            model.entities[mapping.entity].id.clone(),
        )
    });
    mappings
}

/// 指定 entity にマップされた概念モデル要素を返す。
pub fn mappings_for_entity(model: &SemanticModel, entity: EntityKey) -> Vec<&ConceptMapping> {
    model
        .concept_mappings
        .iter()
        .filter(|mapping| mapping.entity == entity)
        .collect()
}

/// 指定概念モデル要素の `maps_to` 対応を返す（複数 entity へのマップを許容）。
pub fn mappings_for_conceptual<'a>(
    model: &'a SemanticModel,
    source: &ConceptualRef,
) -> Vec<&'a ConceptMapping> {
    model
        .concept_mappings
        .iter()
        .filter(|mapping| &mapping.source == source)
        .collect()
}

/// 概念モデル要素の宣言 id を返す。
pub fn conceptual_id(model: &SemanticModel, source: &ConceptualRef) -> String {
    match source {
        ConceptualRef::Concept(k) => model.concepts[*k].id.clone(),
        ConceptualRef::DomainObject(k) => model.domain_objects[*k].id.clone(),
        ConceptualRef::Aggregate(k) => model.aggregates[*k].id.clone(),
        ConceptualRef::ValueObject(k) => model.value_objects[*k].id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::build_model;
    use rdra_ish_syntax::parse;

    fn model_from(src: &str) -> SemanticModel {
        let (ast, parse_errors) = parse(src);
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        let (model, diags) = build_model(&ast);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        model
    }

    const MAPPING_SRC: &str = r#"
concept PatientIdentity "Patient identity"
domain_object Appointment "Appointment"
valueobject TimeSlot "Time slot"
entity AppointmentTable "appointment table" { id: Int @pk }
maps_to(Appointment, AppointmentTable)
maps_to(TimeSlot, AppointmentTable)
"#;

    #[test]
    fn collect_concept_mappings_sorted() {
        let model = model_from(MAPPING_SRC);
        let mappings = collect_concept_mappings(&model);
        assert_eq!(mappings.len(), 2);

        let source_ids: Vec<_> = mappings
            .iter()
            .map(|m| conceptual_id(&model, &m.source))
            .collect();
        assert_eq!(source_ids, vec!["Appointment", "TimeSlot"]);

        for mapping in &mappings {
            assert_eq!(model.entities[mapping.entity].id, "AppointmentTable");
        }
    }

    #[test]
    fn mappings_for_entity_and_conceptual() {
        let model = model_from(MAPPING_SRC);
        let entity = model
            .entities
            .keys()
            .find(|ek| model.entities[*ek].id == "AppointmentTable")
            .expect("entity");

        let entity_mappings = mappings_for_entity(&model, entity);
        assert_eq!(entity_mappings.len(), 2);

        let appointment = model
            .domain_objects
            .keys()
            .find(|dk| model.domain_objects[*dk].id == "Appointment")
            .map(ConceptualRef::DomainObject)
            .expect("domain object");
        let appointment_mappings = mappings_for_conceptual(&model, &appointment);
        assert_eq!(appointment_mappings.len(), 1);
        assert_eq!(appointment_mappings[0].entity, entity);
    }

    #[test]
    fn unmapped_conceptual_has_no_mappings() {
        let model = model_from(MAPPING_SRC);
        let concept = model
            .concepts
            .keys()
            .find(|ck| model.concepts[*ck].id == "PatientIdentity")
            .map(ConceptualRef::Concept)
            .expect("concept");
        assert!(mappings_for_conceptual(&model, &concept).is_empty());
    }
}
