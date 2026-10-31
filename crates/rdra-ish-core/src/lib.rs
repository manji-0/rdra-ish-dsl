//! rdra-core: RDRA semantic model and validation.

pub mod access;
pub mod analysis;
pub(crate) mod analysis_diag;
pub mod diagnostics;
pub mod entity_lifecycle;
pub mod entity_state_variant;
pub mod event_flow;
pub mod import_scope;
pub mod lifecycle;
pub mod lint;
pub mod location;
pub mod model;
pub mod predicate;
pub mod project;
pub mod rdra_rules;
pub mod refinement;
pub mod resolver;
pub mod system;
pub mod tx;
pub mod typed_predicate;
pub mod workspace;

pub use access::{
    actors_for_usecase, derive_actor_input_inferences, derive_actor_permission_audit,
    derive_permission_callables, derive_screen_constraint_patterns, permission_diagnostics,
    ActorInputInference, ActorInputOperation, ActorInputSource, ActorPermissionAudit,
    ActorPermissionAuditStatus, ActorPermissionRequirementSource, PermissionApiPath,
    PermissionCallable, ScreenConstraintPattern,
};
pub use analysis::{build_model, build_model_items, build_model_items_with_scopes};
pub use diagnostics::{Diagnostic, RdraError};
pub use entity_lifecycle::link_entity_status_states;
pub use entity_state_variant::{
    collect_entity_state_variants, entity_state_variants, EntityStateVariant, EntityStateVariants,
    StateFieldValue,
};
pub use event_flow::{
    api_diagnostics, api_route_diagnostics, collect_event_flows, collect_timer_flows,
    event_diagnostics, EventFlow, TimerFlow,
};
pub use import_scope::{build_import_scopes, FileScope, FlatBinding, ImportScopes};
pub use lint::{lint_issues, lint_review_diagnostics, LintIssue, LintSeverity};
pub use location::{
    byte_offset_to_position, format_diagnostic_message, format_location, push_decl_diagnostic,
    push_model_decl_diagnostic, DeclIndex, DeclSite, DiagCtxt, LocatedSpan, Position, SourceId,
};
pub use model::{
    node_ref_kind, Adr, AdrKey, Api, ApiKey, AppliesToTarget, ColumnEffect, ConstrainsTarget,
    ContainedRef, ContainerRef, CoversTarget, CrossCmpRhs, CrossComparisonProp,
    CrossEntityCondition, CrossEntityInvariant, CrossForbiddenConstraint, DecidesTarget,
    EffectValue, EntityKey, EntityTouchpoint, ExclusiveConstraint, LookupResult, Medium, MediumKey,
    NfrOrConstraint, PerformTarget, Permission, PermissionKey, QualifiedModelColumnRef, Raiser,
    RequiredConstraint, SemanticModel, StateTransition, System, SystemKey, TemporalAtom,
    TemporalExpr, TemporalFormula, TemporalProperty, TemporalRhs, Timer, TimerKey, TriggerSource,
    TriggerTarget, TypedPredicate,
};
pub use predicate::{predicate_signature, KNOWN_PREDICATES};
pub use project::{
    collect_rdra_files, discover, load_project, root_include_paths, Manifest, ModelSection,
    Project, ProjectError,
};
pub use resolver::{
    build_merged_model, reachable_from_bucs, resolve, resolve_overlaid, ResolvedProgram,
};
pub use system::{derive_system_boundaries, system_diagnostics, SystemBoundary};
pub use tx::{
    infer_usecase_transactions, tx_diagnostics, TxGroup, TxKind, UcWrite, UsecaseTx, WriteKind,
};
pub use typed_predicate::{build_typed_predicate, collect_typed_predicates, typed_predicate_name};
pub use workspace::{
    analyze_project, analyze_workspace, consistency_diagnostics, WorkspaceAnalysis,
};
