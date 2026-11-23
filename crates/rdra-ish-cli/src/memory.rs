//! What a command may use in memory (`--memory`, else `memory` in `rdra.toml`, else the
//! command's default), and how `verify` splits it between BFS and the TLC runs.

use anyhow::{bail, Result};
use rdra_ish_core::memory::MemorySize;
use rdra_ish_core::project::Manifest;

use crate::tlc_env::Heap;

const MIB: u64 = 1 << 20;

/// A command with a memory limit, and the `rdra.toml` section that sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Budgeted {
    /// `check` and `states`: `[check]`.
    Check,
    /// `verify`: `[verify]`.
    Verify,
}

impl Budgeted {
    /// How to raise the limit, named in the message of a run that stops at it.
    pub(crate) fn raise(self) -> &'static str {
        match self {
            Budgeted::Check => "--memory or `memory` under [check] in rdra.toml",
            Budgeted::Verify => "--memory or `memory` under [verify] in rdra.toml",
        }
    }
}

/// `--memory`, else the manifest's `memory` for the command, else the command's default.
pub(crate) fn memory_limit(
    flag: Option<MemorySize>,
    manifest: Option<&Manifest>,
    command: Budgeted,
) -> MemorySize {
    let section = manifest.map(|m| match command {
        Budgeted::Check => &m.check,
        Budgeted::Verify => &m.verify,
    });
    flag.or(section.and_then(|s| s.memory))
        .unwrap_or(match command {
            Budgeted::Check => MemorySize::CHECK,
            Budgeted::Verify => MemorySize::VERIFY,
        })
}

/// The least heap a TLC run gets. TLC checks a small cone in 32 MiB; a `--memory` that
/// leaves no BFS beside this much is refused.
const LEAST_HEAP: u64 = 64 * MIB;

/// How `verify` spends its memory. Without `--with-tlc` BFS holds all of it. With it, while
/// BFS, the proof, and the TLC cone runs go side by side, the cone runs hold half, or
/// `LEAST_HEAP` when half is less, and BFS the rest; the TLC whole-model runs come after BFS
/// has let go, so they may take all of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VerifySplit {
    pub(crate) limit: MemorySize,
    /// What the BFS state stores may hold at once.
    pub(crate) bfs: MemorySize,
    /// What the `-Xmx` of the TLC cone runs going at once may add up to.
    pub(crate) cones: u64,
    /// What the `-Xmx` of the whole-model runs going at once may add up to.
    pub(crate) wholes: u64,
}

impl VerifySplit {
    pub(crate) fn of(limit: MemorySize, with_tlc: bool) -> Result<Self> {
        let bytes = limit.bytes();
        if !with_tlc {
            return Ok(VerifySplit {
                limit,
                bfs: limit,
                cones: 0,
                wholes: 0,
            });
        }
        let cones = (bytes - bytes / 2).max(LEAST_HEAP);
        let Some(bfs) = MemorySize::from_bytes(bytes.saturating_sub(cones)) else {
            bail!(
                "--memory {limit} is too small for verify: a TLC run takes {}, and BFS needs some beside it",
                MemorySize::from_bytes(LEAST_HEAP).expect("not zero")
            );
        };
        Ok(VerifySplit {
            limit,
            bfs,
            cones,
            wholes: bytes,
        })
    }
}

/// How many TLC runs of one kind go at once, and the heap each gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TlcPlan {
    pub(crate) at_once: usize,
    /// `-Xmx` in bytes.
    pub(crate) heap: u64,
}

