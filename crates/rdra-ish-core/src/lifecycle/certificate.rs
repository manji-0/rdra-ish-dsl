//! A checkable certificate for a state rule's `ok` (roadmap #594, T15; residual risk 2 of
//! docs/theory/README.md).
//!
//! The certificate is the set `R` of states a complete plain run stored. `check_reach_certificate`
//! checks three facts with the step relation alone, without the search, its store, its frontier or
//! its parents: every initial state is in `R`; every successor of a state of `R` is in `R`; every
//! invariant holds in every state of `R`. Then `Reach ⊆ R`, so every invariant holds on the system.
//! The checker shares the builder and the step relation with the run it checks (residual risk 1),
//! and it covers state rules only, on a system with no sub-lifecycle reduction.

use std::collections::HashSet;
use std::io::{self, Read, Write};

use super::explore::ComponentRun;
use super::expr::{State, StateSlice};
use super::TransitionSystem;

const MAGIC: &[u8] = b"rdra-ish reach certificate v1\n";

/// The reachable set of a complete run: the states as raw bytes, `width` each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReachCertificate {
    pub width: usize,
    pub states: Vec<Box<[u8]>>,
}

/// Why a certificate does not check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertFault {
    /// The run stopped, so its states are not the reachable set.
    Incomplete,
    /// The system is a reduced cone: its class moves are not the step relation of a cone.
    Reduced,
    /// A state's width is not the system's.
    Width { expected: usize, found: usize },
    /// An initial state of the system is not in the certificate.
    InitialMissing { initial: usize },
    /// A successor of a certified state is not in the certificate.
    SuccessorMissing { from: usize, action: String },
    /// An invariant fails on a certified state.
    Violated { invariant: String, state: usize },
    /// The file is not a certificate.
    Format(String),
}

/// What a check covered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CertStats {
    pub states: usize,
    pub initial: usize,
    pub moves: u64,
    pub invariants: usize,
}

impl ReachCertificate {
    /// Writes the certificate: a magic line, the width and the count, then the states.
    pub fn write(&self, mut w: impl Write) -> io::Result<()> {
        w.write_all(MAGIC)?;
        w.write_all(&(self.width as u64).to_le_bytes())?;
        w.write_all(&(self.states.len() as u64).to_le_bytes())?;
        for s in &self.states {
            w.write_all(s)?;
        }
        Ok(())
    }

    /// Reads a certificate `write` wrote.
    pub fn read(mut r: impl Read) -> Result<ReachCertificate, CertFault> {
        let bad = |e: io::Error| CertFault::Format(e.to_string());
        let mut magic = vec![0; MAGIC.len()];
        r.read_exact(&mut magic).map_err(bad)?;
        if magic != MAGIC {
            return Err(CertFault::Format("not a reach certificate".into()));
        }
        let mut word = [0u8; 8];
        r.read_exact(&mut word).map_err(bad)?;
        let width = usize::try_from(u64::from_le_bytes(word))
            .map_err(|_| CertFault::Format("width".into()))?;
        r.read_exact(&mut word).map_err(bad)?;
        let count = usize::try_from(u64::from_le_bytes(word))
            .map_err(|_| CertFault::Format("count".into()))?;
        let mut states = Vec::with_capacity(count);
        for _ in 0..count {
            let mut s = vec![0; width].into_boxed_slice();
            r.read_exact(&mut s).map_err(bad)?;
            states.push(s);
        }
        Ok(ReachCertificate { width, states })
    }
}

impl TransitionSystem {
    /// The certificate of a run of this system: every state it stored.
    pub fn reach_certificate(&self, run: &ComponentRun) -> Result<ReachCertificate, CertFault> {
        if !run.complete {
            return Err(CertFault::Incomplete);
        }
        let width = run.states.width();
        let states = (0..run.states.len())
            .map(|i| Box::<[u8]>::from(&run.states.at(i).0))
            .collect();
        Ok(ReachCertificate { width, states })
    }

    /// Checks `cert` against this system's initial states, step relation and invariants.
    pub fn check_reach_certificate(&self, cert: &ReachCertificate) -> Result<CertStats, CertFault> {
        if self.reduction().is_some() {
            return Err(CertFault::Reduced);
        }
        let set: HashSet<&[u8]> = cert.states.iter().map(|s| &s[..]).collect();
        let mut stats = CertStats {
            states: cert.states.len(),
            invariants: self.invariants().len(),
            ..CertStats::default()
        };
        for (i, s) in self.initial_states().iter().enumerate() {
            if s.0.len() != cert.width {
                return Err(CertFault::Width {
                    expected: s.0.len(),
                    found: cert.width,
                });
            }
            if !set.contains(&s.0[..]) {
                return Err(CertFault::InitialMissing { initial: i });
            }
            stats.initial += 1;
        }
        for (i, bytes) in cert.states.iter().enumerate() {
            let s = StateSlice::new(bytes);
            for m in self.moves(s) {
                stats.moves += 1;
                let next: &State = &m.next;
                if !set.contains(&next.0[..]) {
                    return Err(CertFault::SuccessorMissing {
                        from: i,
                        action: self.actions()[m.act.index()].name.clone(),
                    });
                }
            }
            for inv in self.invariants() {
                if self
                    .find_violation(&inv.binders, &inv.body, s, &[])
                    .is_some()
                {
                    return Err(CertFault::Violated {
                        invariant: inv.name.clone(),
                        state: i,
                    });
                }
            }
        }
        Ok(stats)
    }
}
