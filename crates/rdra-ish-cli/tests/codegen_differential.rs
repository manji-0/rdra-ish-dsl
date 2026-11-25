//! The Rust and Go functions `codegen` prints do what BFS does, run as printed. On every
//! `samples.txt` root that `check` passes, each BFS move of a use case the generator printed
//! a function for becomes a case: the rows the function takes, as BFS's pre-state holds them,
//! with every column BFS does not track set to a value of its own. A driver generated from
//! the IR decodes the cases (one JSON line each), runs the function, and prints what it
//! returns; one compile per sample and language runs them all.
//!
//! - **Ran.** Every column of a returned row that BFS tracks holds what the successor holds
//!   (a value, null or present, the slot a key names); the id of a returned, deleted, marked
//!   or raised row names the slot BFS bound. A column BFS does not track holds what the IR
//!   writes (the source row's value, the input, a literal), so a printer that copies the
//!   wrong column fails here too.
//! - **Refused.** On a row in the function's source state where BFS makes no move of it (the
//!   converse scope of `codegen_equivalence.rs`: one existing row, no run guard), the
//!   function returns `Rejected`.
//!
//! The cases are deduplicated by function and arguments. The domain samples are a test of
//! their own, which `scripts/diff-test.sh` skips and CI's check job runs when a change
//! touches codegen, and on main. `RDRA_CODEGEN_SAMPLE=<part>` keeps the samples whose path contains `<part>`. A
//! missing `go` skips Go, except under `RDRA_REQUIRE_CODEGEN_TOOLS=1`. The driver preludes
//! are `tests/fixtures/codegen/differential/prelude.{rs,go}`.

mod codegen_cases;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use codegen_cases::{
    caps, check_passes, column_var, enum_name, explore, matching, pick, samples, Explored, Rows,
};
use rdra_ish_core::lifecycle::{
    Action, ActionOrigin, Domain, GuardWhy, ParamRole, StateSlice, TransitionSystem, Value, VarKind,
};
use rdra_ish_core::SemanticModel;
use rdra_ish_emit::codegen::ir::{
    Adapt, Assigned, DomainModel, Field, FieldType, FnDecl, Lit, MarkSource, OutKind, Scalar,
};
use rdra_ish_emit::codegen::{generate, CheckStatus, Lang, Options};
use serde_json::{json, Map, Value as Json};

// ── Names, as the printers spell them ────────────────────────────────────────

/// PascalCase of an id, as codegen's `type_ident`.
fn type_ident(id: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for ch in id.chars() {
        if ch == '_' || ch == '-' || ch == ' ' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(ch.to_uppercase());
            upper = false;
        } else {
            out.push(ch);
        }
    }
    match out.chars().next() {
        Some(c) if c.is_ascii_uppercase() => out,
        _ => format!("V{out}"),
    }
}

/// snake_case of a PascalCase name, as codegen's `snake`.
fn snake(id: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for ch in id.chars() {
        if ch == '-' || ch == ' ' {
            out.push('_');
            prev_lower = false;
            continue;
        }
        if ch.is_uppercase() {
            if prev_lower {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
            prev_lower = false;
        } else {
            out.push(ch);
            prev_lower = ch.is_lowercase() || ch.is_ascii_digit();
        }
    }
    out
}

/// A Rust field or function name, as the Rust printer escapes it.
fn rs_ident(name: &str) -> String {
    const NOT_RAW: [&str; 5] = ["self", "Self", "super", "crate", "_"];
    const KEYWORDS: [&str; 48] = [
        "as", "async", "await", "break", "const", "continue", "dyn", "else", "enum", "extern",
        "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
        "pub", "ref", "return", "static", "struct", "trait", "true", "type", "unsafe", "use",
        "where", "while", "abstract", "become", "box", "do", "final", "macro", "override", "priv",
        "typeof", "unsized", "virtual", "yield", "try", "gen",
    ];
    if NOT_RAW.contains(&name) {
        format!("{name}_")
    } else if KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

fn rs_variant(ident: &str) -> String {
    if ident == "Self" {
        "Self_".to_string()
    } else {
        ident.to_string()
    }
}

// ── The types a case passes and reads ────────────────────────────────────────

/// The columns an entity's rows hold, as its record or flat row declares them.
fn entity_fields<'d>(domain: &'d DomainModel, entity: &str) -> &'d [Field] {
    if let Some(s) = domain.sums.iter().find(|s| s.entity == entity) {
        return &s.row_fields;
    }
    domain
        .records
        .iter()
        .find(|r| r.id == entity)
        .map(|r| r.fields.as_slice())
        .unwrap_or_else(|| panic!("entity {entity} has no generated type"))
}

fn id_repr(domain: &DomainModel, id_ty: &str) -> Scalar {
    domain
        .ids
        .iter()
        .find(|i| i.name == id_ty)
        .unwrap_or_else(|| panic!("no id type {id_ty}"))
        .repr
}

// ── Concrete values ──────────────────────────────────────────────────────────

/// A value of its own for an untracked column: equal to no other column's.
fn sentinel(domain: &DomainModel, ty: &FieldType, tag: &str) -> Json {
    let h = tag.bytes().fold(2_166_136_261u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(16_777_619)
    });
    match ty {
        FieldType::Scalar(Scalar::Int) => json!(1_000 + (h % 1_000_000) as i64),
        FieldType::Scalar(Scalar::Bool) => json!(h % 2 == 0),
        FieldType::Scalar(_) => json!(format!("~{tag}")),
        FieldType::Enum(name) => {
            let decl = domain.enums.iter().find(|e| e.name == *name).expect("enum");
            json!(decl.values[(h as usize) % decl.values.len()].wire)
        }
        FieldType::Id(name) => match id_repr(domain, name) {
            Scalar::Int => json!(1_000 + (h % 1_000_000) as i64),
            Scalar::Bool => json!(true),
            _ => json!(format!("~{tag}")),
        },
        FieldType::Rows(_) => panic!("a list is not a column"),
    }
}

/// How a column's value is known.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Known {
    /// BFS holds this value (an enum value, a number, a bool, null, a key's slot, an id).
    Exact,
    /// BFS holds only that it is present: the value is a stand-in.
    Present,
    /// BFS does not track the column: the value is a stand-in.
    Untracked,
}

