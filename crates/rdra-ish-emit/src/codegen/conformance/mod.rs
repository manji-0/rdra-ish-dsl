//! The conformance harness (docs/design/algebraic-codegen.md, "The conformance harness"): the
//! port a finished service implements, and the traces the generated harness drives it along.
//!
//! [`port`] decides the port once, in the IR, so the Rust and Go harnesses agree. [`traces`]
//! explores the transition system `check` and `verify` explore, at `Bounds::STATES`, and
//! writes walks from its initial states that take every move of a printed use case, with the
//! state the model reaches after each move and the calls it refuses on the way. The harness
//! abstracts the service's store to the same text and compares; it never asks the generated
//! core what to expect.

pub mod eval;

use std::collections::{BTreeSet, HashSet, VecDeque};

use rdra_ish_core::lifecycle::{
    ActionOrigin, Bounds, Caps, Domain, EntId, ParamRole, Scope, StateSlice, TransitionSystem,
    Value, VarId, VarKind,
};
use rdra_ish_core::SemanticModel;

use super::actions::snake;
use super::ir::{
    CallerDecl, CapabilityDecl, ContractDecl, DomainModel, EntryDecl, Field, FieldType, FnDecl,
    FnInput, OutKind, PortDecl, PortEntity, PortEvent, PortMethod, PortRun, Scalar, Track,
};
use super::{type_ident, CodegenError, Names};
use eval::{column_var, enum_name, matching, Rows};

/// The format line every traces file starts with; the harness refuses another.
pub const TRACES_FORMAT: &str = "# rdra-ish conformance traces 3";

/// The states the traces explore at most by default.
pub const DEFAULT_STATES: usize = 1 << 16;