/// Up to `wanted` runs of `kind` at once within `share` bytes of heap. Runs are queued before a
/// heap is shrunk: fewer at once only costs time, a smaller heap may fail a run. `set` is a
/// heap the user named (`RDRA_TLC_HEAP`, or `-Xmx` in `JAVA_TOOL_OPTIONS`); it is kept as
/// given, and runs one at a time when it alone is past the share. Otherwise the kind's
/// default heap shrinks to the share when the share is smaller.
pub(crate) fn tlc_plan(share: u64, wanted: usize, kind: Heap, set: Option<u64>) -> TlcPlan {
    let heap = set.unwrap_or_else(|| kind.default_bytes().min(share).max(LEAST_HEAP));
    let fits = usize::try_from(share / heap).unwrap_or(usize::MAX);
    TlcPlan {
        at_once: wanted.min(fits).max(1),
        heap,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::project::CommandSection;

    const GIB: u64 = 1 << 30;

    fn size(text: &str) -> MemorySize {
        text.parse().unwrap()
    }

    fn manifest(check: Option<&str>, verify: Option<&str>) -> Manifest {
        Manifest {
            path: "rdra.toml".into(),
            model: rdra_ish_core::project::ModelSection {
                name: "m".to_string(),
            },
            dependencies: Default::default(),
            check: CommandSection {
                memory: check.map(size),
            },
            verify: CommandSection {
                memory: verify.map(size),
            },
        }
    }

    #[test]
    fn the_flag_beats_the_manifest_which_beats_the_default() {
        let both = manifest(Some("1GiB"), Some("2GiB"));
        let neither = manifest(None, None);
        for (flag, m, command, want) in [
            (Some("512MiB"), Some(&both), Budgeted::Check, "512MiB"),
            (Some("512MiB"), Some(&both), Budgeted::Verify, "512MiB"),
            (None, Some(&both), Budgeted::Check, "1GiB"),
            (None, Some(&both), Budgeted::Verify, "2GiB"),
            (None, Some(&neither), Budgeted::Check, "4GiB"),
            (None, Some(&neither), Budgeted::Verify, "16GiB"),
            (None, None, Budgeted::Check, "4GiB"),
            (None, None, Budgeted::Verify, "16GiB"),
        ] {
            let got = memory_limit(flag.map(size), m, command);
            assert_eq!(got.to_string(), want, "{flag:?} {command:?}");
        }
    }

    #[test]
    fn the_default_verify_budget_runs_today_plan() {
        let split = VerifySplit::of(MemorySize::VERIFY, true).unwrap();
        assert_eq!(split.bfs.to_string(), "8GiB");
        assert_eq!(
            tlc_plan(split.cones, 8, Heap::Cone, None),
            TlcPlan {
                at_once: 8,
                heap: GIB
            }
        );
        assert_eq!(
            tlc_plan(split.wholes, 2, Heap::Temporal, None),
            TlcPlan {
                at_once: 2,
                heap: 2 * GIB
            }
        );
    }

    #[test]
    fn a_smaller_budget_queues_the_runs_before_it_shrinks_a_heap() {
        let split = VerifySplit::of(size("4GiB"), true).unwrap();
        assert_eq!(
            tlc_plan(split.cones, 8, Heap::Cone, None),
            TlcPlan {
                at_once: 2,
                heap: GIB
            }
        );
        assert_eq!(
            tlc_plan(split.wholes, 2, Heap::Temporal, None),
            TlcPlan {
                at_once: 2,
                heap: 2 * GIB
            }
        );
        let split = VerifySplit::of(size("1GiB"), true).unwrap();
        assert_eq!(
            tlc_plan(split.cones, 8, Heap::Cone, None),
            TlcPlan {
                at_once: 1,
                heap: GIB / 2
            }
        );
        assert_eq!(
            tlc_plan(split.wholes, 2, Heap::Temporal, None),
            TlcPlan {
                at_once: 1,
                heap: GIB
            }
        );
        let split = VerifySplit::of(size("80MiB"), true).unwrap();
        assert_eq!(split.bfs.to_string(), "16MiB");
        assert_eq!(
            tlc_plan(split.cones, 8, Heap::Cone, None),
            TlcPlan {
                at_once: 1,
                heap: 64 * MIB
            }
        );
    }

    #[test]
    fn a_heap_the_user_names_is_kept_and_queued_to_fit() {
        let split = VerifySplit::of(MemorySize::VERIFY, true).unwrap();
        assert_eq!(
            tlc_plan(split.cones, 8, Heap::Cone, Some(4 * GIB)),
            TlcPlan {
                at_once: 2,
                heap: 4 * GIB
            }
        );
        assert_eq!(
            tlc_plan(split.cones, 8, Heap::Cone, Some(32 * GIB)),
            TlcPlan {
                at_once: 1,
                heap: 32 * GIB
            }
        );
    }

    #[test]
    fn verify_refuses_a_budget_too_small_for_one_tlc_run() {
        let err = VerifySplit::of(size("64MiB"), true)
            .unwrap_err()
            .to_string();
        assert!(err.contains("a TLC run takes 64MiB"), "{err}");
        assert_eq!(
            VerifySplit::of(size("65MiB"), true)
                .unwrap()
                .bfs
                .to_string(),
            "1MiB"
        );
    }

    /// Without `--with-tlc` no TLC run takes a share, so a budget too small for one is fine.
    #[test]
    fn without_tlc_bfs_holds_the_whole_budget() {
        let split = VerifySplit::of(size("64MiB"), false).unwrap();
        assert_eq!(split.bfs.to_string(), "64MiB");
    }
}