struct Conc<'a> {
    ts: &'a TransitionSystem,
    model: &'a SemanticModel,
    domain: &'a DomainModel,
}

impl Conc<'_> {
    fn entity(&self, id: &str) -> rdra_ish_core::lifecycle::EntId {
        self.ts
            .entity_ids()
            .find(|e| self.ts.entity(*e).id == id)
            .unwrap_or_else(|| panic!("no entity {id}"))
    }

    /// The id of row `slot` of `entity`.
    fn id(&self, entity: &str, slot: usize) -> Json {
        let ty = format!("{}Id", type_ident(entity));
        match id_repr(self.domain, &ty) {
            Scalar::Int => json!(100 * (slot as i64 + 1) + 7),
            Scalar::Bool => json!(slot % 2 == 1),
            _ => json!(format!("{entity}#{slot}")),
        }
    }

    /// The columns the state type of row `slot` keeps null.
    fn always_null(&self, state: &StateSlice, entity: &str, slot: usize) -> Vec<String> {
        let Some(sum) = self.domain.sums.iter().find(|s| s.entity == entity) else {
            return Vec::new();
        };
        let Some(var) = column_var(self.ts, entity, &sum.tag.wire) else {
            return Vec::new();
        };
        let Some(tag) = enum_name(self.ts, var, state.get(self.ts, var, slot)) else {
            return Vec::new();
        };
        sum.variants
            .iter()
            .find(|v| v.value.wire == tag)
            .map(|v| v.always_null.iter().map(|f| f.wire.clone()).collect())
            .unwrap_or_default()
    }

    /// Column `field` of row `slot` of `entity` in `state`.
    fn column(
        &self,
        state: &StateSlice,
        entity: &str,
        slot: usize,
        field: &Field,
    ) -> (Json, Known) {
        let ts = self.ts;
        let tag = format!("{entity}.{}#{slot}", field.wire);
        if field.ty == FieldType::Id(format!("{}Id", type_ident(entity))) {
            return (self.id(entity, slot), Known::Exact);
        }
        let e = self.entity(entity);
        let owner = ts.var_ids().find(|v| match &ts.var(*v).kind {
            VarKind::Owner { child, fk, .. } => {
                *child == e
                    && self
                        .model
                        .foreign_key(*fk)
                        .columns
                        .iter()
                        .any(|(c, _)| *c == field.wire)
            }
            _ => false,
        });
        if let Some(owner) = owner {
            let Domain::Instance { parent, .. } = ts.var(owner).domain else {
                panic!("an owner var ranges over a parent's slots");
            };
            return match ts.var(owner).domain.slot(state.get(ts, owner, slot)) {
                Some(p) => (
                    self.id(&ts.entity(parent).id.clone(), usize::from(p)),
                    Known::Exact,
                ),
                None => (Json::Null, Known::Exact),
            };
        }
        let Some(var) = column_var(ts, entity, &field.wire) else {
            if field.nullable && self.always_null(state, entity, slot).contains(&field.wire) {
                return (Json::Null, Known::Untracked);
            }
            return (sentinel(self.domain, &field.ty, &tag), Known::Untracked);
        };
        let value = state.get(ts, var, slot);
        match &ts.var(var).domain {
            Domain::Enum(_) => (
                json!(enum_name(ts, var, value).expect("an enum value")),
                Known::Exact,
            ),
            Domain::Bool => (json!(value == Value::TRUE), Known::Exact),
            Domain::Int | Domain::NullableInt => match ts.var(var).domain.number(value) {
                Some(n) => (json!(n), Known::Exact),
                None => (Json::Null, Known::Exact),
            },
            Domain::Nullable if value == Value::NULL => (Json::Null, Known::Exact),
            Domain::Nullable => (sentinel(self.domain, &field.ty, &tag), Known::Present),
            other => panic!("column {tag} has a {other:?} var"),
        }
    }

    /// Row `slot` of `entity` in `state`, as a JSON object of its columns.
    fn row(&self, state: &StateSlice, entity: &str, slot: usize) -> Json {
        let mut out = Map::new();
        for field in entity_fields(self.domain, entity) {
            out.insert(
                field.wire.clone(),
                self.column(state, entity, slot, field).0,
            );
        }
        Json::Object(out)
    }

    /// Every present row of `entity` in `state`.
    fn rows(&self, state: &StateSlice, entity: &str) -> Json {
        let e = self.entity(entity);
        Json::Array(
            (0..self.ts.slots(e))
                .filter(|s| state.is_present(self.ts, e, *s))
                .map(|s| self.row(state, entity, s))
                .collect(),
        )
    }
}

// ── Cases ────────────────────────────────────────────────────────────────────

/// What a case checks in the function's result.
#[derive(Clone, Debug, PartialEq)]
enum Expect {
    /// The function runs: each `(path, value, how known)` holds in what it returns.
    Ran(Vec<(String, Json, Known)>),
    /// The function returns `Rejected`.
    Refused,
}

#[derive(Clone, Debug)]
struct Case {
    /// The index of the function in `domain.fns`.
    f: usize,
    args: Vec<Json>,
    expect: Expect,
    /// Where the case comes from, for a failure.
    from: String,
}

