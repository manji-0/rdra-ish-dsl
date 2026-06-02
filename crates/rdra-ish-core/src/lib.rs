//! rdra-core: RDRA semantic model and validation.

pub mod access;
pub mod analysis;
pub mod diagnostics;
pub mod event_flow;
pub mod model;
pub mod resolver;
pub mod state_pattern;
pub mod system;
pub mod tx;

pub use access::{
    derive_actor_input_inferences, derive_actor_permission_audit, derive_permission_callables,
    derive_screen_constraint_patterns, permission_diagnostics, ActorInputInference,
    ActorInputOperation, ActorInputSource, ActorPermissionAudit, ActorPermissionAuditStatus,
    ActorPermissionRequirementSource, PermissionApiPath, PermissionCallable,
    ScreenConstraintPattern,
};
pub use analysis::build_model;
pub use diagnostics::{Diagnostic, RdraError};
pub use event_flow::{api_diagnostics, collect_event_flows, event_diagnostics, EventFlow};
pub use model::{
    Api, ApiKey, ColumnEffect, CrossCmpRhs, CrossComparisonProp, CrossEntityCondition,
    CrossEntityInvariant, CrossForbiddenConstraint, EffectValue, EntityKey, ExclusiveConstraint,
    Location, LocationKey, Medium, MediumKey, Permission, PermissionKey, QualifiedModelColumnRef,
    RequiredConstraint, SemanticModel, StateTransition, System, SystemKey, Timing, TimingKey,
};
pub use resolver::{build_merged_model, reachable_from_bucs, resolve, ResolvedProgram};
pub use state_pattern::{
    derive_state_patterns, AbstractValue, AxisKind, EntityStateResult, ReachablePattern, StateAxis,
    StateDiag, StatePattern, DEFAULT_PATTERN_CAP,
};
pub use system::{derive_system_boundaries, system_diagnostics, SystemBoundary};
pub use tx::{infer_usecase_transactions, tx_diagnostics, TxGroup, UcWrite, UsecaseTx, WriteKind};
