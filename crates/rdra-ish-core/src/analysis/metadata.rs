//! Instance metadata emptiness checks used during registration.

use rdra_ish_syntax::ast::{
    AdrMetadata, ApiMetadata, FieldMetadata, NfrMetadata, RequirementMetadata, UseCaseMetadata,
};

pub(crate) fn requirement_metadata_is_empty(metadata: &RequirementMetadata) -> bool {
    metadata.priority.is_none()
        && metadata.sources.is_empty()
        && metadata.stakeholders.is_empty()
        && metadata.owner.is_none()
        && metadata.acceptance_criteria.is_empty()
        && metadata.status.is_none()
        && metadata.risk.is_none()
        && metadata.rationale.is_none()
}

pub(crate) fn adr_metadata_is_empty(metadata: &AdrMetadata) -> bool {
    metadata.status.is_none()
        && metadata.context.is_empty()
        && metadata.decision.is_none()
        && metadata.consequences.is_empty()
        && metadata.accepted_options.is_empty()
        && metadata.rejected_options.is_empty()
        && metadata.reasons.is_empty()
}

pub(crate) fn api_metadata_is_empty(metadata: &ApiMetadata) -> bool {
    metadata.method.is_none()
        && metadata.path.is_none()
        && metadata.idempotency.is_none()
        && metadata.mode.is_none()
        && metadata.auth_scheme.is_none()
}

pub(crate) fn nfr_metadata_is_empty(metadata: &NfrMetadata) -> bool {
    metadata.metric.is_none()
        && metadata.target.is_none()
        && metadata.window.is_none()
        && metadata.slo.is_none()
        && metadata.availability.is_none()
        && metadata.resilience.is_none()
        && metadata.audit.is_none()
        && metadata.logging.is_none()
        && metadata.retention.is_none()
        && metadata.privacy.is_none()
}

pub(crate) fn field_metadata_is_empty(metadata: &FieldMetadata) -> bool {
    metadata.access.is_none() && metadata.required.is_none() && metadata.source.is_none()
}

pub(crate) fn usecase_metadata_is_empty(metadata: &UseCaseMetadata) -> bool {
    metadata.preconditions.is_empty()
        && metadata.postconditions.is_empty()
        && metadata.guards.is_empty()
        && metadata.alternatives.is_empty()
        && metadata.errors.is_empty()
}