/// The arguments of `f` on the rows BFS binds in `pre`; inputs as `post` holds what they
/// write, or a stand-in where the function writes no column from one.
fn arguments(
    c: &Conc<'_>,
    f: &FnDecl,
    action: &Action,
    params: &[u8],
    pre: &StateSlice,
    post: Option<&StateSlice>,
) -> Vec<Json> {
    let ts = c.ts;
    let entity_of = |p: usize| ts.entity(action.params[p].entity).id.clone();
    let slot_of = |p: usize| usize::from(params[p]);
    let mut args = Vec::new();
    for r in &f.rows {
        let p = r.param.expect("a function row binds a param");
        args.push(c.row(pre, &entity_of(p), slot_of(p)));
    }
    for m in &f.marks_in {
        args.push(c.id(&entity_of(m.param), slot_of(m.param)));
    }
    if let Some(ev) = &f.event_in {
        let decl = c
            .domain
            .events
            .iter()
            .find(|e| e.name == ev.ty)
            .expect("an event type");
        let mut obj = Map::new();
        for field in &decl.fields {
            let e = c.entity(&field.entity);
            let p = action
                .params
                .iter()
                .position(|q| q.entity == e && q.role == ParamRole::Triggered)
                .or_else(|| action.params.iter().position(|q| q.entity == e));
            obj.insert(
                field.name.clone(),
                c.id(&field.entity, p.map_or(0, slot_of)),
            );
        }
        args.push(Json::Object(obj));
    }
    for (i, input) in f.inputs.iter().enumerate() {
        let value = if let FieldType::Rows(ty) = &input.ty {
            c.rows(pre, &list_entity(c.domain, ty))
        } else {
            input_value(c, f, action, params, post, i)
                .unwrap_or_else(|| sentinel(c.domain, &input.ty, &format!("input {}", input.name)))
        };
        args.push(value);
    }
    args
}

/// The entity whose rows a list of `ty` holds: a record's, or a flat row's.
fn list_entity(d: &DomainModel, ty: &str) -> String {
    d.records
        .iter()
        .find(|r| r.name == ty)
        .map(|r| r.id.clone())
        .or_else(|| {
            d.sums
                .iter()
                .find(|s| s.row == ty)
                .map(|s| s.entity.clone())
        })
        .unwrap_or_else(|| panic!("no record or row type {ty}"))
}

/// The value of input `i`: the column it writes as `post` holds it, or the id of the row it
/// names.
fn input_value(
    c: &Conc<'_>,
    f: &FnDecl,
    action: &Action,
    params: &[u8],
    post: Option<&StateSlice>,
    i: usize,
) -> Option<Json> {
    let ts = c.ts;
    let post = post?;
    for o in &f.outputs {
        let OutKind::Row { fields, .. } = &o.kind else {
            continue;
        };
        for (target, assigned) in fields {
            if let Assigned::Input { input, .. } = assigned {
                if *input == i {
                    let entity = ts.entity(action.params[o.param].entity).id.clone();
                    let slot = usize::from(params[o.param]);
                    return Some(c.column(post, &entity, slot, target).0);
                }
            }
        }
    }
    let marks = f.marks_out.iter().map(|m| (&m.source, m.param));
    let raised = f.raises.iter().flat_map(|ev| {
        ev.ids.iter().filter_map(|(field, source)| {
            let e = c.entity(&field.entity);
            action
                .params
                .iter()
                .position(|q| q.entity == e)
                .map(|p| (source, p))
        })
    });
    for (source, p) in marks.chain(raised) {
        if matches!(source, MarkSource::Input(k) if *k == i) {
            let entity = ts.entity(action.params[p].entity).id.clone();
            return Some(c.id(&entity, usize::from(params[p])));
        }
    }
    None
}

/// What `f` must return on a BFS move to `post`: each column of a returned row as BFS holds
/// it (or, where BFS does not track it or holds only that it is present, as the IR writes
/// it from the arguments), and the ids of the deleted, marked and raised rows.
fn expected(
    c: &Conc<'_>,
    f: &FnDecl,
    action: &Action,
    params: &[u8],
    args: &[Json],
    post: &StateSlice,
) -> Vec<(String, Json, Known)> {
    let ts = c.ts;
    let entity_of = |p: usize| ts.entity(action.params[p].entity).id.clone();
    let slot_of = |p: usize| usize::from(params[p]);
    let n_rows = f.rows.len();
    let n_marks = f.marks_in.len();
    let n_event = usize::from(f.event_in.is_some());
    let input_arg = |i: usize| &args[n_rows + n_marks + n_event + i];
    let row_arg = |r: usize, wire: &str| args[r].get(wire).cloned().unwrap_or(Json::Null);
    let mut out = Vec::new();
    for o in &f.outputs {
        let entity = entity_of(o.param);
        let slot = slot_of(o.param);
        match &o.kind {
            OutKind::Deleted { id: Some(_), .. } => {
                out.push((
                    format!("deleted.{}", o.name),
                    c.id(&entity, slot),
                    Known::Exact,
                ));
            }
            OutKind::Deleted { id: None, .. } => {}
            OutKind::Row { fields, .. } => {
                for field in entity_fields(c.domain, &entity) {
                    let path = format!("row.{}.{}", o.name, field.wire);
                    let (bfs, known) = c.column(post, &entity, slot, field);
                    match known {
                        Known::Exact => {
                            out.push((path, bfs, known));
                            continue;
                        }
                        // Present in BFS, and the value the IR writes.
                        Known::Present => out.push((path.clone(), Json::Null, known)),
                        Known::Untracked => {}
                    }
                    let ir = match fields.iter().find(|(t, _)| t.wire == field.wire) {
                        None => Json::Null,
                        Some((_, assigned)) => match assigned {
                            Assigned::From { row, field, adapt } => match adapt {
                                Adapt::AbsentNull => Json::Null,
                                _ => row_arg(*row, &field.wire),
                            },
                            Assigned::Lit { lit, .. } => match lit {
                                Lit::Enum { value, .. } => json!(value.wire),
                                Lit::Bool(b) => json!(b),
                                Lit::Int(n) => json!(n),
                            },
                            Assigned::Input { input, .. } => input_arg(*input).clone(),
                            Assigned::ParentId { row, field, .. } => row_arg(*row, &field.wire),
                            Assigned::Switch {
                                row, field, arms, ..
                            } => {
                                let from = row_arg(*row, &field.wire);
                                arms.iter()
                                    .find(|(a, _)| json!(a.wire) == from)
                                    .map_or(Json::Null, |(_, to)| json!(to.wire))
                            }
                            Assigned::EventId { field, .. } => args[n_rows + n_marks]
                                .get(field)
                                .cloned()
                                .unwrap_or(Json::Null),
                            Assigned::MarkId { mark, .. } => args[n_rows + *mark].clone(),
                            Assigned::Null => Json::Null,
                        },
                    };
                    out.push((path, ir, Known::Untracked));
                }
            }
        }
    }
    for m in &f.marks_out {
        out.push((
            format!("mark.{}", m.name),
            c.id(&entity_of(m.param), slot_of(m.param)),
            Known::Exact,
        ));
    }
    for ev in &f.raises {
        for (field, _) in &ev.ids {
            let e = c.entity(&field.entity);
            if let Some(p) = action.params.iter().position(|q| q.entity == e) {
                out.push((
                    format!("event.{}.{}", snake(&ev.ty), field.name),
                    c.id(&field.entity, slot_of(p)),
                    Known::Exact,
                ));
            }
        }
    }
    out
}