/// The port of `domain`'s functions over the system `ts`, or why the harness cannot name a
/// row of it.
pub(super) fn port(
    model: &SemanticModel,
    ts: &TransitionSystem,
    domain: &DomainModel,
    names: &mut Names,
) -> Result<PortDecl, CodegenError> {
    let unsupported = |why: String| CodegenError::Conformance(why);
    for name in [
        "StoreSnapshot",
        "ConformancePort",
        "ConformanceReport",
        "Conform",
        "ConfWrite",
    ] {
        names.claim(name.to_string(), "the conformance harness".to_string())?;
    }
    let mut port = PortDecl::default();
    for e in ts.entity_ids() {
        let entity = &ts.entity(e).id;
        let Some(id) = domain.ids.iter().find(|i| i.entity == *entity) else {
            return Err(unsupported(format!(
                "entity {entity} has no one-column primary key to name its rows by"
            )));
        };
        if id.repr == Scalar::Bool {
            return Err(unsupported(format!(
                "entity {entity} has a Bool primary key"
            )));
        }
        let pk = model
            .entities
            .iter()
            .find(|(_, x)| x.id == *entity)
            .map(|(_, x)| x.primary_key[0].clone())
            .expect("an entity with an id type has a primary key");
        let (ty, columns) = match domain.sums.iter().find(|s| s.entity == *entity) {
            Some(sum) => (sum.row.clone(), sum.row_fields.clone()),
            None => {
                let record = domain
                    .records
                    .iter()
                    .find(|r| r.id == *entity && r.kind == super::ir::RecordKind::Entity)
                    .expect("every entity is a record or a state type");
                (record.name.clone(), record.fields.clone())
            }
        };
        let id_field = columns
            .iter()
            .find(|f| f.wire == pk)
            .cloned()
            .expect("the primary key is a column");
        let mut tracked = Vec::new();
        for field in &columns {
            if let Some(var) = column_var(ts, entity, &field.wire) {
                let how = if ts.var(var).domain == Domain::Nullable {
                    Track::Presence
                } else {
                    Track::Exact
                };
                tracked.push((field.clone(), how));
            } else if owner_var(model, ts, e, &field.wire).is_some() {
                tracked.push((field.clone(), Track::Exact));
            }
        }
        port.entities.push(PortEntity {
            entity: entity.clone(),
            field: snake(entity),
            ty,
            id: id_field,
            columns,
            tracked,
        });
    }
    let mut counted = BTreeSet::new();
    for v in ts.var_ids() {
        match &ts.var(v).kind {
            VarKind::Run { entity, usecase } => {
                let entity = &ts.entity(*entity).id;
                let name = names.claim(
                    format!("{}{}Run", type_ident(usecase), type_ident(entity)),
                    format!("the run marks of {usecase} on {entity}"),
                )?;
                let id_ty = port
                    .entities
                    .iter()
                    .find(|p| p.entity == *entity)
                    .map(|p| field_id_type(&p.id))
                    .expect("every entity of the system is in the port");
                port.runs.push(PortRun {
                    name,
                    field: format!("{}_{}_runs", snake(usecase), snake(entity)),
                    usecase: usecase.clone(),
                    entity: entity.clone(),
                    id_ty,
                });
            }
            VarKind::Raised {
                event,
                handler: Some(handler),
                ..
            } => {
                counted.insert(format!(
                    "the count of {event} events {handler} has yet to handle (a counted trigger)"
                ));
            }
            VarKind::Prop { entity, key } => {
                counted.insert(format!(
                    "the proposition {key} of {}",
                    ts.entity(*entity).id
                ));
            }
            VarKind::Now => {
                counted.insert("the clock (`now`): the traces take no tick".to_string());
            }
            _ => {}
        }
    }
    port.unchecked = counted.into_iter().collect();
    for event in &domain.events {
        let ids: Vec<_> = event
            .fields
            .iter()
            .map(|f| {
                let tracked = raised_var(ts, &f.entity, &event.name).is_some();
                (f.clone(), tracked)
            })
            .collect();
        if ids.iter().any(|(_, t)| *t) {
            port.events.push(PortEvent {
                ty: event.name.clone(),
                field: snake(&event.name),
                ids,
            });
        }
    }
    let mut actions: Vec<&str> = Vec::new();
    for f in &domain.fns {
        if !actions.contains(&f.action.as_str()) {
            actions.push(&f.action);
        }
    }
    for action in actions {
        let group: Vec<&FnDecl> = domain.fns.iter().filter(|f| f.action == action).collect();
        let first = group[0];
        let id_of = |entity: &str| -> Result<String, CodegenError> {
            domain
                .ids
                .iter()
                .find(|i| i.entity == entity)
                .map(|i| i.name.clone())
                .ok_or_else(|| {
                    unsupported(format!(
                        "{action} takes a {entity} row, which has no id to load it by"
                    ))
                })
        };
        let mut rows = Vec::new();
        for r in &first.rows {
            rows.push((r.name.clone(), id_of(&r.entity)?));
        }
        let marks = first
            .marks_in
            .iter()
            .map(|m| {
                let decl = domain
                    .marks
                    .iter()
                    .find(|d| d.name == m.ty)
                    .expect("a mark argument has a mark type");
                (m.name.clone(), decl.id_ty.clone())
            })
            .collect();
        let mut inputs: Vec<FnInput> = Vec::new();
        let mut choices: Vec<(String, String)> = Vec::new();
        for f in &group {
            for input in &f.inputs {
                if matches!(input.ty, FieldType::Rows(_)) {
                    continue;
                }
                if !inputs.iter().any(|x| x.name == input.name) {
                    inputs.push(input.clone());
                }
            }
            for (r, tag_enum, _) in &f.chosen {
                let name = format!("{}_to", f.rows[*r].name);
                if !choices.iter().any(|(n, _)| *n == name) {
                    choices.push((name, tag_enum.clone()));
                }
            }
        }
        let pascal = type_ident(action);
        let write = domain
            .dispatchers
            .iter()
            .find(|d| d.name == format!("Run{pascal}"))
            .map_or_else(|| first.result.clone(), |d| d.outcome.clone());
        port.methods.push(PortMethod {
            write,
            action: action.to_string(),
            name: type_ident(action),
            usecase: first.usecase.clone(),
            rows,
            marks,
            event: first
                .event_in
                .as_ref()
                .map(|e| (e.name.clone(), e.ty.clone())),
            inputs,
            choices,
        });
    }
    port.contracts = contracts(domain, &port.methods, names)?;
    Ok(port)
}

/// One contract per use case of `methods`, in the order of its first method.
fn contracts(
    domain: &DomainModel,
    methods: &[PortMethod],
    names: &mut Names,
) -> Result<Vec<ContractDecl>, CodegenError> {
    let mut out: Vec<ContractDecl> = Vec::new();
    for (i, m) in methods.iter().enumerate() {
        if let Some(c) = out.iter_mut().find(|c| c.usecase == m.usecase) {
            c.methods.push(i);
            continue;
        }
        let decl = domain.usecases.iter().find(|u| u.id == m.usecase);
        let pascal = type_ident(&m.usecase);
        out.push(ContractDecl {
            name: super::claim_first(
                names,
                [format!("{pascal}UseCase"), format!("{pascal}Contract")],
                format!("the contract of {}", m.usecase),
            )?,
            usecase: m.usecase.clone(),
            label: decl.map_or_else(String::new, |d| d.label.clone()),
            site: decl.and_then(|d| d.site.clone()),
            error: domain
                .error_of(&m.usecase)
                .map_or_else(String::new, |e| e.name.clone()),
            methods: vec![i],
            entries: Vec::new(),
            caller: None,
        });
    }
    Ok(out)
}

