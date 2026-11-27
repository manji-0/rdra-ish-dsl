//! RDRA-ish Language Server.

pub mod backend;
pub mod catch_panic;
pub mod code_actions;
pub mod code_fingerprints;
pub mod code_lens;
pub mod completion;
pub mod convert;
pub mod document;
pub mod folding;
pub mod hover;
pub mod inlay_hints;
pub mod lifecycle;
pub mod linked_editing;
pub mod predicates;
pub mod refs;
pub mod rename;
pub mod semantic_tokens;
pub mod symbols;
pub mod uri;

#[cfg(test)]
mod workspace_test;

pub use backend::Backend;

/// Serves the language server over stdin and stdout until the client exits. Both
/// `rdra-ish-lsp` and `rdra-ish lsp` run this; stdout carries only the protocol.
pub fn run_stdio() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("start the tokio runtime");
    runtime.block_on(async {
        let (service, socket) = tower_lsp::LspService::new(Backend::new);
        tower_lsp::Server::new(tokio::io::stdin(), tokio::io::stdout(), socket)
            .serve(catch_panic::CatchPanic(service))
            .await;
    });
}