/// Every case of a sample: a move of each BFS edge, and a refusal on each row in a
/// function's source state BFS makes no move of it from.
fn cases(sample: &str, x: &Explored) -> (Vec<Case>, Vec<String>) {
    let Explored { model, domain, run } = x;
    let ts = &run.system;
    let c = Conc { ts, model, domain };
    let index: BTreeMap<&str, usize> = domain
        .fns
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();
    let mut out: Vec<Case> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut failures = Vec::new();
    // (state, function, slot) of every move of a one-row function.
    let mut moved: BTreeSet<(u32, usize, u8)> = BTreeSet::new();
    let mut push = |case: Case, out: &mut Vec<Case>| {
        let key = format!("{} {:?} {:?}", case.f, case.args, case.expect);
        if seen.insert(key) {
            out.push(case);
        }
    };
    for edge in run.edges() {
        let (act, params) = run.step(edge);
        let action = &ts.actions()[act.index()];
        let ActionOrigin::UseCase { .. } = action.origin else {
            continue;
        };
        if domain.skipped.iter().any(|s| s.action == action.name) {
            continue;
        }
        let pre = &run.states.at(edge.from as usize);
        let post = &run.states.at(edge.to as usize);
        let f = match pick(domain, model, ts, action, params, pre, post) {
            Ok(f) => f,
            Err(why) => {
                failures.push(format!("{sample}: {why}"));
                continue;
            }
        };
        let fi = index[f.name.as_str()];
        if params.len() == 1 {
            moved.insert((edge.from, fi, params[0]));
        }
        let args = arguments(&c, f, action, params, pre, Some(post));
        let expect = Expect::Ran(expected(&c, f, action, params, &args, post));
        push(
            Case {
                f: fi,
                args,
                expect,
                from: format!("the move of state {} to {}", edge.from, edge.to),
            },
            &mut out,
        );
    }
    if !run.complete {
        return (out, failures);
    }
    // Refusals, in the converse scope of codegen_equivalence.rs.
    for (fi, f) in domain.fns.iter().enumerate() {
        let Some(action) = ts.actions().iter().find(|a| a.name == f.action) else {
            continue;
        };
        if action.params.len() != 1
            || action.params[0].role != ParamRole::Existing
            || f.rows.len() != 1
            || action
                .guards
                .iter()
                .any(|g| matches!(g.why, GuardWhy::Run { .. }))
        {
            continue;
        }
        let e = action.params[0].entity;
        let entity = ts.entity(e).id.clone();
        for (at, state) in run.states.iter().enumerate() {
            for slot in 0..ts.slots(e) {
                if !state.is_present(ts, e, slot) || moved.contains(&(at as u32, fi, slot as u8)) {
                    continue;
                }
                let rows = Rows::new(
                    ts,
                    model,
                    &state,
                    vec![(entity.clone(), slot)],
                    f.elems.iter().map(|e| e.entity.clone()).collect(),
                );
                if matching(std::slice::from_ref(f), &f.action, &rows).is_empty()
                    || !rows.checks_pass(f)
                {
                    continue;
                }
                let args = arguments(&c, f, action, &[slot as u8], &state, None);
                push(
                    Case {
                        f: fi,
                        args,
                        expect: Expect::Refused,
                        from: format!("{entity}#{slot} in state {at}, where BFS makes no move"),
                    },
                    &mut out,
                );
            }
        }
    }
    (out, failures)
}

// ── The Rust driver ──────────────────────────────────────────────────────────

const RS_PRELUDE: &str = include_str!("fixtures/codegen/differential/prelude.rs");

fn rs_scalar_dec(s: Scalar, j: &str) -> String {
    match s {
        Scalar::Int => format!("{j}.int()"),
        Scalar::String => format!("{j}.str()"),
        Scalar::Bool => format!("{j}.bool()"),
        other => format!("{}({j}.str())", other.name()),
    }
}

fn rs_scalar_enc(s: Scalar, r: &str) -> String {
    match s {
        Scalar::Int => format!("DrvJ::Num(*{r})"),
        Scalar::String => format!("DrvJ::Str({r}.clone())"),
        Scalar::Bool => format!("DrvJ::Bool(*{r})"),
        _ => format!("DrvJ::Str({r}.0.clone())"),
    }
}

/// An expression decoding JSON `j` (a `&DrvJ`) as a value of `ty`.
fn rs_dec(d: &DomainModel, ty: &FieldType, nullable: bool, j: &str) -> String {
    if nullable {
        return format!("drv_opt({j}, |j| {})", rs_dec(d, ty, false, "j"));
    }
    match ty {
        FieldType::Scalar(s) => rs_scalar_dec(*s, j),
        FieldType::Enum(name) => {
            format!("{name}::parse(&{j}.str()).expect(\"driver: a value of {name}\")")
        }
        FieldType::Id(name) => format!("{name}({})", rs_scalar_dec(id_repr(d, name), j)),
        FieldType::Rows(name) => format!("{j}.arr().iter().map(dec_{name}).collect::<Vec<_>>()"),
    }
}

/// An expression encoding `r` (a reference to a value of `ty`) as a `DrvJ`.
fn rs_enc(d: &DomainModel, ty: &FieldType, nullable: bool, r: &str) -> String {
    if nullable {
        return format!("drv_opt_enc({r}, |x| {})", rs_enc(d, ty, false, "x"));
    }
    match ty {
        FieldType::Scalar(s) => rs_scalar_enc(*s, r),
        FieldType::Enum(_) => format!("DrvJ::Str({r}.as_str().to_string())"),
        FieldType::Id(name) => rs_scalar_enc(id_repr(d, name), &format!("(&{r}.0)")),
        FieldType::Rows(_) => panic!("a list is not returned"),
    }
}