/// The callers of every contract (#337): the actors that perform its use case, each by a
/// capability type of its own, claimed once.
pub(super) fn add_callers(
    model: &SemanticModel,
    domain: &mut DomainModel,
    names: &mut Names,
) -> Result<(), CodegenError> {
    let Some(mut port) = domain.port.take() else {
        return Ok(());
    };
    for c in &mut port.contracts {
        let Some((uc, _)) = model.use_cases.iter().find(|(_, u)| u.id == c.usecase) else {
            continue;
        };
        let mut actors = Vec::new();
        for a in rdra_ish_core::actors_for_usecase(model, uc) {
            let actor = &model.actors[a];
            let i = match domain.capabilities.iter().position(|x| x.actor == actor.id) {
                Some(i) => i,
                None => {
                    let pascal = type_ident(&actor.id);
                    domain.capabilities.push(CapabilityDecl {
                        actor: actor.id.clone(),
                        label: actor.label.clone(),
                        name: super::claim_first(
                            names,
                            [format!("{pascal}Capability"), format!("{pascal}Cap")],
                            format!("the capability of {}", actor.id),
                        )?,
                    });
                    domain.capabilities.len() - 1
                }
            };
            actors.push(i);
        }
        if actors.is_empty() {
            continue;
        }
        let pascal = type_ident(&c.usecase);
        c.caller = Some(CallerDecl {
            name: super::claim_first(
                names,
                [format!("{pascal}Caller"), format!("{pascal}UseCaseCaller")],
                format!("the callers of {}", c.usecase),
            )?,
            actors,
        });
    }
    domain.port = Some(port);
    Ok(())
}

/// The entry points of the actions with no printed function (#335), added to their use
/// cases' contracts: a contract of its own for a use case with none printed. An action whose
/// rows have no id type gets none.
pub(super) fn add_entries(
    ts: &TransitionSystem,
    domain: &mut DomainModel,
    names: &mut Names,
) -> Result<(), CodegenError> {
    let Some(mut port) = domain.port.take() else {
        return Ok(());
    };
    for skipped in &domain.skipped {
        let Some(action) = ts.actions().iter().find(|a| a.name == skipped.action) else {
            continue;
        };
        let mut rows = Vec::new();
        for p in &action.params {
            let entity = &ts.entity(p.entity).id;
            let Some(id) = domain.ids.iter().find(|i| i.entity == *entity) else {
                rows.clear();
                break;
            };
            let base = snake(entity);
            let mut name = base.clone();
            let mut k = 2;
            while rows.iter().any(|(n, _): &(String, String)| *n == name) {
                name = format!("{base}_{k}");
                k += 1;
            }
            rows.push((name, id.name.clone()));
        }
        if rows.is_empty() && !action.params.is_empty() {
            continue;
        }
        let entry = EntryDecl {
            action: action.name.clone(),
            name: type_ident(&action.name),
            rows,
        };
        if let Some(c) = port
            .contracts
            .iter_mut()
            .find(|c| c.usecase == skipped.usecase)
        {
            c.entries.push(entry);
            continue;
        }
        let decl = domain.usecases.iter().find(|u| u.id == skipped.usecase);
        let pascal = type_ident(&skipped.usecase);
        port.contracts.push(ContractDecl {
            name: super::claim_first(
                names,
                [format!("{pascal}UseCase"), format!("{pascal}Contract")],
                format!("the contract of {}", skipped.usecase),
            )?,
            usecase: skipped.usecase.clone(),
            label: decl.map_or_else(String::new, |d| d.label.clone()),
            site: decl.and_then(|d| d.site.clone()),
            error: domain
                .error_of(&skipped.usecase)
                .map_or_else(String::new, |e| e.name.clone()),
            methods: Vec::new(),
            entries: vec![entry],
            caller: None,
        });
    }
    domain.port = Some(port);
    Ok(())
}

fn field_id_type(field: &Field) -> String {
    match &field.ty {
        FieldType::Id(name) => name.clone(),
        _ => unreachable!("a primary key with an id type is typed by it"),
    }
}

/// The owner var of `entity`'s one-column key `column`.
fn owner_var(
    model: &SemanticModel,
    ts: &TransitionSystem,
    e: EntId,
    column: &str,
) -> Option<VarId> {
    ts.var_ids().find(|v| match &ts.var(*v).kind {
        VarKind::Owner { child, fk, .. } => {
            let fk = model.foreign_key(*fk);
            *child == e && matches!(fk.columns.as_slice(), [(c, _)] if c == column)
        }
        _ => false,
    })
}

/// The latch that remembers `event` (by its type name) was raised on a row of `entity`.
fn raised_var(ts: &TransitionSystem, entity: &str, event: &str) -> Option<VarId> {
    ts.var_ids().find(|v| {
        matches!(&ts.var(*v).kind, VarKind::Raised { entity: e, event: ev, handler: None }
            if ts.entity(*e).id == entity && type_ident(ev) == event)
    })
}

/// What [`traces`] wrote and how much of the system it covers.
#[derive(Clone, Debug, Default)]
pub struct Traces {
    pub text: String,
    /// States the printed use cases reach from the initial states.
    pub states: usize,
    /// Distinct moves of the printed use cases from those states, and how many a trace takes.
    pub moves: usize,
    pub covered: usize,
    /// Moves of use cases with no function, or of no use case (a tick), which the traces leave out.
    pub left_out: usize,
    /// Whether BFS reached every state below the cap.
    pub complete: bool,
    pub traces: usize,
    pub calls: usize,
    pub refusals: usize,
    /// Calls whose write the traces try after another call changed a row it writes.
    pub interleavings: usize,
}

