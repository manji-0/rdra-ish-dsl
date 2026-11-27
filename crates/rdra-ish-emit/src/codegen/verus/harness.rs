//! `harness.rs`: the differential test of the spec against the BFS. It reads a dump of every
//! state and move BFS found (`rdra-ish`'s test writes it), and, for every state and every call
//! on its slots, runs the verified port over the in-memory store and compares the rows it
//! leaves with the BFS successor (or with the state, when BFS has no such move). Verus proved
//! the port moves as the spec's `next`, so a difference is a difference between the spec (or
//! the store) and rdra-ish's semantics. Not verified: plain Rust.
//!
//! The dump, one line each:
//! - `N <Entity> <slots>`
//! - `S <state> <Entity>:<slot>:<exists>:<column>=<value>;..;@<Parent>=<slot>` (one field per row)
//! - `E <from> <to> <Action> <slot per param>`

use std::fmt::Write as _;
use std::num::NonZeroU8;

use rdra_ish_core::lifecycle::{Bounds, Caps, ParamRole, Scope, TransitionSystem};
use rdra_ish_core::SemanticModel;

use super::model::act_name;
use super::{Ctx, MTy};
use crate::codegen::ir::DomainModel;
use crate::codegen::ir::FieldType;
use crate::codegen::rust::ident;
use crate::codegen::string_literal;

pub(super) fn print(ctx: &Ctx) -> String {
    let mut seed = String::new();
    let mut observe = String::new();
    for e in &ctx.ents {
        let fields = match ctx.domain.sums.iter().find(|s| s.entity == e.id) {
            Some(s) => s.row_fields.clone(),
            None => ctx
                .domain
                .records
                .iter()
                .find(|r| r.id == e.id)
                .map(|r| r.fields.clone())
                .unwrap_or_default(),
        };
        let mut inits = Vec::new();
        let mut obs = Vec::new();
        for f in &fields {
            let tracked = e.fields.iter().find(|m| m.name == f.name);
            let value = match (tracked.map(|m| &m.ty), &f.ty) {
                (_, FieldType::Id(t)) if f.wire == "id" => format!("{t}(*id)"),
                (Some(MTy::Key { parent }), FieldType::Id(t)) => {
                    let p = ctx.ent(*parent);
                    obs.push(format!(
                        "format!(\"@{}={{}}\", r.{}.0)",
                        p.id,
                        ident(&f.name)
                    ));
                    format!("{t}(c[\"@{}\"].parse().unwrap())", p.id)
                }
                (Some(MTy::Enum { .. }), FieldType::Enum(t)) => {
                    obs.push(format!(
                        "format!(\"{}={{}}\", r.{}.as_str())",
                        f.wire,
                        ident(&f.name)
                    ));
                    format!("{t}::parse(&c[{}]).unwrap()", string_literal(&f.wire))
                }
                (Some(MTy::Bool), _) => {
                    obs.push(format!(
                        "format!(\"{}={{}}\", r.{})",
                        f.wire,
                        ident(&f.name)
                    ));
                    format!("c[{}] == \"true\"", string_literal(&f.wire))
                }
                // A column the spec does not track: any value.
                (None, FieldType::Enum(t)) => {
                    let first = ctx
                        .domain
                        .enums
                        .iter()
                        .find(|x| x.name == *t)
                        .map(|x| x.values[0].ident.clone())
                        .unwrap_or_default();
                    format!("{t}::{first}")
                }
                (None, FieldType::Id(t)) => format!("{t}(0)"),
                _ => "false".to_string(),
            };
            inits.push(format!("{}: {value}", ident(&f.name)));
        }
        seed.push_str(&format!(
            "            {} => s.{}.push({} {{ {} }}),\n",
            string_literal(&e.id),
            e.snake,
            e.row,
            inits.join(", ")
        ));
        obs.sort();
        observe.push_str(&format!(
            "    for r in &store.{} {{\n        let mut f: Vec<String> = vec![{}];\n        f.sort();\n        out.push(format!(\"{}:{{}}:{{}}\", r.id.0, f.join(\";\")));\n    }}\n",
            e.snake,
            obs.join(", "),
            e.id
        ));
    }
    let mut calls = String::new();
    for a in ctx.ts.actions() {
        let ents: Vec<String> = a
            .params
            .iter()
            .map(|p| string_literal(&ctx.ent(p.entity).id))
            .collect();
        let news: Vec<String> = a
            .params
            .iter()
            .map(|p| (p.role == ParamRole::New).to_string())
            .collect();
        let args: Vec<String> = a
            .params
            .iter()
            .enumerate()
            .map(|(k, p)| format!("{}(slots[{k}])", ctx.ent(p.entity).id_ty))
            .collect();
        calls.push_str(&format!(
            "        Call {{ name: {}, ents: vec![{}], new: vec![{}], run: Box::new(|st: &mut Store, slots: &[i64]| port_{}::<Core>(st, {}).is_ok()) }},\n",
            string_literal(&a.name),
            ents.join(", "),
            news.join(", "),
            act_name(a),
            args.join(", ")
        ));
    }
    TEMPLATE
        .replace("{seed}", &seed)
        .replace("{observe}", &observe)
        .replace("{calls}", &calls)
}