fn rust_driver(d: &DomainModel) -> String {
    let mut out = String::from(RS_PRELUDE);
    // Decoders and encoders of every record and flat row.
    let mut shapes: Vec<(&str, &[Field])> = d
        .records
        .iter()
        .map(|r| (r.name.as_str(), r.fields.as_slice()))
        .collect();
    shapes.extend(
        d.sums
            .iter()
            .map(|s| (s.row.as_str(), s.row_fields.as_slice())),
    );
    for (name, fields) in &shapes {
        let inits: Vec<String> = fields
            .iter()
            .map(|f| {
                format!(
                    "{}: {}",
                    rs_ident(&f.wire),
                    rs_dec(d, &f.ty, f.nullable, &format!("j.get({:?})", f.wire))
                )
            })
            .collect();
        let _ = writeln!(
            out,
            "fn dec_{name}(j: &DrvJ) -> {name} {{ {name} {{ {} }} }}",
            inits.join(", ")
        );
        let pairs: Vec<String> = fields
            .iter()
            .map(|f| {
                format!(
                    "({:?}, {})",
                    f.wire,
                    rs_enc(d, &f.ty, f.nullable, &format!("(&v.{})", rs_ident(&f.wire)))
                )
            })
            .collect();
        let _ = writeln!(
            out,
            "fn enc_{name}(v: &{name}) -> DrvJ {{ drv_obj(vec![{}]) }}",
            pairs.join(", ")
        );
    }
    for sum in &d.sums {
        for v in &sum.variants {
            let _ = writeln!(
                out,
                "fn dec_{0}(j: &DrvJ) -> {0} {{ match {1}::from_row(dec_{2}(j)) {{ ::core::result::Result::Ok({1}::{3}(v)) => v, other => panic!(\"driver: from_row gives {{other:?}}\") }} }}",
                v.name,
                sum.name,
                sum.row,
                rs_variant(&v.value.ident)
            );
            let _ = writeln!(
                out,
                "fn enc_{0}(v: &{0}) -> DrvJ {{ enc_{2}(&{1}::from(v.clone()).into_row()) }}",
                v.name, sum.name, sum.row
            );
        }
    }
    let mut run = String::from("fn drv_run(f: i64, a: &[DrvJ]) -> DrvJ {\n    match f {\n");
    for (k, f) in d.fns.iter().enumerate() {
        let _ = writeln!(run, "        {k} => case_{k}(a),");
        let mut body = String::new();
        let mut call = Vec::new();
        let mut at = 0;
        for r in &f.rows {
            let _ = writeln!(body, "    let a{at} = dec_{}(&a[{at}]);", r.ty);
            call.push(format!("a{at}"));
            at += 1;
        }
        for m in &f.marks_in {
            let id_ty = d
                .marks
                .iter()
                .find(|x| x.name == m.ty)
                .expect("a mark")
                .id_ty
                .clone();
            let _ = writeln!(
                body,
                "    let a{at} = {} {{ id: {} }};",
                m.ty,
                rs_dec(d, &FieldType::Id(id_ty), false, &format!("(&a[{at}])"))
            );
            call.push(format!("a{at}"));
            at += 1;
        }
        if let Some(ev) = &f.event_in {
            let decl = d.events.iter().find(|e| e.name == ev.ty).expect("an event");
            let value = if decl.fields.is_empty() {
                ev.ty.clone()
            } else {
                let inits: Vec<String> = decl
                    .fields
                    .iter()
                    .map(|x| {
                        format!(
                            "{}: {}",
                            rs_ident(&x.name),
                            rs_dec(
                                d,
                                &FieldType::Id(x.id_ty.clone()),
                                false,
                                &format!("a[{at}].get({:?})", x.name)
                            )
                        )
                    })
                    .collect();
                format!("{} {{ {} }}", ev.ty, inits.join(", "))
            };
            let _ = writeln!(body, "    let a{at} = {value};");
            call.push(format!("a{at}"));
            at += 1;
        }
        for i in &f.inputs {
            let _ = writeln!(
                body,
                "    let a{at} = {};",
                rs_dec(d, &i.ty, i.nullable, &format!("(&a[{at}])"))
            );
            call.push(if matches!(i.ty, FieldType::Rows(_)) {
                format!("&a{at}")
            } else {
                format!("a{at}")
            });
            at += 1;
        }
        let _ = writeln!(
            body,
            "    let r = {}({});",
            rs_ident(&snake(&f.name)),
            call.join(", ")
        );
        if !f.conditions.is_empty() {
            body.push_str("    let r = match r { ::core::result::Result::Ok(r) => r, ::core::result::Result::Err(e) => return drv_obj(vec![(\"rejected\", DrvJ::Str(e.condition.to_string()))]) };\n");
        }
        let mut pairs = Vec::new();
        for o in &f.outputs {
            match &o.kind {
                OutKind::Row { ty, .. } => pairs.push(format!(
                    "(\"row.{}\", enc_{ty}(&r.{}))",
                    o.name,
                    rs_ident(&o.name)
                )),
                OutKind::Deleted {
                    id: Some((ty, _)), ..
                } => pairs.push(format!(
                    "(\"deleted.{}\", {})",
                    o.name,
                    rs_enc(
                        d,
                        &FieldType::Id(ty.clone()),
                        false,
                        &format!("(&r.{})", rs_ident(&o.name))
                    )
                )),
                OutKind::Deleted { id: None, .. } => {}
            }
        }
        for m in &f.marks_out {
            let id_ty = d
                .marks
                .iter()
                .find(|x| x.name == m.ty)
                .expect("a mark")
                .id_ty
                .clone();
            pairs.push(format!(
                "(\"mark.{}\", {})",
                m.name,
                rs_enc(
                    d,
                    &FieldType::Id(id_ty),
                    false,
                    &format!("(&r.{}.id)", rs_ident(&m.name))
                )
            ));
        }
        for ev in &f.raises {
            let ev_field = rs_ident(&snake(&ev.ty));
            for (field, _) in &ev.ids {
                pairs.push(format!(
                    "(\"event.{}.{}\", {})",
                    snake(&ev.ty),
                    field.name,
                    rs_enc(
                        d,
                        &FieldType::Id(field.id_ty.clone()),
                        false,
                        &format!("(&r.{ev_field}.{})", rs_ident(&field.name))
                    )
                ));
            }
        }
        let _ = writeln!(
            out,
            "fn case_{k}(a: &[DrvJ]) -> DrvJ {{\n{body}    drv_obj(vec![(\"ran\", drv_obj(vec![{}]))])\n}}",
            pairs.join(", ")
        );
    }
    run.push_str("        _ => panic!(\"driver: no function {f}\"),\n    }\n}\n");
    out.push_str(&run);
    out
}