/// One move of a printed use case: the call that makes it.
struct Call {
    from: u32,
    to: u32,
    action: String,
    act: usize,
    /// The function the move takes, and the slot of each param.
    function: String,
    params: Vec<u8>,
    args: Vec<String>,
}

/// The traces of `domain`'s port, from a BFS of `model` at `Bounds::STATES` that stops at
/// `cap` states. `header` lines go after the format line.
pub fn traces(
    model: &SemanticModel,
    domain: &DomainModel,
    cap: usize,
    header: &[String],
) -> Traces {
    let port = domain
        .port
        .as_ref()
        .expect("traces are written for a model with a port");
    let built = TransitionSystem::build(model, Bounds::STATES, &Scope::ALL);
    let run = built
        .ts
        .clone()
        .run(Caps::new(Caps::for_instances(1).patterns, cap));
    let ts = &run.system;
    let cx = Cx { model, ts, domain };

    // Every move, and the calls that make the moves of a printed use case.
    let mut seen: HashSet<(u32, u32, usize, Vec<u8>)> = HashSet::new();
    let mut moved: HashSet<(u32, usize, Vec<u8>)> = HashSet::new();
    let mut calls: Vec<Call> = Vec::new();
    let mut out = Traces {
        complete: run.complete,
        ..Traces::default()
    };
    for edge in run.edges() {
        let (act, params) = run.step(edge);
        moved.insert((edge.from, act.index(), params.to_vec()));
        if !seen.insert((edge.from, edge.to, act.index(), params.to_vec())) {
            continue;
        }
        let action = &ts.actions()[act.index()];
        let ActionOrigin::UseCase { .. } = action.origin else {
            out.left_out += 1;
            continue;
        };
        let Some(method) = port.methods.iter().find(|m| m.action == action.name) else {
            out.left_out += 1;
            continue;
        };
        let pre = &run.states.at(edge.from as usize);
        let post = &run.states.at(edge.to as usize);
        let Some(f) = cx.function(&action.name, params, pre, post) else {
            out.left_out += 1;
            continue;
        };
        calls.push(Call {
            from: edge.from,
            to: edge.to,
            action: action.name.clone(),
            act: act.index(),
            function: f.name.clone(),
            params: params.to_vec(),
            args: cx.args(method, f, params, post),
        });
    }

    // The states the calls reach from the initial states, and a shortest call path to each.
    let n = run.states.len();
    let mut out_of: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, c) in calls.iter().enumerate() {
        out_of[c.from as usize].push(i);
    }
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut root: Vec<Option<u32>> = vec![None; n];
    let mut queue = VecDeque::new();
    for at in 0..n as u32 {
        if run.parent(at).is_none() {
            root[at as usize] = Some(at);
            queue.push_back(at);
        }
    }
    while let Some(u) = queue.pop_front() {
        for &c in &out_of[u as usize] {
            let v = calls[c].to as usize;
            if root[v].is_none() {
                root[v] = root[u as usize];
                parent[v] = Some(c);
                queue.push_back(v as u32);
            }
        }
    }
    let reached: Vec<u32> = (0..n as u32)
        .filter(|s| root[*s as usize].is_some())
        .collect();
    out.states = reached.len();
    out.moves = reached.iter().map(|s| out_of[*s as usize].len()).sum();

    // The calls the model refuses from each reached state: a use case over one row it runs
    // from no state of that row, where BFS makes no move and the functions allow none. Only
    // a complete run says no move exists.
    let refusals = |s: u32| -> Vec<(String, Vec<String>)> {
        if !run.complete {
            return Vec::new();
        }
        cx.refusals(port, &run.states.at(s as usize), |act, slot| {
            moved.contains(&(s, act, vec![slot]))
        })
    };

    // The walk: take a move not taken yet from where the trace is; when there is none, start
    // a new trace and follow the shortest path to a state that has one.
    let mut text = String::new();
    text.push_str(TRACES_FORMAT);
    text.push('\n');
    for line in header {
        text.push_str(&format!("# {line}\n"));
    }
    let mut body = String::new();
    let mut used_states: BTreeSet<u32> = BTreeSet::new();
    let mut visited: HashSet<u32> = HashSet::new();
    let mut left: Vec<Vec<usize>> = out_of.clone();
    let mut pending: BTreeSet<u32> = reached
        .iter()
        .copied()
        .filter(|s| !left[*s as usize].is_empty())
        .collect();
    let arrive = |s: u32,
                  body: &mut String,
                  visited: &mut HashSet<u32>,
                  out: &mut Traces,
                  used: &mut BTreeSet<u32>| {
        used.insert(s);
        if visited.insert(s) {
            for (action, args) in refusals(s) {
                out.refusals += 1;
                body.push_str(&format!("X {action}{}\n", joined(&args)));
            }
        }
    };
    // Every initial state starts a trace, so its seeding is checked even with no move.
    let mut starts: Vec<u32> = reached
        .iter()
        .copied()
        .filter(|s| root[*s as usize] == Some(*s))
        .collect();
    starts.reverse();
    loop {
        let target = match starts.pop() {
            Some(s) => s,
            None => match pending.iter().next() {
                Some(s) => *s,
                None => break,
            },
        };
        // A new trace: seed the initial state, then follow the path to `target`.
        let init = root[target as usize].expect("a reached state has an initial state");
        out.traces += 1;
        used_states.insert(init);
        body.push_str(&format!("T {init}\n"));
        for line in cx.seed(port, &run.states.at(init as usize)) {
            body.push_str(&format!("R {line}\n"));
        }
        body.push_str("G\n");
        arrive(init, &mut body, &mut visited, &mut out, &mut used_states);
        let mut path = Vec::new();
        let mut at = target;
        while let Some(c) = parent[at as usize] {
            path.push(c);
            at = calls[c].from;
        }
        path.reverse();
        let mut here = init;
        for c in path {
            let call = &calls[c];
            out.calls += 1;
            body.push_str(&format!(
                "C {} {}{}\n",
                call.to,
                call.action,
                joined(&call.args)
            ));
            here = call.to;
            arrive(here, &mut body, &mut visited, &mut out, &mut used_states);
        }
        // Take moves not taken yet while there are some here, preferring one into a state
        // that has more.
        loop {
            if left[here as usize].is_empty() {
                pending.remove(&here);
                break;
            }
            let pick = left[here as usize]
                .iter()
                .position(|c| {
                    let to = calls[*c].to;
                    to != here && !left[to as usize].is_empty()
                })
                .unwrap_or(0);
            let c = left[here as usize].remove(pick);
            if left[here as usize].is_empty() {
                pending.remove(&here);
            }
            let call = &calls[c];
            out.calls += 1;
            out.covered += 1;
            body.push_str(&format!(
                "C {} {}{}\n",
                call.to,
                call.action,
                joined(&call.args)
            ));
            here = call.to;
            arrive(here, &mut body, &mut visited, &mut out, &mut used_states);
        }
    }
    // Interleavings: in a reached state, call B between the read and the write of A (B may be
    // A again). When B leaves a row A writes in another state than A read it in, deletes it,
    // or creates the row A creates, and the model has no move of A from there, A's write must
    // be refused and change nothing: `check` and `verify` ran A as one atomic action and never
    // explored that write. One trace per pair of A's function and B's use case.
    let mut tried: BTreeSet<(String, String)> = BTreeSet::new();
    for &s in &reached {
        for &a in &out_of[s as usize] {
            for &b in &out_of[s as usize] {
                let (ca, cb) = (&calls[a], &calls[b]);
                if tried.contains(&(ca.function.clone(), cb.action.clone()))
                    || moved.contains(&(cb.to, ca.act, ca.params.clone()))
                    || !cx.breaks(
                        &ca.action,
                        &ca.function,
                        &ca.params,
                        &run.states.at(cb.to as usize),
                    )
                {
                    continue;
                }
                tried.insert((ca.function.clone(), cb.action.clone()));
                let init = root[s as usize].expect("a reached state has an initial state");
                out.traces += 1;
                out.interleavings += 1;
                used_states.insert(init);
                used_states.insert(cb.to);
                body.push_str(&format!("T {init}\n"));
                for line in cx.seed(port, &run.states.at(init as usize)) {
                    body.push_str(&format!("R {line}\n"));
                }
                body.push_str("G\n");
                let mut path = Vec::new();
                let mut at = s;
                while let Some(c) = parent[at as usize] {
                    path.push(c);
                    at = calls[c].from;
                }
                path.reverse();
                for c in path {
                    let call = &calls[c];
                    out.calls += 1;
                    used_states.insert(call.to);
                    body.push_str(&format!(
                        "C {} {}{}\n",
                        call.to,
                        call.action,
                        joined(&call.args)
                    ));
                }
                body.push_str(&format!(
                    "I {} {}{} | {}{}\n",
                    cb.to,
                    ca.action,
                    joined(&ca.args),
                    cb.action,
                    joined(&cb.args)
                ));
            }
        }
    }
    // The states first, so the harness knows each before a trace names it.
    for s in &used_states {
        text.push_str(&format!("S {s}\n"));
        for line in cx.abstract_state(port, &run.states.at(*s as usize)) {
            text.push_str(&format!("A {line}\n"));
        }
    }
    text.push_str(&body);
    out.text = text;
    out
}

