//! What a command may use in memory (`--memory`, else `memory` in `rdra.toml`, else the
//! command's default), the caps `check` and the generators explore at, and how `verify`
//! splits its memory between BFS and the TLC runs.

use anyhow::{bail, Result};
use rdra_ish_core::lifecycle::{Bounds, Caps};
use rdra_ish_core::memory::{MemoryPool, MemorySize};
use rdra_ish_core::project::Manifest;

use crate::tlc_env::{xmx, Heap};

const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;

/// A command with a memory limit, and the `rdra.toml` section that sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Budgeted {
    /// `check` and `states`: `[check]`.
    Check,
    /// The generators: the outputs that read the explored states, and `codegen` and `next`,
    /// which run `check`'s findings first. `[check]`, as `check`; they take no `--memory`.
    Generators,
    /// `verify`: `[verify]`.
    Verify,
}

impl Budgeted {
    /// How to raise the limit, named in the message of a run that stops at it.
    pub(crate) fn raise(self) -> &'static str {
        match self {
            Budgeted::Check => "--memory or `memory` under [check] in rdra.toml",
            Budgeted::Generators => "`memory` under [check] in rdra.toml",
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
        Budgeted::Check | Budgeted::Generators => &m.check,
        Budgeted::Verify => &m.verify,
    });
    flag.or(section.and_then(|s| s.memory))
        .unwrap_or(match command {
            Budgeted::Check | Budgeted::Generators => MemorySize::CHECK,
            Budgeted::Verify => MemorySize::VERIFY,
        })
}

/// `caps` with every run charged to one pool of `command`'s memory limit.
pub(crate) fn charged(
    caps: Caps,
    flag: Option<MemorySize>,
    manifest: Option<&Manifest>,
    command: Budgeted,
) -> Caps {
    caps.charged_to(MemoryPool::for_command(
        memory_limit(flag, manifest, command),
        command.raise(),
    ))
}

/// What the generators explore at: the caps `check` runs at by default, at the bounds the
/// generators run at (`Bounds::STATES`), so no state cap and the memory of `[check]`. A model
/// `check` passes at its defaults explores the same here.
pub(crate) fn generator_caps(manifest: Option<&Manifest>) -> Caps {
    charged(
        Caps::for_instances(Bounds::STATES.instances.get().into()),
        None,
        manifest,
        Budgeted::Generators,
    )
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
    /// What the TLC cone runs going at once may hold: their `-Xmx` and `JVM_OVERHEAD` each.
    pub(crate) cones: u64,
    /// What the whole-model runs going at once may hold, counted the same way.
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

/// What a TLC JVM holds beside its heap: 64 MiB of fingerprints off the heap, metaspace, code
/// cache, and stacks, about 250 MB once the heap fills (#309). A run is charged its `-Xmx`
/// and this.
pub(crate) const JVM_OVERHEAD: u64 = 256 * MIB;

/// Heap per byte of module text: 512 MiB per MiB. TLC was measured to need 183 to 269 MiB per
/// MiB to parse and process a module (#309), and twice that also holds the 7,167,444 states
/// of the 7.6 MB cone of #299, which checks at 4 GiB and not at 2.
const HEAP_PER_MODULE_BYTE: u64 = 512;

/// Heap per state BFS counted, under `RDRA_TLC_HEAP=states` (#384): 12.8M states then get
/// 8 GiB, where #384 measured 8g and 16g the same and 5g 15% slower. A guess past that one
/// point: a safety cone of 1.34M states ran in 64 MiB at full speed (#309).
const HEAP_PER_STATE: u64 = 640;

/// The heap of one TLC run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TlcHeap {
    /// Sized by the module (`TlcHeap::of`); doubled once when the run runs out of memory and
    /// the share holds twice it (`TlcHeap::doubled`).
    Sized(u64),
    /// The heap the user named (`RDRA_TLC_HEAP`, or `-Xmx` in `JAVA_TOOL_OPTIONS`), kept as
    /// given. A user `-Xmx` comes after the one `verify` passes, so a rerun at twice the heap
    /// would rerun at the same one: there is none.
    User(u64),
    /// The module needs `need` bytes of heap, more than the share: no run.
    OverShare { need: u64 },
}