// ── The Go driver ────────────────────────────────────────────────────────────

const GO_PRELUDE: &str = include_str!("fixtures/codegen/differential/prelude.go");

fn go_scalar(s: Scalar) -> &'static str {
    match s {
        Scalar::Int => "int64",
        Scalar::String => "string",
        Scalar::Bool => "bool",
        other => other.name(),
    }
}

fn go_type(ty: &FieldType) -> String {
    match ty {
        FieldType::Scalar(s) => go_scalar(*s).to_string(),
        FieldType::Enum(n) | FieldType::Id(n) => n.clone(),
        FieldType::Rows(n) => format!("[]{n}"),
    }
}

fn go_scalar_dec(s: Scalar, j: &str) -> String {
    match s {
        Scalar::Int => format!("drvInt({j})"),
        Scalar::String => format!("drvStr({j})"),
        Scalar::Bool => format!("drvBool({j})"),
        other => format!("{}(drvStr({j}))", other.name()),
    }
}

fn go_scalar_enc(s: Scalar, v: &str) -> String {
    match s {
        Scalar::Int => format!("int64({v})"),
        Scalar::Bool => format!("bool({v})"),
        _ => format!("string({v})"),
    }
}

fn go_dec(d: &DomainModel, ty: &FieldType, nullable: bool, j: &str) -> String {
    if nullable {
        return format!(
            "drvOpt({j}, func(j any) {} {{ return {} }})",
            go_type(ty),
            go_dec(d, ty, false, "j")
        );
    }
    match ty {
        FieldType::Scalar(s) => go_scalar_dec(*s, j),
        FieldType::Enum(name) => format!("{name}(drvStr({j}))"),
        FieldType::Id(name) => format!("{name}({})", go_scalar_dec(id_repr(d, name), j)),
        FieldType::Rows(name) => format!("drvList({j}, drvDec{name})"),
    }
}

fn go_enc(d: &DomainModel, ty: &FieldType, nullable: bool, v: &str) -> String {
    if nullable {
        return format!(
            "drvPtr({v}, func(x {}) any {{ return {} }})",
            go_type(ty),
            go_enc(d, ty, false, "x")
        );
    }
    match ty {
        FieldType::Scalar(s) => go_scalar_enc(*s, v),
        FieldType::Enum(_) => format!("string({v})"),
        FieldType::Id(name) => go_scalar_enc(id_repr(d, name), v),
        FieldType::Rows(_) => panic!("a list is not returned"),
    }
}