fn joined(args: &[String]) -> String {
    args.iter().map(|a| format!(" {a}")).collect()
}

/// The text of a row's id: slot `s` is `s + 1`, in every id type the harness parses.
fn id_text(slot: usize) -> String {
    (slot + 1).to_string()
}

struct Cx<'a> {
    model: &'a SemanticModel,
    ts: &'a TransitionSystem,
    domain: &'a DomainModel,
}

impl Cx<'_> {
    fn entity(&self, id: &str) -> Option<EntId> {
        self.ts.entity_ids().find(|e| self.ts.entity(*e).id == id)
    }

    /// The one function of `action` that makes the move from `pre` to `post`.
    fn function(
        &self,
        action: &str,
        params: &[u8],
        pre: &StateSlice,
        post: &StateSlice,
    ) -> Option<&FnDecl> {
        let ts = self.ts;
        let act = ts.actions().iter().find(|a| a.name == action)?;
        let first = self.domain.fns.iter().find(|f| f.action == action)?;
        let slots = |f: &FnDecl| -> Vec<(String, usize)> {
            f.rows
                .iter()
                .map(|r| {
                    let p = r.param.expect("a function row binds a param");
                    (
                        ts.entity(act.params[p].entity).id.clone(),
                        usize::from(params[p]),
                    )
                })
                .collect()
        };
        let before = Rows::new(ts, self.model, pre, slots(first), Vec::new());
        let mut fns = matching(&self.domain.fns, action, &before);
        if fns.len() > 1 {
            fns.retain(|f| {
                f.outputs.iter().all(|o| match &o.kind {
                    OutKind::Row {
                        tag: Some((column, value)),
                        ..
                    } => {
                        let e = act.params[o.param].entity;
                        let slot = usize::from(params[o.param]);
                        column_var(ts, &ts.entity(e).id, column)
                            .and_then(|v| enum_name(ts, v, post.get(ts, v, slot)))
                            .is_some_and(|n| n == value.wire)
                    }
                    _ => true,
                })
            });
        }
        match fns.as_slice() {
            [f] => Some(f),
            _ => None,
        }
    }

    /// The call's arguments, in the port method's order: what makes the function write what
    /// the move wrote.
    fn args(&self, m: &PortMethod, f: &FnDecl, params: &[u8], post: &StateSlice) -> Vec<String> {
        let ts = self.ts;
        let act = ts
            .actions()
            .iter()
            .find(|a| a.name == f.action)
            .expect("the function's action");
        let mut args = Vec::new();
        for r in &f.rows {
            args.push(id_text(usize::from(
                params[r.param.expect("a function row binds a param")],
            )));
        }
        for mk in &f.marks_in {
            args.push(id_text(usize::from(params[mk.param])));
        }
        if let Some(ev) = &f.event_in {
            args.extend(self.event_ids(&ev.ty, act, params));
        }
        for input in &m.inputs {
            let value = f
                .inputs
                .iter()
                .position(|i| i.name == input.name)
                .and_then(|i| self.input_value(f, i, act, params, post));
            args.push(value.unwrap_or_else(|| default_input(self.domain, input)));
        }
        for (name, tag_enum) in &m.choices {
            let chosen = f
                .chosen
                .iter()
                .find(|(r, _, _)| format!("{}_to", f.rows[*r].name) == *name)
                .map(|(_, _, v)| v.wire.clone());
            args.push(chosen.unwrap_or_else(|| first_value(self.domain, tag_enum)));
        }
        args
    }

    /// The ids of an event value: each field names the action's row of its entity.
    fn event_ids(
        &self,
        ty: &str,
        act: &rdra_ish_core::lifecycle::Action,
        params: &[u8],
    ) -> Vec<String> {
        let event = self
            .domain
            .events
            .iter()
            .find(|e| e.name == ty)
            .expect("an event argument has an event type");
        event
            .fields
            .iter()
            .map(|field| {
                act.params
                    .iter()
                    .position(|p| self.ts.entity(p.entity).id == field.entity)
                    .map_or_else(|| id_text(0), |p| id_text(usize::from(params[p])))
            })
            .collect()
    }

    /// The value of input `i`: the id of the row it creates, or what the move left in the
    /// column it writes.
    fn input_value(
        &self,
        f: &FnDecl,
        i: usize,
        act: &rdra_ish_core::lifecycle::Action,
        params: &[u8],
        post: &StateSlice,
    ) -> Option<String> {
        let input = &f.inputs[i];
        for o in &f.outputs {
            let OutKind::Row { fields, .. } = &o.kind else {
                continue;
            };
            let e = act.params[o.param].entity;
            let slot = usize::from(params[o.param]);
            let port = self.port_entity(&self.ts.entity(e).id)?;
            for (target, assigned) in fields {
                if !matches!(assigned, super::ir::Assigned::Input { input: k, .. } if *k == i) {
                    continue;
                }
                if target.wire == port.id.wire {
                    return Some(id_text(slot));
                }
                return Some(self.concrete(e, slot, target, input.nullable, post));
            }
        }
        None
    }

    /// Whether, in `state`, a row the function `function` of `action` writes (at `params`) is
    /// no longer what the function read: gone, in another state, or created by someone else.
    fn breaks(&self, action: &str, function: &str, params: &[u8], state: &StateSlice) -> bool {
        let ts = self.ts;
        let (Some(act), Some(f)) = (
            ts.actions().iter().find(|a| a.name == action),
            self.domain.fns.iter().find(|f| f.name == function),
        ) else {
            return false;
        };
        act.params.iter().enumerate().any(|(p, param)| {
            let slot = usize::from(params[p]);
            let present = state.is_present(ts, param.entity, slot);
            match param.role {
                ParamRole::New => present,
                ParamRole::Existing => {
                    if !present {
                        return true;
                    }
                    let Some((tag, value)) = f
                        .rows
                        .iter()
                        .find(|r| r.param == Some(p))
                        .and_then(|r| r.state.as_ref())
                    else {
                        return false;
                    };
                    column_var(ts, &ts.entity(param.entity).id, tag)
                        .and_then(|v| enum_name(ts, v, state.get(ts, v, slot)))
                        .is_some_and(|now| now != *value)
                }
                _ => false,
            }
        })
    }

    fn port_entity(&self, entity: &str) -> Option<&PortEntity> {
        self.domain
            .port
            .as_ref()?
            .entities
            .iter()
            .find(|p| p.entity == entity)
    }

    /// A concrete value of `field` of row `slot` that abstracts to what the state holds;
    /// a default when the system does not track it.
    fn concrete(
        &self,
        e: EntId,
        slot: usize,
        field: &Field,
        nullable: bool,
        state: &StateSlice,
    ) -> String {
        let ts = self.ts;
        let entity = &ts.entity(e).id;
        if let Some(var) = column_var(ts, entity, &field.wire) {
            let v = state.get(ts, var, slot);
            return match &ts.var(var).domain {
                Domain::NullableEnum(_) | Domain::NullableBool if v == Value::NULL => {
                    "~".to_string()
                }
                d @ (Domain::Enum(_) | Domain::NullableEnum(_)) => {
                    d.name(v).expect("a declared value").to_string()
                }
                d @ (Domain::Bool | Domain::NullableBool) => {
                    d.truth(v).expect("a truth value").to_string()
                }
                Domain::Int => v.0.to_string(),
                Domain::NullableInt if v == Value::NULL => "~".to_string(),
                Domain::NullableInt => (v.0 - 1).to_string(),
                Domain::Nullable if v == Value::NULL => "~".to_string(),
                _ => default_value(self.domain, &field.ty),
            };
        }
        if let Some(var) = owner_var(self.model, ts, e, &field.wire) {
            return match ts.var(var).domain.slot(state.get(ts, var, slot)) {
                Some(p) => id_text(usize::from(p)),
                None => "~".to_string(),
            };
        }
        if nullable {
            "~".to_string()
        } else {
            default_value(self.domain, &field.ty)
        }
    }

    /// The rows of an initial state, every column of each row type in order.
    fn seed(&self, port: &PortDecl, state: &StateSlice) -> Vec<String> {
        let mut lines = Vec::new();
        for p in &port.entities {
            let Some(e) = self.entity(&p.entity) else {
                continue;
            };
            let sum = self.domain.sums.iter().find(|s| s.entity == p.entity);
            for slot in 0..self.ts.slots(e) {
                if !state.is_present(self.ts, e, slot) {
                    continue;
                }
                // The state's type: a column it keeps present needs a value.
                let variant = sum.and_then(|s| {
                    let var = column_var(self.ts, &p.entity, &s.tag.wire)?;
                    let name = enum_name(self.ts, var, state.get(self.ts, var, slot))?;
                    s.variants.iter().find(|v| v.value.wire == name)
                });
                let mut line = p.entity.clone();
                for field in &p.columns {
                    let required = variant.is_some_and(|v| {
                        v.fields.iter().any(|f| f.wire == field.wire && !f.nullable)
                    });
                    let value = if field.wire == p.id.wire {
                        id_text(slot)
                    } else {
                        let v = self.concrete(e, slot, field, field.nullable && !required, state);
                        if v == "~" && required {
                            default_value(self.domain, &field.ty)
                        } else {
                            v
                        }
                    };
                    line.push(' ');
                    line.push_str(&value);
                }
                lines.push(line);
            }
        }
        lines
    }

    /// The state as the harness abstracts a store: one line per present row with its tracked
    /// columns, per run mark, and per remembered raise, sorted.
    fn abstract_state(&self, port: &PortDecl, state: &StateSlice) -> Vec<String> {
        let ts = self.ts;
        let mut lines = Vec::new();
        for p in &port.entities {
            let Some(e) = self.entity(&p.entity) else {
                continue;
            };
            for slot in 0..ts.slots(e) {
                if !state.is_present(ts, e, slot) {
                    continue;
                }
                let mut line = format!("{} {}", p.entity, id_text(slot));
                for (field, how) in &p.tracked {
                    let value = match how {
                        Track::Presence => {
                            let var = column_var(ts, &p.entity, &field.wire)
                                .expect("a presence column has a var");
                            if state.get(ts, var, slot) == Value::NULL {
                                "~".to_string()
                            } else {
                                "+".to_string()
                            }
                        }
                        Track::Exact => self.concrete(e, slot, field, true, state),
                    };
                    line.push_str(&format!(" {}={value}", field.wire));
                }
                lines.push(line);
            }
        }
        for r in &port.runs {
            let Some(e) = self.entity(&r.entity) else {
                continue;
            };
            let Some(var) = ts.var_ids().find(|v| {
                matches!(&ts.var(*v).kind, VarKind::Run { entity, usecase }
                    if *entity == e && *usecase == r.usecase)
            }) else {
                continue;
            };
            for slot in 0..ts.slots(e) {
                let v = state.get(ts, var, slot);
                if v.0 > 0 {
                    lines.push(format!(
                        "mark {} {} {} {}",
                        r.usecase,
                        r.entity,
                        id_text(slot),
                        v.0
                    ));
                }
            }
        }
        for ev in &port.events {
            for (field, tracked) in &ev.ids {
                if !tracked {
                    continue;
                }
                let (Some(e), Some(var)) = (
                    self.entity(&field.entity),
                    raised_var(ts, &field.entity, &ev.ty),
                ) else {
                    continue;
                };
                for slot in 0..ts.slots(e) {
                    if state.get(ts, var, slot) == Value::TRUE {
                        lines.push(format!(
                            "raised {} {} {}",
                            ev.ty,
                            field.entity,
                            id_text(slot)
                        ));
                    }
                }
            }
        }
        lines.sort();
        lines.dedup();
        lines
    }

    /// The calls the model refuses in `state`: a method over one existing row, on each
    /// present row where no move runs it (`moves`) and the functions allow none.
    fn refusals(
        &self,
        port: &PortDecl,
        state: &StateSlice,
        moves: impl Fn(usize, u8) -> bool,
    ) -> Vec<(String, Vec<String>)> {
        let ts = self.ts;
        let mut out = Vec::new();
        for m in &port.methods {
            let Some((act, action)) = ts
                .actions()
                .iter()
                .enumerate()
                .find(|(_, a)| a.name == m.action)
            else {
                continue;
            };
            let [param] = action.params.as_slice() else {
                continue;
            };
            if param.role != ParamRole::Existing {
                continue;
            }
            let e = param.entity;
            let fns: Vec<&FnDecl> = self
                .domain
                .fns
                .iter()
                .filter(|f| f.action == m.action)
                .collect();
            for slot in 0..ts.slots(e) {
                if !state.is_present(ts, e, slot) || moves(act, slot as u8) {
                    continue;
                }
                let entity = ts.entity(e).id.clone();
                let allowed = fns.iter().any(|f| {
                    let rows = Rows::new(
                        ts,
                        self.model,
                        state,
                        vec![(entity.clone(), slot)],
                        f.elems.iter().map(|x| x.entity.clone()).collect(),
                    );
                    !matching(std::slice::from_ref(*f), &f.action, &rows).is_empty()
                        && rows.allows(f)
                });
                if allowed {
                    continue;
                }
                let mut args = vec![id_text(slot)];
                if let Some((_, ty)) = &m.event {
                    let event = self.domain.events.iter().find(|x| x.name == *ty);
                    for _ in event.map_or(&[][..], |x| &x.fields[..]) {
                        args.push(id_text(slot));
                    }
                }
                for input in &m.inputs {
                    args.push(default_input(self.domain, input));
                }
                for (_, tag_enum) in &m.choices {
                    args.push(first_value(self.domain, tag_enum));
                }
                out.push((m.action.clone(), args));
            }
        }
        out
    }
}

