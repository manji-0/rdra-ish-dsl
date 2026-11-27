//! A reference interpreter for the builder (open problem P26 of docs/theory/README.md).
//!
//! The native checker and TLC both read the one `TransitionSystem` the builder makes, so a
//! fault in the builder moves both and TLC agreement cannot see it (P40 and P54 were such
//! faults: a value past a byte wrapped in `T`). `interp.rs` reads a subset of the language
//! from the `SemanticModel` on its own and explores it by brute force; this harness compares
//! what it finds with the native checker's run of the whole model (no cone, no reduction, no
//! symmetry): the initial states, the reachable states, the labelled edges, the rules the
//! system holds, and which of them some reachable state breaks.
//!
//! The comparison decodes each native state through `Domain`'s documented encoding (null
//! first, then the declared values, an Int `k` as `k` or `k + 1` past a null), never through
//! the builder's tables. A model outside the subset, or one the builder refuses, is skipped
//! with its reason, and each test asserts how many systems it compared, so that a subset
//! that shrinks cannot pass by skipping.

mod gen;
mod interp;
mod proofs;

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU8;

use super::{repo_root, Bounds, Caps, Scope, StateDiag, TransitionSystem};
use crate::analysis::build_model;
use crate::lifecycle::diag::Severity;
use crate::lifecycle::expr::StateSlice;
use crate::lifecycle::types::{Domain, VarKind};
use crate::model::SemanticModel;
use interp::{Ex, RState, RefModel, Row, Val};
use rdra_ish_syntax::parse;

/// The largest system either side explores here.
const MOST_STATES: usize = 20_000;

/// The model `src` builds, if it parses and builds without an error.
fn try_model(src: &str) -> Option<SemanticModel> {
    let (ast, errors) = parse(src);
    if !errors.is_empty() {
        return None;
    }
    let (model, diags) = build_model(&ast);
    diags.iter().all(|d| d.is_warning).then_some(model)
}

/// What one comparison found.
enum Outcome {
    Skipped(String),
    Compared {
        states: usize,
        edges: usize,
        rules: usize,
        violated: usize,
    },
    Differs(Vec<String>),
}

/// The value `domain` stores as byte `v`, read as `Domain` documents its encoding.
fn decode(domain: &Domain, v: u8) -> Val {
    let null = domain.has_null() && v == 0;
    match domain {
        _ if null => Val::Null,
        Domain::Enum(names) | Domain::NullableEnum(names) => {
            let i = usize::from(v) - usize::from(domain.has_null());
            Val::Enum(names[i].clone())
        }
        Domain::Bool | Domain::NullableBool => {
            Val::Bool(usize::from(v) - usize::from(domain.has_null()) == 1)
        }
        Domain::Nullable => Val::Present,
        Domain::Int | Domain::NullableInt => Val::Int(i64::from(v) - i64::from(domain.has_null())),
        Domain::Existence | Domain::Instance { .. } => unreachable!("not a column"),
    }
}

/// Reads the native system's states in the reference's terms.
struct Decoder<'a> {
    ts: &'a TransitionSystem,
    /// Per reference entity: the native entity, and per column its variable, if it has one.
    map: Vec<(
        crate::lifecycle::EntId,
        Vec<Option<crate::lifecycle::types::VarId>>,
    )>,
}

impl<'a> Decoder<'a> {
    fn new(ts: &'a TransitionSystem, r: &RefModel) -> Result<Self, String> {
        let mut map = Vec::new();
        for e in &r.ents {
            let Some(id) = ts.entity_ids().find(|x| ts.entity(*x).id == e.name) else {
                return Err(format!("the native system has no entity {}", e.name));
            };
            if ts.slots(id) != e.slots {
                return Err(format!(
                    "{}: {} native slots, {} wanted",
                    e.name,
                    ts.slots(id),
                    e.slots
                ));
            }
            let cols = (e.cols.iter())
                .map(|c| {
                    ts.var_ids().find(|v| {
                        matches!(&ts.var(*v).kind,
                            VarKind::Column { entity, column, .. } if *entity == id && *column == c.name)
                    })
                })
                .collect();
            map.push((id, cols));
        }
        if ts.entity_ids().count() != r.ents.len() {
            return Err("the native system has entities the model does not".into());
        }
        for v in ts.var_ids() {
            if !matches!(ts.var(v).kind, VarKind::Exists(_) | VarKind::Column { .. }) {
                return Err(format!(
                    "the native system has the variable {}",
                    ts.var_name(v)
                ));
            }
        }
        Ok(Decoder { ts, map })
    }

