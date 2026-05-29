//! rdra-core: RDRA semantic model and validation.

pub mod analysis;
pub mod diagnostics;
pub mod model;
pub mod resolver;
pub mod state_pattern;
pub mod tx;

pub use analysis::build_model;
pub use diagnostics::{Diagnostic, RdraError};
pub use model::{ColumnEffect, EffectValue, EntityKey, SemanticModel, StateTransition};
pub use state_pattern::{
    derive_state_patterns, AbstractValue, AxisKind, EntityStateResult, ReachablePattern,
    StateAxis, StateDiag, StatePattern, DEFAULT_PATTERN_CAP,
};
pub use resolver::{build_merged_model, reachable_from_bucs, resolve, ResolvedProgram};
pub use tx::{infer_usecase_transactions, tx_diagnostics, TxGroup, UcWrite, UsecaseTx, WriteKind};
