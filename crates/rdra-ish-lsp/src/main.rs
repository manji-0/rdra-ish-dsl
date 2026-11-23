use rdra_ish_lsp::Backend;
use tower_lsp::{LspService, Server};

// Freed stores leave the process's memory, so the memory limit bounds what it holds (#242).
#[global_allocator]
static ALLOCATOR: rdra_ish_core::allocator::Allocator = rdra_ish_core::allocator::Allocator;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
