//! #426 check: writes the full SMT-LIB transcript z3 receives for each root to OUT/<n>.smt2,
//! with the proof's Debug as OUT/<n>.proof, on one solver.
//! Run: cargo run --release -p rdra-ish-core --example prove_transcript -- OUT ROOT...
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rdra_ish_core::lifecycle::prove::{self, ProveOptions, Solver, SolverError, Z3};
use rdra_ish_core::lifecycle::{Bounds, Scope, TransitionSystem};
use rdra_ish_core::project::load_project;

struct Tape {
    z3: Z3,
    out: std::io::BufWriter<std::fs::File>,
}

impl Solver for Tape {
    fn exec(&mut self, commands: &str) -> Result<Vec<String>, SolverError> {
        writeln!(self.out, "{commands}\n;;--").unwrap();
        self.z3.exec(commands)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (out, roots) = args.split_first().expect("OUT ROOT...");
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let z3 = std::env::var_os("RDRA_Z3")
        .map(PathBuf::from)
        .unwrap_or_else(|| "z3".into());
    for root in roots {
        let name = root.replace('/', "_");
        let Ok(project) = load_project(&[Path::new(root).to_path_buf()], None) else {
            println!("{root}: does not load");
            continue;
        };
        let built = TransitionSystem::build(&project.model, Bounds::VERIFY, &Scope::default());
        if rdra_ish_core::lifecycle::has_errors(&built.diags) {
            println!("{root}: build errors, skipped");
            continue;
        }
        let ts = built.ts;
        let twin = prove::mining_twin(&project.model, &Scope::default(), &ts);
        let rules: Vec<usize> = (0..ts.invariants().len()).collect();
        let f = std::fs::File::create(out.join(format!("{name}.smt2"))).unwrap();
        let mut tape = [Tape {
            z3: Z3::start(&z3).unwrap(),
            out: std::io::BufWriter::new(f),
        }];
        let start = Instant::now();
        let proof = prove::prove(
            &ts,
            twin.as_ref(),
            &rules,
            &Default::default(),
            &mut tape,
            ProveOptions::default(),
        );
        let secs = start.elapsed().as_secs_f64();
        std::fs::write(out.join(format!("{name}.proof")), format!("{proof:#?}")).unwrap();
        println!(
            "{root}: {secs:.2} s {}",
            match &proof {
                Ok(p) => format!("{} proved", p.proved()),
                Err(e) => format!("error {e}"),
            }
        );
    }
}