fn default_input(domain: &DomainModel, input: &FnInput) -> String {
    if input.nullable {
        "~".to_string()
    } else {
        default_value(domain, &input.ty)
    }
}

/// A value of a type the model leaves open, as the traces spell it.
fn default_value(domain: &DomainModel, ty: &FieldType) -> String {
    match ty {
        FieldType::Scalar(Scalar::Int) => "0".to_string(),
        FieldType::Scalar(Scalar::Bool) => "false".to_string(),
        FieldType::Scalar(Scalar::String) => "s".to_string(),
        FieldType::Scalar(Scalar::Money | Scalar::Decimal) => "0".to_string(),
        FieldType::Scalar(Scalar::Date) => "2026-01-01".to_string(),
        FieldType::Scalar(Scalar::DateTime) => "2026-01-01T00:00:00Z".to_string(),
        FieldType::Enum(name) => first_value(domain, name),
        FieldType::Id(_) => id_text(0),
        FieldType::Rows(_) => unreachable!("a list of rows is no port argument"),
    }
}

fn first_value(domain: &DomainModel, enum_ty: &str) -> String {
    domain
        .enums
        .iter()
        .find(|e| e.name == enum_ty)
        .and_then(|e| e.values.first())
        .map(|v| v.wire.clone())
        .unwrap_or_default()
}
