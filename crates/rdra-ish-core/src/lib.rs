//! rdra-core: RDRA semantic model and validation.

pub mod analysis;
pub mod diagnostics;
pub mod event_flow;
pub mod model;
pub mod resolver;
pub mod state_pattern;
pub mod tx;

pub use analysis::build_model;
pub use diagnostics::{Diagnostic, RdraError};
pub use event_flow::{collect_event_flows, event_diagnostics, EventFlow};
pub use model::{ColumnEffect, EffectValue, EntityKey, SemanticModel, StateTransition};
pub use resolver::{build_merged_model, reachable_from_bucs, resolve, ResolvedProgram};
pub use state_pattern::{
    derive_state_patterns, AbstractValue, AxisKind, EntityStateResult, ReachablePattern, StateAxis,
    StateDiag, StatePattern, DEFAULT_PATTERN_CAP,
};
pub use tx::{infer_usecase_transactions, tx_diagnostics, TxGroup, UcWrite, UsecaseTx, WriteKind};
