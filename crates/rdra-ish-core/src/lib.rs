//! rdra-core: RDRA semantic model and validation.

pub mod access;
pub mod analysis;
pub mod concept_mapping;
pub mod diagnostics;
pub mod entity_lifecycle;
pub mod event_flow;
pub mod model;
pub mod resolver;
pub mod state_pattern;
pub mod system;
pub mod tx;
pub mod typed_predicate;

pub use access::{
    derive_actor_input_inferences, derive_actor_permission_audit, derive_permission_callables,
    derive_screen_constraint_patterns, permission_diagnostics, ActorInputInference,
    ActorInputOperation, ActorInputSource, ActorPermissionAudit, ActorPermissionAuditStatus,
    ActorPermissionRequirementSource, PermissionApiPath, PermissionCallable,
    ScreenConstraintPattern,
};
pub use analysis::build_model;
pub use concept_mapping::{
    collect_concept_mappings, conceptual_id, mappings_for_conceptual, mappings_for_entity,
};
pub use diagnostics::{Diagnostic, RdraError};
pub use entity_lifecycle::{collect_entity_lifecycles, link_entity_status_states, EntityLifecycle};
pub use event_flow::{api_diagnostics, collect_event_flows, event_diagnostics, EventFlow};
pub use model::{
    Adr, AdrKey, Api, ApiKey, AppliesToTarget, Cardinality, ColumnEffect, ConceptMapping,
    ConceptualRef, ConstrainsTarget, ContainedRef, ContainerRef, CoversTarget, CrossCmpRhs,
    CrossComparisonProp, CrossEntityCondition, CrossEntityInvariant, CrossForbiddenConstraint,
    DataOrigin, DecidesTarget, EffectValue, EntityKey, EntityTouchpoint, ExclusiveConstraint,
    Location, LocationKey, Medium, MediumKey, NfrOrConstraint, PerformTarget, Permission,
    PermissionKey, QualifiedModelColumnRef, RequiredConstraint, SemanticModel, StateTransition,
    System, SystemKey, Timing, TimingKey, TriggerTarget, TypedPredicate,
};
pub use resolver::{build_merged_model, reachable_from_bucs, resolve, ResolvedProgram};
pub use state_pattern::{
    derive_state_patterns, AbstractValue, AxisKind, EntityStateResult, ReachablePattern, StateAxis,
    StateDiag, StatePattern, DEFAULT_PATTERN_CAP,
};
pub use system::{derive_system_boundaries, system_diagnostics, SystemBoundary};
pub use tx::{infer_usecase_transactions, tx_diagnostics, TxGroup, UcWrite, UsecaseTx, WriteKind};
pub use typed_predicate::{build_typed_predicate, collect_typed_predicates, typed_predicate_name};