impl TlcHeap {
    /// The heap of a run of `kind` on a module of `module_bytes` bytes within `share`: 512 MiB
    /// per MiB of module, rounded up to whole GiB, at least the kind's default (`Heap`) and at
    /// most the share; at the share, or `LEAST_HEAP`, when the default is more than the share
    /// holds. A module that needs more than the share gets no run, since a run that runs out
    /// of memory spends far more than it would queueing (a 12.6 MB module spent 303 CPU-s in
    /// garbage collection before it stopped at 2 GiB). `set`, the heap the user named, wins.
    pub(crate) fn of(module_bytes: u64, kind: Heap, share: u64, set: Option<u64>) -> TlcHeap {
        if let Some(set) = set {
            return TlcHeap::User(set);
        }
        let need = module_bytes.saturating_mul(HEAP_PER_MODULE_BYTE);
        let whole_gib = need.div_ceil(GIB).saturating_mul(GIB);
        if need > share {
            return TlcHeap::OverShare { need: whole_gib };
        }
        TlcHeap::Sized(
            whole_gib
                .max(kind.default_bytes())
                .min(share)
                .max(LEAST_HEAP),
        )
    }

    /// `-Xmx` in bytes; `None` for a run over the share.
    pub(crate) fn bytes(self) -> Option<u64> {
        match self {
            TlcHeap::Sized(bytes) | TlcHeap::User(bytes) => Some(bytes),
            TlcHeap::OverShare { .. } => None,
        }
    }

    /// What the run holds of its share while it goes: its heap and `JVM_OVERHEAD`.
    pub(crate) fn charge(self) -> u64 {
        self.bytes().map_or(0, |bytes| bytes + JVM_OVERHEAD)
    }

    /// The run-line note of a run over the share, and the report in place of TLC's output:
    /// why it did not run, and what to change.
    pub(crate) fn over_share(
        module: &str,
        module_bytes: u64,
        need: u64,
        share: u64,
    ) -> (String, String) {
        let size =
            |bytes: u64| MemorySize::from_bytes(bytes).map_or("0B".to_string(), |s| s.to_string());
        let flag = xmx(need);
        let note = format!(
            "not run: needs a {} heap, more than the {} TLC has of --memory",
            size(need),
            size(share)
        );
        let report = format!(
            "not run: {module}.tla is {:.1} MB, and TLC needs about 512 MiB of heap per MiB of module, so {}; TLC's runs have {} of --memory\n\
             hint: TLC would run out of memory; raise --memory (TLC's cone runs get half of it, a whole-model run all of it), or set the heap with RDRA_TLC_HEAP={flag} or JAVA_TOOL_OPTIONS=-Xmx{flag}",
            module_bytes as f64 / 1e6,
            size(need),
            size(share),
        );
        (note, report)
    }

    /// This heap raised for a run whose module BFS counted `states` states in (#384,
    /// `RDRA_TLC_HEAP=states`): 640 B per state in whole GiB, when that is more than the
    /// module's rule gave, and at most the share. It never lowers a heap, nor makes a run
    /// that runs `not run`, and a heap the user names or one over the share stays as it is.
    /// The bytes per state are fitted to #384's 8g floor at about 12.8M states, a guess.
    pub(crate) fn with_states(self, states: u64, share: u64) -> TlcHeap {
        match self {
            TlcHeap::Sized(bytes) => {
                let need = states.saturating_mul(HEAP_PER_STATE);
                let whole_gib = need.div_ceil(GIB).saturating_mul(GIB);
                TlcHeap::Sized(bytes.max(whole_gib.min(share)))
            }
            other => other,
        }
    }

    /// The heap of the one rerun of a run that ran out of memory: twice this one, when the
    /// heap was sized here and the share holds twice it.
    pub(crate) fn doubled(self, share: u64) -> Option<TlcHeap> {
        match self {
            TlcHeap::Sized(bytes) if bytes.saturating_mul(2) <= share => {
                Some(TlcHeap::Sized(bytes * 2))
            }
            _ => None,
        }
    }
}