fn go_driver(d: &DomainModel) -> String {
    let mut out = String::from(GO_PRELUDE);
    let mut shapes: Vec<(&str, &[Field])> = d
        .records
        .iter()
        .map(|r| (r.name.as_str(), r.fields.as_slice()))
        .collect();
    shapes.extend(
        d.sums
            .iter()
            .map(|s| (s.row.as_str(), s.row_fields.as_slice())),
    );
    for (name, fields) in &shapes {
        let inits: Vec<String> = fields
            .iter()
            .map(|f| {
                format!(
                    "{}: {}",
                    f.pascal,
                    go_dec(d, &f.ty, f.nullable, &format!("drvGet(j, {:?})", f.wire))
                )
            })
            .collect();
        let _ = writeln!(
            out,
            "\nfunc drvDec{name}(j any) {name} {{ return {name}{{{}}} }}",
            inits.join(", ")
        );
        let pairs: Vec<String> = fields
            .iter()
            .map(|f| {
                format!(
                    "{:?}: {}",
                    f.wire,
                    go_enc(d, &f.ty, f.nullable, &format!("v.{}", f.pascal))
                )
            })
            .collect();
        let _ = writeln!(
            out,
            "\nfunc drvEnc{name}(v {name}) map[string]any {{ return map[string]any{{{}}} }}",
            pairs.join(", ")
        );
    }
    for sum in &d.sums {
        for v in &sum.variants {
            let _ = writeln!(
                out,
                "\nfunc drvDec{0}(j any) {0} {{\n\ts, err := {1}FromRow(drvDec{2}(j))\n\tif err != nil {{\n\t\tpanic(\"driver: from_row: \" + err.Error())\n\t}}\n\tv, ok := s.({0})\n\tif !ok {{\n\t\tpanic(fmt.Sprintf(\"driver: from_row gives %T\", s))\n\t}}\n\treturn v\n}}",
                v.name, sum.name, sum.row
            );
            let _ = writeln!(
                out,
                "\nfunc drvEnc{0}(v {0}) map[string]any {{ return drvEnc{2}({1}ToRow(v)) }}",
                v.name, sum.name, sum.row
            );
        }
    }
    let mut run = String::from("\nfunc drvRun(f int64, a []any) map[string]any {\n\tswitch f {\n");
    for (k, f) in d.fns.iter().enumerate() {
        let _ = writeln!(run, "\tcase {k}:\n\t\treturn drvCase{k}(a)");
        let mut call = Vec::new();
        let mut at = 0;
        for r in &f.rows {
            call.push(format!("drvDec{}(a[{at}])", r.ty));
            at += 1;
        }
        for m in &f.marks_in {
            let id_ty = d
                .marks
                .iter()
                .find(|x| x.name == m.ty)
                .expect("a mark")
                .id_ty
                .clone();
            call.push(format!(
                "{}{{Id: {}}}",
                m.ty,
                go_dec(d, &FieldType::Id(id_ty), false, &format!("a[{at}]"))
            ));
            at += 1;
        }
        if let Some(ev) = &f.event_in {
            let decl = d.events.iter().find(|e| e.name == ev.ty).expect("an event");
            let inits: Vec<String> = decl
                .fields
                .iter()
                .map(|x| {
                    format!(
                        "{}: {}",
                        type_ident(&x.name),
                        go_dec(
                            d,
                            &FieldType::Id(x.id_ty.clone()),
                            false,
                            &format!("drvGet(a[{at}], {:?})", x.name)
                        )
                    )
                })
                .collect();
            call.push(format!("{}{{{}}}", ev.ty, inits.join(", ")));
            at += 1;
        }
        for i in &f.inputs {
            call.push(go_dec(d, &i.ty, i.nullable, &format!("a[{at}]")));
            at += 1;
        }
        let mut body = String::new();
        if f.conditions.is_empty() {
            let _ = writeln!(body, "\tr := {}({})", f.name, call.join(", "));
        } else {
            let _ = writeln!(
                body,
                "\tr, err := {}({})\n\tif err != nil {{\n\t\treturn map[string]any{{\"rejected\": err.(*Rejected).Condition}}\n\t}}",
                f.name,
                call.join(", ")
            );
        }
        let mut pairs = Vec::new();
        for o in &f.outputs {
            let field = type_ident(&o.name);
            match &o.kind {
                OutKind::Row { ty, .. } => {
                    pairs.push(format!("\"row.{}\": drvEnc{ty}(r.{field})", o.name))
                }
                OutKind::Deleted {
                    id: Some((ty, _)), ..
                } => pairs.push(format!(
                    "\"deleted.{}\": {}",
                    o.name,
                    go_enc(d, &FieldType::Id(ty.clone()), false, &format!("r.{field}"))
                )),
                OutKind::Deleted { id: None, .. } => {}
            }
        }
        for m in &f.marks_out {
            let id_ty = d
                .marks
                .iter()
                .find(|x| x.name == m.ty)
                .expect("a mark")
                .id_ty
                .clone();
            pairs.push(format!(
                "\"mark.{}\": {}",
                m.name,
                go_enc(
                    d,
                    &FieldType::Id(id_ty),
                    false,
                    &format!("r.{}.Id", type_ident(&m.name))
                )
            ));
        }
        for ev in &f.raises {
            for (field, _) in &ev.ids {
                pairs.push(format!(
                    "\"event.{}.{}\": {}",
                    snake(&ev.ty),
                    field.name,
                    go_enc(
                        d,
                        &FieldType::Id(field.id_ty.clone()),
                        false,
                        &format!("r.{}.{}", ev.ty, type_ident(&field.name))
                    )
                ));
            }
        }
        if pairs.is_empty() {
            body.push_str("\t_ = r\n");
        }
        let _ = writeln!(
            out,
            "\nfunc drvCase{k}(a []any) map[string]any {{\n{body}\treturn map[string]any{{\"ran\": map[string]any{{{}}}}}\n}}",
            pairs.join(", ")
        );
    }
    run.push_str("\t}\n\tpanic(fmt.Sprintf(\"driver: no function %d\", f))\n}\n");
    out.push_str(&run);
    out
}

// ── Running ──────────────────────────────────────────────────────────────────

fn go_available() -> bool {
    let found = Command::new("go").arg("version").output().is_ok();
    if !found {
        assert!(
            std::env::var("RDRA_REQUIRE_CODEGEN_TOOLS").as_deref() != Ok("1"),
            "go is not on PATH, and RDRA_REQUIRE_CODEGEN_TOOLS=1 requires it; the dev shell provides it"
        );
        eprintln!("skipping Go: go is not on PATH");
    }
    found
}

