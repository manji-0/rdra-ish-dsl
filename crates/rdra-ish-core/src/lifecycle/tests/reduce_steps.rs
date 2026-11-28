//! Lemma 6 of docs/theory/cones.md checked step by step (`reduce/lemma6.rs`), on every reduced
//! target and property cone whose unreduced cone completes: every step of the cone is a step
//! of the extended sub-lifecycle or a stutter, rises included, every silent edge from an image
//! state is a step of the cone, and the reduced cone's labelled states and edges are the cone's
//! up to stuttering.

use std::num::NonZeroU8;
use std::path::Path;

use super::cone::load;
use super::*;
use crate::lifecycle::explore::Checks;
use crate::lifecycle::reduce::lemma6::{check, StepCheck};
use crate::lifecycle::{Cone, Seed};
use crate::load_project;

/// The caps of every run here: the cone, the reduction, the extended sub-lifecycle.
const CAPS: Caps = Caps::new(usize::MAX, 200_000);

/// The bounds and scope a line of `verify` flags names: `--instance-count`, `--instances
/// <Entity>=<n>`, `--int-range-max` and `--entity`; the other flags change no cone.
fn bounds_of(model: &SemanticModel, flags: &[String]) -> (Bounds, Scope) {
    let flag = |name: &str| {
        (flags.iter().position(|f| f == name))
            .map(|i| flags[i + 1].parse::<u8>().expect("a number"))
    };
    let mut bounds = Bounds::uniform(
        NonZeroU8::new(flag("--instance-count").unwrap_or(2)).expect("not zero"),
        flag("--int-range-max").unwrap_or(Bounds::VERIFY.int_max),
    );
    let mut entities = Vec::new();
    for (i, f) in flags.iter().enumerate() {
        match f.as_str() {
            "--instances" => {
                let (e, n) = flags[i + 1].split_once('=').expect("<Entity>=<n>");
                let n = NonZeroU8::new(n.parse().expect("a count")).expect("not zero");
                bounds.per_entity.insert(e.to_string(), n);
            }
            "--entity" => entities.push(flags[i + 1].clone()),
            _ => {}
        }
    }
    let scope = Scope::parse(model, &[], &entities).expect("the entities exist");
    (bounds, scope)
}

/// Runs the check on every reduced cone of `model` at `bounds` whose unreduced cone completes
/// under `CAPS`, and panics naming the cone on the first failure. A target cone is reduced
/// under the seeds of every target that shares it, as `explore_targets_reduced` reduces it.
fn check_model(name: &str, model: &SemanticModel, bounds: Bounds, scope: &Scope) -> StepCheck {
    let ts = TransitionSystem::build(model, bounds.clone(), scope).ts;
    let mut cones: Vec<(String, Cone, Vec<Seed>)> = Vec::new();
    for t in ts.targets() {
        let cone = ts.target_cone(t);
        match cones.iter_mut().find(|(_, c, _)| *c == cone) {
            Some((_, _, seeds)) => seeds.push(ts.target_seed(t)),
            None => cones.push((format!("the cone of {t:?}"), cone, vec![ts.target_seed(t)])),
        }
    }
    for i in 0..ts.properties().len() {
        let what = format!("the cone of property {}", ts.properties()[i].name);
        cones.push((what, ts.property_cone(i), vec![ts.property_seed(i)]));
    }
    let mut seen = StepCheck::default();
    for (what, cone, seeds) in cones {
        let sys = ts.reduced_cone(&cone, seeds, CAPS);
        if sys.reduction().is_none() {
            continue;
        }
        let c = ts.restrict_to(&cone.vars, &cone.keep);
        let run = c.clone().run_checking(CAPS.without_views(), Checks::none());
        if !run.complete {
            continue;
        }
        match check(&c, &run, &sys, CAPS) {
            Ok(s) => seen += s,
            Err(e) => panic!("{name} at {bounds:?}, {what}: Lemma 6: {e}"),
        }
    }
    seen
}