/// TLC workers per run when `in_flight` runs, this one included, are running or queued on
/// `cores`: the cores divided by the runs that share them, at most 8, and 1 once the runs
/// outnumber the cores. Past 4 to 8 workers a run gains no time and spends 50 to 70% more
/// CPU, and with more runs than cores a JVM per core on one worker each uses the CPU best
/// (#309).
pub(crate) fn tlc_workers(cores: usize, in_flight: usize) -> usize {
    match in_flight > cores {
        true => 1,
        false => (cores / in_flight.max(1)).clamp(1, 8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_core::project::CommandSection;

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
                ..Default::default()
            },
            verify: CommandSection {
                memory: verify.map(size),
                ..Default::default()
            },
            code: None,
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
            (None, Some(&both), Budgeted::Generators, "1GiB"),
            (None, None, Budgeted::Generators, "4GiB"),
        ] {
            let got = memory_limit(flag.map(size), m, command);
            assert_eq!(got.to_string(), want, "{flag:?} {command:?}");
        }
    }

    const MB: u64 = 1 << 20;

    /// 512 MiB per MiB of module, in whole GiB, at least 1 GiB for a cone and 2 GiB for a
    /// whole-model run: every module under 2 MiB keeps today's heap, and #299's 7.6 MB cone
    /// gets 4 GiB.
    #[test]
    fn the_heap_follows_the_module_in_whole_gib_above_the_kind_default() {
        let split = VerifySplit::of(MemorySize::VERIFY, true).unwrap();
        assert_eq!(split.bfs.to_string(), "8GiB");
        let cone = |bytes: u64| TlcHeap::of(bytes, Heap::Cone, split.cones, None);
        assert_eq!(cone(0), TlcHeap::Sized(GIB));
        assert_eq!(cone(94 * 1024), TlcHeap::Sized(GIB));
        assert_eq!(cone(2 * MB), TlcHeap::Sized(GIB));
        assert_eq!(cone(2 * MB + 1), TlcHeap::Sized(2 * GIB));
        assert_eq!(cone(2_800_000), TlcHeap::Sized(2 * GIB));
        assert_eq!(cone(7_600_000), TlcHeap::Sized(4 * GIB));
        assert_eq!(cone(12_600_000), TlcHeap::Sized(7 * GIB));
        assert_eq!(cone(16 * MB), TlcHeap::Sized(8 * GIB));
        let whole = |bytes: u64| TlcHeap::of(bytes, Heap::Temporal, split.wholes, None);
        assert_eq!(whole(50 * 1024), TlcHeap::Sized(2 * GIB));
        assert_eq!(whole(7_600_000), TlcHeap::Sized(4 * GIB));
    }

    /// A module that needs more than the share gets no run; one whose need fits but whose
    /// default does not runs at the share, as before.
    #[test]
    fn a_module_past_the_share_gets_no_run_and_a_small_share_shrinks_the_default() {
        let split = VerifySplit::of(MemorySize::VERIFY, true).unwrap();
        assert_eq!(
            TlcHeap::of(16 * MB + 1, Heap::Cone, split.cones, None),
            TlcHeap::OverShare { need: 9 * GIB }
        );
        let split = VerifySplit::of(size("1GiB"), true).unwrap();
        assert_eq!(
            TlcHeap::of(50 * 1024, Heap::Cone, split.cones, None),
            TlcHeap::Sized(GIB / 2)
        );
        assert_eq!(
            TlcHeap::of(50 * 1024, Heap::Temporal, split.wholes, None),
            TlcHeap::Sized(GIB)
        );
        assert_eq!(
            TlcHeap::of(2 * MB, Heap::Cone, split.cones, None),
            TlcHeap::OverShare { need: GIB }
        );
        let split = VerifySplit::of(size("80MiB"), true).unwrap();
        assert_eq!(split.bfs.to_string(), "16MiB");
        assert_eq!(
            TlcHeap::of(1024, Heap::Cone, split.cones, None),
            TlcHeap::Sized(64 * MIB)
        );
    }

    /// A run over the share says what it needs, what TLC has, and what to change.
    #[test]
    fn a_run_over_the_share_names_the_heap_it_needs_and_the_hint() {
        let (note, report) = TlcHeap::over_share("Cone_Big", 16_800_000, 9 * GIB, 8 * GIB);
        assert_eq!(
            note,
            "not run: needs a 9GiB heap, more than the 8GiB TLC has of --memory"
        );
        assert!(
            report.starts_with("not run: Cone_Big.tla is 16.8 MB, and TLC needs about 512 MiB of heap per MiB of module, so 9GiB; TLC's runs have 8GiB of --memory\nhint: "),
            "{report}"
        );
        assert!(
            report.ends_with("RDRA_TLC_HEAP=9g or JAVA_TOOL_OPTIONS=-Xmx9g"),
            "{report}"
        );
    }

    /// The heap the user names is kept whatever the module and the share, and never doubled.
    #[test]
    fn a_heap_the_user_names_is_kept_and_not_doubled() {
        let split = VerifySplit::of(MemorySize::VERIFY, true).unwrap();
        for bytes in [1024, 7_600_000, 64 * MB] {
            let heap = TlcHeap::of(bytes, Heap::Cone, split.cones, Some(32 * GIB));
            assert_eq!(heap, TlcHeap::User(32 * GIB));
            assert_eq!(heap.doubled(split.cones), None);
        }
    }

    /// A run that runs out of memory reruns once at twice its heap when the share holds it.
    #[test]
    fn a_rerun_doubles_the_heap_within_the_share() {
        let share = 8 * GIB;
        assert_eq!(
            TlcHeap::Sized(GIB).doubled(share),
            Some(TlcHeap::Sized(2 * GIB))
        );
        assert_eq!(
            TlcHeap::Sized(4 * GIB).doubled(share),
            Some(TlcHeap::Sized(8 * GIB))
        );
        assert_eq!(TlcHeap::Sized(5 * GIB).doubled(share), None);
        assert_eq!(TlcHeap::OverShare { need: 9 * GIB }.doubled(share), None);
        assert_eq!(TlcHeap::Sized(GIB).charge(), GIB + 256 * MIB);
        assert_eq!(TlcHeap::OverShare { need: GIB }.charge(), 0);
    }

    /// Under `RDRA_TLC_HEAP=states` a counted run's heap only rises, to whole GiB, within the
    /// share; a heap the user names and a run over the share stay as they are.
    #[test]
    fn a_counted_heap_only_rises_within_the_share() {
        let share = 48 * GIB;
        let sized = TlcHeap::Sized(GIB);
        assert_eq!(sized.with_states(300, share), TlcHeap::Sized(GIB));
        assert_eq!(sized.with_states(2_000_000, share), TlcHeap::Sized(2 * GIB));
        assert_eq!(sized.with_states(6_700_000, share), TlcHeap::Sized(4 * GIB));
        assert_eq!(
            sized.with_states(12_800_000, share),
            TlcHeap::Sized(8 * GIB)
        );
        assert_eq!(
            sized.with_states(12_800_000, 4 * GIB),
            TlcHeap::Sized(4 * GIB)
        );
        assert_eq!(
            TlcHeap::Sized(7 * GIB).with_states(10, share),
            TlcHeap::Sized(7 * GIB)
        );
        assert_eq!(
            TlcHeap::Sized(GIB).with_states(10, GIB / 2),
            TlcHeap::Sized(GIB)
        );
        assert_eq!(
            TlcHeap::User(GIB).with_states(12_800_000, share),
            TlcHeap::User(GIB)
        );
        assert_eq!(
            TlcHeap::OverShare { need: 9 * GIB }.with_states(10, share),
            TlcHeap::OverShare { need: 9 * GIB }
        );
    }

    #[test]
    fn workers_share_the_cores_up_to_8_and_drop_to_1_past_them() {
        assert_eq!(tlc_workers(18, 1), 8);
        assert_eq!(tlc_workers(18, 2), 8);
        assert_eq!(tlc_workers(18, 3), 6);
        assert_eq!(tlc_workers(18, 18), 1);
        assert_eq!(tlc_workers(18, 19), 1);
        assert_eq!(tlc_workers(4, 1), 4);
        assert_eq!(tlc_workers(4, 2), 2);
        assert_eq!(tlc_workers(4, 3), 1);
        assert_eq!(tlc_workers(4, 9), 1);
        assert_eq!(tlc_workers(1, 1), 1);
        assert_eq!(tlc_workers(4, 0), 4);
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
