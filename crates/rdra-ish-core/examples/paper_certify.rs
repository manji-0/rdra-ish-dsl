//! Reach certificates for the safety cones of a sample (#594, T15; docs/paper/extensions.md).
//!
//! Run: `cargo run --release -p rdra-ish-core --example paper_certify -- PROJECT INSTANCES DIR`
//!
//! For each distinct cone of the sample's invariants and after-asserts, explores the cone
//! unreduced, writes its reach certificate to DIR, reads it back, and checks it with
//! `TransitionSystem::check_reach_certificate`. Then it plants two faults in the certificate of
//! the largest cone, which the check must reject: one state left out (a successor or an initial
//! state goes missing), and one state replaced by a state that breaks a rule, when the cone has
//! a rule that some unreachable state breaks. Prints one tab-separated line per cone: states,
//! moves checked, invariants, certificate bytes, run ms, check ms, verdict.

use std::collections::BTreeMap;
use std::num::NonZeroU8;
use std::path::Path;
use std::time::Instant;

use rdra_ish_core::lifecycle::{
    Bounds, Caps, CertFault, Cone, ReachCertificate, Scope, Target, TransitionSystem,
};
use rdra_ish_core::project::load_project;

fn main() {
    let mut args = std::env::args().skip(1);
    let project = args.next().expect("PROJECT");
    let instances: u8 = args
        .next()
        .map(|s| s.parse().expect("INSTANCES"))
        .unwrap_or(2);
    let dir = args.next().expect("DIR");
    std::fs::create_dir_all(&dir).expect("DIR");
    let model = load_project(&[Path::new(&project).to_path_buf()], None)
        .expect("project loads")
        .model;
    let bounds = Bounds::uniform(NonZeroU8::new(instances).expect("INSTANCES >= 1"), 5);
    let ts = TransitionSystem::build(&model, bounds, &Scope::ALL).ts;
    let mut cones: BTreeMap<Vec<String>, (Cone, String)> = BTreeMap::new();
    for t in ts.targets() {
        let name = match t {
            Target::Invariant(i) => ts.invariants()[i].name.clone(),
            Target::AfterAssert(i) => ts.after_asserts()[i].name.clone(),
            _ => continue,
        };
        let cone = ts.target_cone(t);
        let key: Vec<String> = cone.vars.iter().map(|v| format!("{v:?}")).collect();
        cones.entry(key).or_insert((cone, name));
    }
    println!("project\tcone\tstates\tmoves\tinvariants\tbytes\trun_ms\tcheck_ms\tverdict");
    let mut largest: Option<(usize, TransitionSystem, ReachCertificate)> = None;
    for (cone, name) in cones.values() {
        let system = ts.restrict_to(&cone.vars, &cone.keep);
        let start = Instant::now();
        let mut caps = Caps::new(usize::MAX, 20_000_000);
        caps.views = false;
        let run = system.clone().run(caps);
        let run_ms = start.elapsed().as_millis();
        let cert = match system.reach_certificate(&run) {
            Ok(c) => c,
            Err(e) => {
                println!("{project}\t{name}\t-\t-\t-\t-\t{run_ms}\t-\t{e:?}");
                continue;
            }
        };
        let path = Path::new(&dir).join(format!("{name}.cert"));
        cert.write(std::io::BufWriter::new(
            std::fs::File::create(&path).unwrap(),
        ))
        .unwrap();
        let bytes = std::fs::metadata(&path).unwrap().len();
        let read =
            ReachCertificate::read(std::io::BufReader::new(std::fs::File::open(&path).unwrap()))
                .expect("reads back");
        assert_eq!(read, cert, "the certificate reads back as written");
        let start = Instant::now();
        let verdict = system.check_reach_certificate(&read);
        let check_ms = start.elapsed().as_millis();
        let (moves, invariants) = match &verdict {
            Ok(s) => (s.moves.to_string(), s.invariants.to_string()),
            Err(_) => ("-".into(), "-".into()),
        };
        let shown = match &verdict {
            Ok(_) => "ok".to_string(),
            Err(e) => format!("{e:?}"),
        };
        println!(
            "{project}\t{name}\t{}\t{moves}\t{invariants}\t{bytes}\t{run_ms}\t{check_ms}\t{shown}",
            read.states.len()
        );
        if verdict.is_ok()
            && largest
                .as_ref()
                .is_none_or(|(n, _, _)| read.states.len() > *n)
        {
            largest = Some((read.states.len(), system, read));
        }
    }
    // Planted faults on the largest certified cone.
    if let Some((_, system, cert)) = largest {
        if cert.states.len() > 1 {
            let mut dropped = cert.clone();
            dropped.states.pop();
            let got = system.check_reach_certificate(&dropped);
            assert!(
                matches!(
                    got,
                    Err(CertFault::SuccessorMissing { .. } | CertFault::InitialMissing { .. })
                ),
                "a dropped state must fail the check: {got:?}"
            );
            println!("planted\tdrop one state\t{got:?}");
        }
        // A state with one byte changed is either outside the reachable set, which the check
        // accepts only if its successors are certified too, or breaks a rule. Try each cell
        // value until the check rejects one, and report what it said.
        let mut rejected = None;
        'outer: for c in 0..cert.width {
            for v in 0..=3u8 {
                let mut changed = cert.clone();
                let mut s = changed.states[0].to_vec();
                if s[c] == v {
                    continue;
                }
                s[c] = v;
                changed.states.push(s.into_boxed_slice());
                if let Err(e) = system.check_reach_certificate(&changed) {
                    rejected = Some((c, v, e));
                    break 'outer;
                }
            }
        }
        println!("planted\tadd one changed state\t{rejected:?}");
    }
}