fn scratch(lang: &str, sample: &str) -> PathBuf {
    let name: String = sample
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let dir = std::env::temp_dir()
        .join(format!(
            "rdra-ish-codegen-differential-{}",
            std::process::id()
        ))
        .join(lang)
        .join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn printed(model: &SemanticModel, lang: Lang, package: &str) -> String {
    let options = Options {
        lang,
        package: package.to_string(),
        model_path: "sample".to_string(),
        version: "test".to_string(),
        conformance: None,
        check: CheckStatus::Passed {
            bounds: "test".to_string(),
            caps: caps(),
        },
    };
    generate(model, &options, &|_| None).expect("generates")
}

fn must(cmd: &mut Command, what: &str) {
    let output = cmd.output().unwrap_or_else(|e| panic!("{what}: {e}"));
    assert!(
        output.status.success(),
        "{what}:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Runs `exe` on the cases, one JSON line each, and returns its result lines.
fn drive(exe: &Path, cases: &[Case]) -> Vec<Json> {
    let mut input = String::new();
    for c in cases {
        input.push_str(&json!({ "f": c.f, "a": c.args }).to_string());
        input.push('\n');
    }
    let mut child = Command::new(exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("runs the driver");
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || {
        use std::io::Write;
        stdin.write_all(input.as_bytes()).unwrap();
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    assert!(
        output.status.success(),
        "{} failed:\n{}",
        exe.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<Json> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON result line"))
        .collect();
    assert_eq!(lines.len(), cases.len(), "one result line per case");
    lines
}

/// The value at a path: `row.<out>.<column>` is a column of a returned row (the driver
/// returns the row as an object under `row.<out>`); `deleted.<out>`, `mark.<name>` and
/// `event.<type>.<field>` are ids under their own keys.
fn at<'j>(ran: &'j Json, path: &str) -> Option<&'j Json> {
    match path.strip_prefix("row.") {
        Some(rest) => {
            let (name, column) = rest.split_once('.')?;
            ran.get(format!("row.{name}"))?.get(column)
        }
        None => ran.get(path),
    }
}

/// Failures of one language's results against the cases.
fn judge(
    lang: &str,
    sample: &str,
    d: &DomainModel,
    cases: &[Case],
    results: &[Json],
    tally: &mut Tally,
) -> Vec<String> {
    let mut failures = Vec::new();
    for (case, result) in cases.iter().zip(results) {
        let f = &d.fns[case.f];
        let say = |what: String| format!("{lang} {sample}: {} on {}: {what}", f.name, case.from);
        match (&case.expect, result) {
            (Expect::Ran(checks), r) if r.get("ran").is_some() => {
                tally.ran += 1;
                let ran = &r["ran"];
                for (path, want, known) in checks {
                    let got = at(ran, path);
                    let ok = match known {
                        Known::Present => got.is_some_and(|g| !g.is_null()),
                        _ => got == Some(want),
                    };
                    if !ok {
                        let by = match known {
                            Known::Untracked => "the IR writes",
                            _ => "BFS holds",
                        };
                        failures.push(say(format!(
                            "{path} is {}, {by} {}",
                            got.map_or("missing".to_string(), Json::to_string),
                            if *known == Known::Present {
                                "a value".to_string()
                            } else {
                                want.to_string()
                            }
                        )));
                    }
                }
            }
            (Expect::Refused, r) if r.get("rejected").is_some() => tally.refused += 1,
            (Expect::Ran(_), r) => {
                failures.push(say(format!("BFS moves; the function returns {r}")))
            }
            (Expect::Refused, r) => {
                failures.push(say(format!("BFS makes no move; the function returns {r}")))
            }
        }
    }
    failures
}

#[derive(Default)]
struct Tally {
    ran: usize,
    refused: usize,
}

/// The samples whose check and BFS take seconds each in a debug build: the domain samples.
/// They run in `printed_code_agrees_on_the_domain_samples`.
const DOMAIN: [&str; 5] = [
    "samples/clinic-ops",
    "samples/patent-ops",
    "samples/food-permit-ops",
    "samples/insurance-claim-ops",
    "samples/oidc-auth",
];

/// Every `samples.txt` root but the domain samples: about 30 s in a debug build.
#[test]
fn printed_code_agrees_on_the_small_samples() {
    agree(|s| !DOMAIN.contains(&s));
}

/// The domain samples: about 100 s in a debug build. `scripts/diff-test.sh` skips it; CI's
/// check job runs it when a change touches codegen, and on main.
#[test]
fn printed_code_agrees_on_the_domain_samples() {
    agree(|s| DOMAIN.contains(&s));
}

fn agree(pick_sample: impl Fn(&str) -> bool) {
    let only = std::env::var("RDRA_CODEGEN_SAMPLE").ok();
    let go = go_available();
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let mut failures: Vec<String> = Vec::new();
    let mut rust = Tally::default();
    let mut golang = Tally::default();
    let mut total = 0usize;
    for sample in samples() {
        if !pick_sample(&sample) || only.as_ref().is_some_and(|o| !sample.contains(o.as_str())) {
            continue;
        }
        let start = Instant::now();
        if !check_passes(&sample) {
            continue;
        }
        let x = explore(&sample);
        let (cases, why) = cases(&sample, &x);
        failures.extend(why);
        if cases.is_empty() {
            continue;
        }
        total += cases.len();
        let d = &x.domain;
        let explored = start.elapsed();

        let dir = scratch("rust", &sample);
        std::fs::write(
            dir.join("domain.rs"),
            printed(&x.model, Lang::Rust, "domain"),
        )
        .unwrap();
        std::fs::write(dir.join("driver.rs"), rust_driver(d)).unwrap();
        let exe = dir.join(format!("driver{}", std::env::consts::EXE_SUFFIX));
        must(
            Command::new(&rustc)
                .args(["--edition", "2021", "-A", "warnings", "-o"])
                .arg(&exe)
                .arg(dir.join("driver.rs")),
            &format!("rustc on the driver of {sample}"),
        );
        let results = drive(&exe, &cases);
        failures.extend(judge("rust", &sample, d, &cases, &results, &mut rust));
        let ran_rust = start.elapsed();

        if go {
            let dir = scratch("go", &sample);
            std::fs::write(dir.join("domain.go"), printed(&x.model, Lang::Go, "main")).unwrap();
            std::fs::write(dir.join("driver.go"), go_driver(d)).unwrap();
            std::fs::write(dir.join("go.mod"), "module example.com/driver\n\ngo 1.21\n").unwrap();
            let exe = dir.join(format!("driver{}", std::env::consts::EXE_SUFFIX));
            must(
                Command::new("go")
                    .args(["build", "-o"])
                    .arg(&exe)
                    .arg(".")
                    .current_dir(&dir)
                    .env("GOFLAGS", "-mod=mod"),
                &format!("go build on the driver of {sample}"),
            );
            let results = drive(&exe, &cases);
            failures.extend(judge("go", &sample, d, &cases, &results, &mut golang));
        }
        eprintln!(
            "{sample}: {} cases; check and BFS {:.1}s, Rust {:.1}s, Go {:.1}s",
            cases.len(),
            explored.as_secs_f64(),
            (ran_rust - explored).as_secs_f64(),
            (start.elapsed() - ran_rust).as_secs_f64()
        );
    }
    failures.sort();
    failures.dedup();
    // One failure per language, sample and function, with how many it stands for.
    let mut groups: BTreeMap<&str, (&str, usize)> = BTreeMap::new();
    for f in &failures {
        let key = f.split(" on ").next().unwrap_or(f);
        groups.entry(key).or_insert((f, 0)).1 += 1;
    }
    assert!(
        failures.is_empty(),
        "{} disagreement(s); one of each language, sample and function:\n{}",
        failures.len(),
        groups
            .values()
            .take(60)
            .map(|(f, n)| format!("{f} (and {} more)", n - 1))
            .collect::<Vec<_>>()
            .join("\n")
    );
    eprintln!(
        "{total} cases; Rust ran {} and refused {}; Go ran {} and refused {}",
        rust.ran, rust.refused, golang.ran, golang.refused
    );
    if only.is_some() {
        return;
    }
    assert!(
        rust.ran > 0 && rust.refused > 0,
        "Rust ran or refused no case"
    );
    if go {
        assert!(
            golang.ran > 0 && golang.refused > 0,
            "Go ran or refused no case"
        );
    }
}