/// A corpus case's model and the bounds of its `// flags:` line, or `None` for a case whose
/// model does not load (a `// rejects:` case).
fn corpus_case(path: &Path) -> Option<(SemanticModel, Bounds, Scope)> {
    let src = std::fs::read_to_string(path).expect("a case");
    let (ast, errors) = rdra_ish_syntax::parse(&src);
    if !errors.is_empty() {
        return None;
    }
    let (model, diags) = crate::analysis::build_model(&ast);
    if diags.iter().any(|d| !d.is_warning) {
        return None;
    }
    let flags: Vec<String> = (src.lines())
        .find_map(|l| l.strip_prefix("// flags: "))
        .map(|f| f.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default();
    let (bounds, scope) = bounds_of(&model, &flags);
    Some((model, bounds, scope))
}

fn corpus_dir() -> std::path::PathBuf {
    repo_root().join("crates/rdra-ish-cli/tests/engine_corpus")
}

/// One corpus case at its flags.
fn check_case(case: &str) -> StepCheck {
    let path = corpus_dir().join(format!("{case}.rdra"));
    let (model, bounds, scope) = corpus_case(&path).expect("the case loads");
    check_model(case, &model, bounds, &scope)
}

/// The flags of each `// expect verify` line of a sample root, as verify-samples.sh runs them:
/// once without flags when no line has any or some line has none, and once per flagged line.
fn sample_flags(root: &str) -> Vec<Vec<String>> {
    let project =
        load_project(&[repo_root().join(root)], None).unwrap_or_else(|e| panic!("{root}: {e}"));
    let files = (project.entry_files.iter().chain(&project.include_paths)).filter(|f| f.is_file());
    let mut lines: Vec<Vec<String>> = Vec::new();
    for f in files {
        let src = std::fs::read_to_string(f).expect("a file");
        for l in src.lines() {
            if let Some(rest) = l.strip_prefix("// expect verify") {
                let flags = rest.split_once(':').map_or("", |(f, _)| f);
                let flags: Vec<String> = flags.split_whitespace().map(str::to_string).collect();
                if !lines.contains(&flags) {
                    lines.push(flags);
                }
            }
        }
    }
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    lines
}

/// Every reduced cone of every corpus case at its flags and of every sample root at the flags
/// of its `// expect verify` lines steps as its cone steps (Lemma 6), and the class level
/// holds where it runs. CI runs this in release in formal-verify.
#[test]
fn reduced_cones_step_as_their_cones_step() {
    let mut seen = StepCheck::default();
    let mut paths: Vec<_> = (std::fs::read_dir(corpus_dir()).expect("the corpus"))
        .map(|e| e.expect("an entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "rdra"))
        .collect();
    paths.sort();
    for path in &paths {
        if let Some((model, bounds, scope)) = corpus_case(path) {
            seen += check_model(&path.display().to_string(), &model, bounds, &scope);
        }
    }
    let corpus = seen;
    let roots = std::fs::read_to_string(repo_root().join("samples.txt")).expect("samples.txt");
    for root in roots
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let model = load(root);
        for flags in sample_flags(root) {
            let (bounds, scope) = bounds_of(&model, &flags);
            seen += check_model(root, &model, bounds, &scope);
        }
    }
    eprintln!("lemma 6: corpus {corpus:?}; with the samples {seen:?}");
    assert!(corpus.reductions >= 20, "{corpus:?}");
    assert!(
        seen.rises > 0 && seen.s_steps > 0 && seen.silent_back > 0,
        "{seen:?}"
    );
    assert!(seen.classes > 0 && seen.nested > 0, "{seen:?}");
}

/// The two stub shapes of claim A11: a `Triggered` param beside a fold's key on a stub, and a
/// rise whose `Parent` param is on one. With `bind_stub_params` and the stub check of
/// `reduced_with` both undone, the first fails on an `S_a` step with no silent edge, the second
/// on a `T_a` step with no rise.
#[test]
fn the_stub_cases_step_as_their_cones_step() {
    let triggered = check_case("cone_triggered_param_on_a_stub");
    assert!(
        triggered.s_steps > 0 && triggered.classes > 0,
        "{triggered:?}"
    );
    let rise = check_case("cone_rise_param_on_a_stub");
    assert!(rise.rises > 0 && rise.stages > 0, "{rise:?}");
}

/// A class move removed from a flat reduction's tables is caught at the class level: the
/// reduced cone, run with the planted tables, lacks an edge the cone has.
#[test]
fn a_removed_class_move_fails_the_class_level() {
    let path = corpus_dir().join("cone_triggered_param_on_a_stub.rdra");
    let (model, bounds, scope) = corpus_case(&path).expect("the case loads");
    let ts = TransitionSystem::build(&model, bounds, &scope).ts;
    let mut planted = 0;
    for t in ts.targets() {
        let cone = ts.target_cone(t);
        let sys = ts.reduced_cone(&cone, [ts.target_seed(t)], CAPS);
        let Some(red) = sys.reduction().filter(|r| r.inner().is_none()) else {
            continue;
        };
        let r_run = (sys.clone()).run_checking(CAPS.without_views(), Checks::none());
        // A class the reduced run reaches, with a move.
        let Some(class) = (r_run.states.iter())
            .filter_map(|s| red.class(&sys, &s))
            .find(|c| !red.class_moves()[*c as usize].is_empty())
        else {
            continue;
        };
        let c = ts.restrict_to(&cone.vars, &cone.keep);
        let run = c.clone().run_checking(CAPS.without_views(), Checks::none());
        let seen = check(&c, &run, &sys, CAPS).expect("the tables as built pass");
        assert_eq!(seen.classes, 1, "{t:?}: the class level runs");
        let bad = sys.with_reduction(red.without_class_moves(class));
        let e = check(&c, &run, &bad, CAPS).expect_err("a removed class move is caught");
        assert!(
            e.contains("R has no such edge") || e.contains("label"),
            "{e}"
        );
        planted += 1;
    }
    assert!(planted > 0, "some flat cone has class moves");
}

/// The check on every model in the directory `RDRA_LEMMA6_MODELS` names, each at the bounds of
/// its `// flags:` line: the driver of `scripts/reduction-diff.py lemma6`, which writes its
/// seeds' models there. Prints a line per model, `lemma6 <file>: ok <counts>`, `FAIL <why>` or
/// `load_error`, and fails when some model fails.
#[test]
#[ignore = "run by scripts/reduction-diff.py lemma6"]
fn generated_models_step_as_their_cones_step() {
    let dir = std::env::var_os("RDRA_LEMMA6_MODELS").expect("RDRA_LEMMA6_MODELS names a directory");
    let mut paths: Vec<_> = (std::fs::read_dir(&dir).expect("the models"))
        .map(|e| e.expect("an entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "rdra"))
        .collect();
    paths.sort();
    let mut failed = Vec::new();
    for path in &paths {
        let file = path
            .file_name()
            .expect("a file")
            .to_string_lossy()
            .into_owned();
        let Some((model, bounds, scope)) = corpus_case(path) else {
            println!("lemma6 {file}: load_error");
            continue;
        };
        let checked = std::panic::catch_unwind(|| check_model(&file, &model, bounds, &scope));
        match checked {
            Ok(s) => println!(
                "lemma6 {file}: ok reductions={} nested={} states={} s_steps={} rises={} \
                 silent_back={} classes={} classes_skipped={} stopped={}",
                s.reductions,
                s.nested,
                s.states,
                s.s_steps,
                s.rises,
                s.silent_back,
                s.classes,
                s.classes_skipped,
                s.stopped
            ),
            Err(e) => {
                let why = (e.downcast_ref::<String>().cloned())
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                println!("lemma6 {file}: FAIL {}", why.replace('\n', " "));
                failed.push(file);
            }
        }
    }
    assert!(failed.is_empty(), "Lemma 6 fails on {failed:?}");
}