    /// A column the native system has no variable for keeps the reference's base value.
    fn state(&self, r: &RefModel, s: &StateSlice) -> RState {
        let ts = self.ts;
        (r.ents.iter().zip(&self.map))
            .map(|(e, (id, cols))| {
                (0..e.slots)
                    .map(|k| {
                        let ex = match s.get(ts, ts.entity(*id).exists, k).0 {
                            0 => Ex::Absent,
                            1 => Ex::Present,
                            _ => Ex::Deleted,
                        };
                        let vals = (e.cols.iter().zip(cols))
                            .map(|(c, var)| match var {
                                Some(v) => decode(&ts.var(*v).domain, s.get(ts, *v, k).0),
                                None => c.base.clone(),
                            })
                            .collect();
                        Row { ex, vals }
                    })
                    .collect()
            })
            .collect()
    }
}

/// Up to three members of `a` not in `b`, shown.
fn missing(r: &RefModel, a: &BTreeSet<RState>, b: &BTreeSet<RState>) -> String {
    let xs: Vec<String> = a.difference(b).take(3).map(|s| r.show(s)).collect();
    xs.join(" | ")
}

fn compare(model: &SemanticModel, bounds: &Bounds) -> Outcome {
    let int_max = i64::from(bounds.int_max);
    let slots = |id: &str| usize::from(bounds.of(id).get());
    let r = match RefModel::read(model, slots, int_max) {
        Ok(r) => r,
        Err(why) => return Outcome::Skipped(why),
    };
    let built = TransitionSystem::build(model, bounds.clone(), &Scope::ALL);
    if let Some(d) = built
        .diags
        .iter()
        .find(|d| d.diag.severity() == Severity::Error)
    {
        return Outcome::Skipped(format!("the build refuses it: {}", d.diag.name(&[])));
    }
    let ts = built.ts;
    if ts.now().is_some() {
        return Outcome::Skipped("now".into());
    }
    let Some(want) = r.explore(MOST_STATES) else {
        return Outcome::Skipped(format!("over {MOST_STATES} states"));
    };
    let dec = match Decoder::new(&ts, &r) {
        Ok(d) => d,
        Err(why) => return Outcome::Differs(vec![why]),
    };
    let mut wrong = Vec::new();
    let initial: BTreeSet<RState> = (ts.initial_states().iter())
        .map(|s| dec.state(&r, s))
        .collect();
    if initial != want.initial {
        wrong.push(format!(
            "initial states: native {}, reference {}",
            missing(&r, &initial, &want.initial),
            missing(&r, &want.initial, &initial)
        ));
    }
    let run = ts.clone().run(Caps::new(usize::MAX, MOST_STATES + 1));
    if !run.complete {
        return Outcome::Differs(vec![format!(
            "the native run stopped; the reference reached {} states",
            want.states.len()
        )]);
    }
    let graph = run.graph();
    let decoded: Vec<RState> = (0..graph.states.len())
        .map(|i| dec.state(&r, &graph.states.at(i)))
        .collect();
    let got: BTreeSet<RState> = decoded.iter().cloned().collect();
    if got.len() != decoded.len() {
        wrong.push(format!(
            "{} native states decode to {} distinct states",
            decoded.len(),
            got.len()
        ));
    }
    if got != want.states {
        wrong.push(format!(
            "states: {} native, {} reference; only native: {}; only reference: {}",
            got.len(),
            want.states.len(),
            missing(&r, &got, &want.states),
            missing(&r, &want.states, &got)
        ));
    }
    let edges: BTreeSet<(RState, String, RState)> = (graph.edges())
        .map(|(from, to, act)| {
            let name = ts.actions()[act.index()].name.clone();
            (
                decoded[from as usize].clone(),
                name,
                decoded[to as usize].clone(),
            )
        })
        .collect();
    if edges != want.edges {
        let show =
            |x: &(RState, String, RState)| format!("{} -{}-> {}", r.show(&x.0), x.1, r.show(&x.2));
        let only_native: Vec<String> = edges.difference(&want.edges).take(2).map(show).collect();
        let only_ref: Vec<String> = want.edges.difference(&edges).take(2).map(show).collect();
        wrong.push(format!(
            "edges: {} native, {} reference; only native: {}; only reference: {}",
            edges.len(),
            want.edges.len(),
            only_native.join(" | "),
            only_ref.join(" | ")
        ));
    }
    let names: BTreeSet<String> = ts.invariants().iter().map(|i| i.name.clone()).collect();
    let rule_names: BTreeSet<String> = r.rules.iter().map(|x| x.name.clone()).collect();
    if names != rule_names {
        wrong.push(format!("rules: native {names:?}, reference {rule_names:?}"));
    }
    let violated: BTreeSet<String> = (run.diags.iter())
        .filter_map(|d| match &d.diag {
            StateDiag::ConstraintViolated { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    if violated != want.violated {
        wrong.push(format!(
            "violated: native {violated:?}, reference {:?}",
            want.violated
        ));
    }
    if !wrong.is_empty() {
        return Outcome::Differs(wrong);
    }
    Outcome::Compared {
        states: want.states.len(),
        edges: want.edges.len(),
        rules: r.rules.len(),
        violated: want.violated.len(),
    }
}

/// What a sweep compared, skipped, and found different.
#[derive(Default)]
struct Tally {
    systems: usize,
    states: usize,
    edges: usize,
    rules: usize,
    violated: usize,
    /// Per reason, how many were skipped and the first of them.
    skipped: BTreeMap<String, (usize, String)>,
    wrong: Vec<String>,
}

impl Tally {
    fn add(&mut self, name: &str, bounds: &Bounds, outcome: Outcome) {
        match outcome {
            Outcome::Skipped(why) => self.skip(why, name),
            Outcome::Compared {
                states,
                edges,
                rules,
                violated,
            } => {
                self.systems += 1;
                self.states += states;
                self.edges += edges;
                self.rules += rules;
                self.violated += violated;
            }
            Outcome::Differs(ws) => {
                let at = format!(
                    "{name} at {} instances{}, int max {}",
                    bounds.instances,
                    bounds.per_entity_note(),
                    bounds.int_max
                );
                self.wrong
                    .extend(ws.into_iter().map(|w| format!("{at}: {w}")));
            }
        }
    }

    fn skip(&mut self, why: String, name: &str) {
        let entry = self.skipped.entry(why).or_insert((0, name.to_string()));
        entry.0 += 1;
    }

    fn report(&self, what: &str) {
        eprintln!(
            "{what}: {} systems compared ({} states, {} edges, {} rules, {} violated)",
            self.systems, self.states, self.edges, self.rules, self.violated
        );
        for (why, (n, first)) in &self.skipped {
            eprintln!("  skipped {n}: {why} (first: {first})");
        }
        assert!(
            self.wrong.is_empty(),
            "{what}: {} disagreement(s) with the reference interpreter:\n{}",
            self.wrong.len(),
            self.wrong.join("\n")
        );
    }
}

fn bounds(n: u8, int_max: u8) -> Bounds {
    Bounds::uniform(NonZeroU8::new(n).expect("n > 0"), int_max)
}

/// Every corpus case and formal-verification sample.
fn corpus() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for dir in [
        "crates/rdra-ish-cli/tests/engine_corpus",
        "samples/formal-verification",
    ] {
        let mut paths: Vec<_> = std::fs::read_dir(repo_root().join(dir))
            .expect("a model directory")
            .map(|entry| entry.expect("an entry").path())
            .filter(|p| p.extension().is_some_and(|x| x == "rdra"))
            .collect();
        paths.sort();
        for path in paths {
            let src = std::fs::read_to_string(&path).expect("a model");
            let name = path
                .file_name()
                .expect("a name")
                .to_string_lossy()
                .into_owned();
            out.push((name, src));
        }
    }
    out
}

/// The builder's system equals the reference's on every corpus case and formal-verification
/// sample in the subset, at one and two instances and Int ranges 0..=2 and 0..=5.
#[test]
fn the_builder_agrees_with_the_reference_on_the_corpus() {
    let mut tally = Tally::default();
    for (name, src) in corpus() {
        let Some(model) = try_model(&src) else {
            tally.skip("does not build".into(), &name);
            continue;
        };
        for b in [bounds(1, 5), bounds(2, 5), bounds(2, 2)] {
            let outcome = compare(&model, &b);
            tally.add(&name, &b, outcome);
        }
    }
    tally.report("corpus");
    assert!(
        tally.systems >= 20,
        "only {} systems compared",
        tally.systems
    );
    assert!(
        tally.violated >= 5,
        "only {} violated rules",
        tally.violated
    );
}

/// The builder's system equals the reference's on random models of the subset (`gen.rs`):
/// the seeds a pull request runs.
#[test]
fn the_builder_agrees_with_the_reference_on_random_models() {
    sweep(0..300, "random 0..300");
}

/// As above on more seeds, on main only (.config/nextest.toml).
#[test]
fn the_builder_agrees_with_the_reference_on_many_random_models() {
    sweep(300..3_000, "random 300..3000");
}

fn sweep(seeds: std::ops::Range<u64>, what: &str) {
    let mut tally = Tally::default();
    let mut invalid = Vec::new();
    let count = seeds.end - seeds.start;
    for seed in seeds {
        let (src, b) = gen::model(seed);
        if std::env::var("RDRA_REF_SEED").is_ok_and(|s| s == seed.to_string()) {
            eprintln!("seed {seed}:\n{src}");
        }
        let Some(model) = try_model(&src) else {
            invalid.push(seed);
            continue;
        };
        let outcome = compare(&model, &b);
        tally.add(&format!("seed {seed}"), &b, outcome);
        if !tally.wrong.is_empty() {
            eprintln!("seed {seed}:\n{src}");
            break;
        }
    }
    tally.report(what);
    assert!(
        invalid.is_empty(),
        "the generator wrote models that do not build: seeds {invalid:?}"
    );
    assert!(
        tally.systems * 10 >= count as usize * 9,
        "{what}: only {} of {count} compared",
        tally.systems
    );
    assert!(
        tally.violated * 5 >= tally.systems,
        "{what}: few violated rules"
    );
}

/// The wraps of P40 and P54: a nullable Int written `present` at `--int-range-max 255`
/// takes 256 values beside null, and a nullable Enum of 256 values has 257. A build that
/// accepts either must reach the top value as itself, not as null; at 254 values (where no
/// byte wraps) the comparison must run and agree.
#[test]
fn a_value_past_a_byte_is_not_wrapped() {
    let int = "entity Item \"item\" {\n  id: Int @pk\n  n: Int @null\n}\n\
               usecase Make \"make\"\nusecase Fill \"fill\"\n\
               creates(Make, Item)\nsets(Fill, Item.n == present)\n\
               forbidden(Item, n > 253)\n";
    let values: Vec<String> = (0..256).map(|i| format!("v{i}")).collect();
    let wide_enum = |k: usize| {
        format!(
            "entity Item \"item\" {{\n  id: Int @pk\n  e: Enum({}) @null\n}}\n\
             usecase Make \"make\"\nusecase Fill \"fill\"\n\
             creates(Make, Item)\nsets(Fill, Item.e == v{})\n\
             forbidden(Item, e == v{})\n",
            values[..k].join(", "),
            k - 1,
            k - 1
        )
    };
    let mut tally = Tally::default();
    let cases = [
        ("nullable Int", int.to_string(), 254, true),
        ("nullable Int", int.to_string(), 255, false),
        ("nullable Enum of 255", wide_enum(255), 5, true),
        ("nullable Enum of 256", wide_enum(256), 5, false),
    ];
    for (name, src, int_max, must_compare) in cases {
        let model = try_model(&src).expect("the model builds");
        let b = bounds(1, int_max);
        let outcome = compare(&model, &b);
        if must_compare {
            assert!(
                matches!(outcome, Outcome::Compared { violated: 1, .. }),
                "{name} at {int_max}: not compared, or the rule held"
            );
        }
        tally.add(name, &b, outcome);
    }
    tally.report("a value past a byte");
}
