// Freed stores leave the process's memory, so `--memory` bounds what it holds (#242).
#[global_allocator]
static ALLOCATOR: rdra_ish_core::allocator::Allocator = rdra_ish_core::allocator::Allocator;

fn main() -> anyhow::Result<()> {
    rdra_ish_cli::run()
}