/// The dump `harness.rs` reads: every state and move BFS finds at `instances` rows per
/// entity. `Err` when the model is outside `--verus`, or the BFS stopped at a cap.
pub fn bfs_dump(
    model: &SemanticModel,
    domain: &DomainModel,
    instances: NonZeroU8,
) -> Result<String, Vec<String>> {
    let built = TransitionSystem::build(model, Bounds::uniform(instances, 5), &Scope::ALL);
    let run = built.ts.clone().run(Caps::new(
        Caps::for_instances(u32::from(instances.get())).patterns,
        1 << 22,
    ));
    if !run.complete {
        return Err(vec!["the BFS stopped at a cap".to_string()]);
    }
    let ts = &run.system;
    let ctx = Ctx::new(model, ts, domain)?;
    let mut out = String::new();
    for e in &ctx.ents {
        let _ = writeln!(out, "N {} {}", e.id, ts.slots(e.eid));
    }
    // Per entity: its exists var, and per tracked field its key in the dump.
    let fields: Vec<(&super::Ent, Vec<(&super::MField, String)>)> = ctx
        .ents
        .iter()
        .map(|e| {
            let row = row_fields(&ctx, &e.id);
            let fs = e
                .fields
                .iter()
                .map(|f| {
                    let key = match &f.ty {
                        MTy::Key { parent } => format!("@{}", ctx.ent(*parent).id),
                        _ => row
                            .iter()
                            .find(|x| x.name == f.name)
                            .map(|x| x.wire.clone())
                            .unwrap_or_else(|| f.name.clone()),
                    };
                    (f, key)
                })
                .collect();
            (e, fs)
        })
        .collect();
    for i in 0..run.states.len() {
        let s = run.states.at(i);
        let _ = write!(out, "S {i}");
        for (e, fs) in &fields {
            let exists = ts.entity(e.eid).exists;
            for slot in 0..ts.slots(e.eid) {
                let x = s.get(ts, exists, slot).0;
                let cols: Vec<String> = fs
                    .iter()
                    .map(|(f, key)| {
                        let v = s.get(ts, f.var, slot);
                        let text = match &f.ty {
                            MTy::Enum { .. } => {
                                ts.var(f.var).domain.name(v).unwrap_or("?").to_string()
                            }
                            MTy::Bool => (v.0 != 0).to_string(),
                            MTy::Key { .. } => v.0.to_string(),
                        };
                        format!("{key}={text}")
                    })
                    .collect();
                let _ = write!(out, " {}:{slot}:{x}:{}", e.id, cols.join(";"));
            }
        }
        out.push('\n');
    }
    for edge in run.edges() {
        let (act, params) = run.step(edge);
        let action = &ts.actions()[act.index()];
        let _ = write!(out, "E {} {} {}", edge.from, edge.to, action.name);
        for p in params.iter().take(action.params.len()) {
            let _ = write!(out, " {p}");
        }
        out.push('\n');
    }
    Ok(out)
}

fn row_fields(ctx: &Ctx, entity: &str) -> Vec<crate::codegen::ir::Field> {
    match ctx.domain.sums.iter().find(|s| s.entity == entity) {
        Some(s) => s.row_fields.clone(),
        None => ctx
            .domain
            .records
            .iter()
            .find(|r| r.id == entity)
            .map(|r| r.fields.clone())
            .unwrap_or_default(),
    }
}

const TEMPLATE: &str = r#"//! The differential harness (not verified): `harness <dump>` exits 1 on a mismatch.

use std::collections::{BTreeMap, HashMap};

use crate::core::Core;
use crate::port::*;
use crate::types::*;

struct Call {
    name: &'static str,
    ents: Vec<&'static str>,
    new: Vec<bool>,
    run: Box<dyn Fn(&mut Store, &[i64]) -> bool>,
}

#[derive(Default, Clone)]
struct St {
    /// `(entity, slot)` to the exists value: 0 absent, 1 present, 2 deleted.
    exists: BTreeMap<(String, i64), u8>,
    cols: BTreeMap<(String, i64), HashMap<String, String>>,
}

fn calls() -> Vec<Call> {
    vec![
{calls}    ]
}

fn seed(st: &St) -> Store {
    let mut s = Store::new();
    for ((e, id), ex) in &st.exists {
        if *ex != 1 {
            continue;
        }
        let c = &st.cols[&(e.clone(), *id)];
        match e.as_str() {
{seed}            _ => panic!("no entity {e}"),
        }
    }
    s
}

fn observe(store: &Store) -> Vec<String> {
    let mut out = Vec::new();
{observe}    out.sort();
    out
}

fn expect(st: &St) -> Vec<String> {
    let mut out = Vec::new();
    for ((e, id), ex) in &st.exists {
        if *ex != 1 {
            continue;
        }
        let mut f: Vec<String> = st.cols[&(e.clone(), *id)]
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        f.sort();
        out.push(format!("{e}:{id}:{}", f.join(";")));
    }
    out.sort();
    out
}

pub fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("usage: harness <dump>");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(path).expect("reads the dump");
    if !run(&text) {
        std::process::exit(1);
    }
}

