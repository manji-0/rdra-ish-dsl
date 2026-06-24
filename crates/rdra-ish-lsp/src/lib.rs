//! RDRA-ish Language Server.

pub mod backend;
pub mod code_actions;
pub mod code_lens;
pub mod completion;
pub mod convert;
pub mod hover;
pub mod inlay_hints;
pub mod linked_editing;
pub mod predicates;
pub mod refs;
pub mod rename;
pub mod semantic_tokens;
pub mod symbols;

#[cfg(test)]
mod workspace_test;

pub use backend::Backend;