fn run(text: &str) -> bool {
    let mut slots: HashMap<String, i64> = HashMap::new();
    let mut states: Vec<St> = Vec::new();
    let mut edges: HashMap<(usize, String), usize> = HashMap::new();
    for line in text.lines() {
        let w: Vec<&str> = line.split(' ').collect();
        match w[0] {
            "N" => {
                slots.insert(w[1].to_string(), w[2].parse().unwrap());
            }
            "S" => {
                let mut st = St::default();
                for f in &w[2..] {
                    let p: Vec<&str> = f.splitn(4, ':').collect();
                    let key = (p[0].to_string(), p[1].parse::<i64>().unwrap());
                    st.exists.insert(key.clone(), p[2].parse().unwrap());
                    let cols = p
                        .get(3)
                        .filter(|s| !s.is_empty())
                        .map(|s| {
                            s.split(';')
                                .map(|kv| {
                                    let (k, v) = kv.split_once('=').unwrap();
                                    (k.to_string(), v.to_string())
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    st.cols.insert(key, cols);
                }
                states.push(st);
            }
            "E" => {
                let from: usize = w[1].parse().unwrap();
                let to: usize = w[2].parse().unwrap();
                edges.insert((from, w[3..].join(" ")), to);
            }
            _ => {}
        }
    }
    let calls = calls();
    let (mut agree, mut bad, mut runs, mut refused) = (0u64, 0u64, 0u64, 0u64);
    // Creates only the spec makes: [into a deleted slot, into an absent slot that is not the
    // lowest, with no absent slot left]. BFS creates into the lowest absent slot and never
    // reuses a deleted one; the spec takes any absent id.
    let mut extra = [0u64; 3];
    for (i, st) in states.iter().enumerate() {
        for c in &calls {
            let n: Vec<i64> = c.ents.iter().map(|e| slots[*e]).collect();
            let mut tuple = vec![0i64; n.len()];
            loop {
                let key = std::iter::once(c.name.to_string())
                    .chain(tuple.iter().map(|s| s.to_string()))
                    .collect::<Vec<_>>()
                    .join(" ");
                let mut store = seed(st);
                let ran = (c.run)(&mut store, &tuple);
                let got = observe(&store);
                match (ran, edges.get(&(i, key.clone()))) {
                    (true, Some(&to)) if got == expect(&states[to]) => {
                        agree += 1;
                        runs += 1;
                    }
                    (false, None) if got == expect(st) => {
                        agree += 1;
                        refused += 1;
                    }
                    (true, None) => {
                        // The same call with each created row at the slot BFS picks.
                        let mut twin = tuple.clone();
                        let mut class = None;
                        let mut present = false;
                        for (k, new) in c.new.iter().enumerate() {
                            if !new {
                                continue;
                            }
                            let ex = |s: i64| st.exists.get(&(c.ents[k].to_string(), s)).copied().unwrap_or(0);
                            present |= ex(tuple[k]) == 1;
                            let lowest = (0..n[k]).find(|s| ex(*s) == 0);
                            match lowest {
                                None => class = Some(2),
                                Some(lo) if lo != tuple[k] => {
                                    class = class.or(Some(if ex(tuple[k]) == 2 { 0 } else { 1 }));
                                    twin[k] = lo;
                                }
                                Some(_) => {}
                            }
                        }
                        let twin_key = std::iter::once(c.name.to_string())
                            .chain(twin.iter().map(|s| s.to_string()))
                            .collect::<Vec<_>>()
                            .join(" ");
                        match class {
                            _ if present => {
                                bad += 1;
                                if bad <= 10 {
                                    println!("MISMATCH state {i}: {key}: the port created a row over a present one");
                                }
                            }
                            Some(2) => extra[2] += 1,
                            Some(k) if edges.contains_key(&(i, twin_key)) => extra[k] += 1,
                            _ => {
                                bad += 1;
                                if bad <= 10 {
                                    println!("MISMATCH state {i}: {key}: the port ran, BFS refuses");
                                }
                            }
                        }
                    }
                    (false, Some(_)) => {
                        bad += 1;
                        if bad <= 10 {
                            println!("MISMATCH state {i}: {key}: the port refused, BFS runs");
                        }
                    }
                    _ => {
                        bad += 1;
                        if bad <= 10 {
                            println!("MISMATCH state {i}: {key}: another successor");
                        }
                    }
                }
                // The next tuple.
                let mut k = 0;
                loop {
                    if k == tuple.len() {
                        break;
                    }
                    tuple[k] += 1;
                    if tuple[k] < n[k] {
                        break;
                    }
                    tuple[k] = 0;
                    k += 1;
                }
                if k == tuple.len() {
                    break;
                }
            }
        }
    }
    println!(
        "states {} calls {} agree {agree} (ran {runs}, refused {refused}) spec-only creates [deleted slot {}, not the lowest slot {}, no free slot {}] mismatch {bad}",
        states.len(),
        agree + extra.iter().sum::<u64>() + bad,
        extra[0],
        extra[1],
        extra[2]
    );
    bad == 0
}
"#;
