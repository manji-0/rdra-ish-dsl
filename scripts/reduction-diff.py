#!/usr/bin/env python3
"""Differential test of the sub-lifecycle reduction (#74, #102).

Generates random models from seeds and runs `verify --deny-undecided` on each twice: as
users run it, and with RDRA_ISH_NO_REDUCE set, which keeps every cone unreduced. The two
runs must agree on the exit code, the finding names, the verdict of each check, and the
state-model summary. State counts and traces are not compared: a reduced cone has fewer
states, and a lifted trace can take another path to the same violation. Where one run leaves
a check or a state-model finding undecided at a bound (a cone past the state cap or the memory
limit, a property past the budget) and the other decides it, the seed is listed under
`plain_undecided` or `reduced_undecided` and compared on what both decided: a finding one run
prints must be one the other prints or leaves open.

With `--with-tlc` both runs pass `--with-tlc`, so TLC runs every cone beside the native
checker, and every check on which the two engines differ (an `error[EngineDisagreement]`
line, or an `error[LassoFault]` line for a native counterexample that fails its own check)
is recorded. Such a seed is listed under `disagreed`, its model copied to
`<work>/disagreed/seed<N>.rdra`, whatever the reduced and unreduced runs compare to. So is a
seed whose reduced run prints that an extended sub-lifecycle failed its stub check (claim
A8), with or without TLC: the run decides that cone unreduced, so the verdicts agree.

  python3 -I scripts/reduction-diff.py run BIN --seeds 0:40 [--jobs N] [--fail-on-timeout] [--with-tlc] [--counted]
      [--stubs] [--min-compared N] [--min-reduced N] [--min-stub-fold N] [--min-stub-rise N] [--min-up-to-h N]
  (--seeds @FILE reads the seeds and ranges from FILE, as scripts/reduction-diff-pr-seeds.txt)
  (--timeout S, --work DIR and --symmetry MODE go before the subcommand)
  python3 -I scripts/reduction-diff.py gen SEED [--counted] [--stubs]

`--counted` puts `.counted(k)`, k of 1 or 2, on every `triggers` line the seed draws (#220),
so the reduced and unreduced runs also compare a model whose handler runs once per raise.
`--stubs` draws each seed's model from `gen_stubs` instead: a cone whose reduction binds a
param on a stub to any slot (claims A8, T9), which `gen`'s models draw rarely. The reduced
run prints a `reduction-shape:` line per reduction it builds (`RDRA_ISH_REDUCTION_SHAPE_STATS`),
and the summary counts the seeds whose reduction bound a fold's T param on a stub other than
its key (`stub_fold`), bound a rise's (`stub_rise`), nested a reduction (`nested`), or ran S
up to `H` (`up_to_h`, under `--symmetry always` with `RDRA_ISH_FORCE_S_UP_TO_H` in a `shadow`
build). `--min-stub-fold`, `--min-stub-rise` and `--min-up-to-h` fail a run with fewer.
  python3 -I scripts/reduction-diff.py compare BIN MODEL.rdra [--instance-count N] [--with-tlc]
  python3 -I scripts/reduction-diff.py minimize BIN MODEL.rdra OUT.rdra [--instance-count N] [--with-tlc]
  python3 -I scripts/reduction-diff.py selfcheck
  python3 -I scripts/reduction-diff.py lemma6 --seeds 0:40 [--counted] [--min-reduced N] [--profile P]

`run --with-tlc` needs `tlc` on PATH (run it under `nix develop`); without it no run starts
TLC. It writes each seed's model and outputs under `--work` (default target/reduction-diff),
copies a diverging model to `<work>/diverged/seed<N>.rdra`, and ends with one JSON summary
line. A seed whose run passes `--timeout` is listed under `timed_out` with the side that
timed out first (the unreduced run is skipped after the reduced one times out), never
dropped silently. The exit code is 1 when any seed diverges or disagrees, times out under
`--fail-on-timeout`, or fails to load (the generator wrote a model `check` rejects, as after
a syntax change), and when fewer seeds than `--min-compared` (default 1) were compared or
fewer than `--min-reduced` (default 0) reduced a cone: a run that compared nothing, or
reduced nothing, is not a pass.

`lemma6` checks Lemma 6 of docs/theory/cones.md step by step on each seed's model, at its
instance count, on every reduced cone whose unreduced cone completes (`reduce/lemma6.rs`).
It writes the models to `<work>/lemma6/` and runs the ignored core test
`generated_models_step_as_their_cones_step` on them (`cargo test --profile P`, `release` by
default, from this tree), which prints a line per seed. The exit code is 1 when a seed fails the check, and when
fewer seeds than `--min-reduced` (default 1) reduced a cone.

`minimize` shrinks a diverging or disagreeing model by deleting lines (ddmin) while it
still does. `selfcheck` runs the comparison on the saved output pairs under
scripts/reduction-diff-fixtures and checks each gets the status its `expect` file names.
"""

import argparse
import json
import os
import random
import re
import shutil
import signal
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path


def gen(seed, counted=False):
    """The model for a seed, and the instance count to verify it at. With `counted`, each
    `triggers` line counts its raises, with a k drawn from a third generator."""
    r = random.Random(seed)
    instances = r.choice([2, 2, 3])
    # Three instances of larger models pass most timeouts.
    n = r.choice([3, 3, 3, 4, 4, 5] if instances == 2 else [3])
    out = []
    ents = []
    small = instances == 3
    timed = set(r.sample(range(n), r.choice([0, 1] if small else [0, 0, 1, 1, 2])))
    varied = set(r.sample(range(n), r.choice([0, 0, 1] if small else [0, 0, 1, 1, 2])))
    for i in range(n):
        if i in varied:
            out.append(f'variation V{i} "v{i}" {{\n  a "a"\n  b "b"\n  c "c"\n}}')
    for i in range(n):
        k = r.choice([2, 2, 3] if small else [2, 2, 3, 3, 4])
        vals = [f"s{j}" for j in range(k)]
        ents.append(dict(vals=vals, flag=r.random() < 0.4, due=i in timed, kind=i in varied))
        e = ents[-1]
        out.append(
            f'entity E{i} "e{i}" {{\n  id: Int @pk\n'
            f'  status: Enum({", ".join(vals)}) @default({vals[0]})'
        )
        if e["flag"]:
            out.append("  flag: Bool @default(false)")
        if e["due"]:
            out.append("  due: Date")
        if e["kind"]:
            out.append(f"  kind: V{i}")
        out.append("}\n")
    parent = {}
    for i in range(1, n):
        parent[i] = i - 1 if r.random() < 0.7 else r.randrange(0, i)
        out.append(f"relate(E{i}, E{parent[i]}, N:1)" + (".optional()" if r.random() < 0.3 else ""))
    out.append("")
    cidx = [0]

    def kids_of(i):
        return [j for j in range(1, n) if parent[j] == i]

    def mkcond(ei, allow_q=True):
        name = f"C{cidx[0]}"
        cidx[0] += 1
        e = ents[ei]
        sub = r.sample(e["vals"], r.randint(1, max(1, len(e["vals"]) - 1)))
        if len(sub) == 1:
            body = f"E{ei}.status == {sub[0]}"
        else:
            body = f"E{ei}.status in ({', '.join(sub)})"
        if e["kind"] and r.random() < 0.3:
            body = f"E{ei}.kind == {r.choice('abc')}"
        suffix = ""
        kids = kids_of(ei)
        if allow_q and kids and r.random() < 0.5:
            j = r.choice(kids)
            q = r.choice(["has", "none"])
            if r.random() < 0.6:
                suffix = f".{q}(E{j}.status == {r.choice(ents[j]['vals'])})"
            else:
                suffix = f".{q}(E{j})"
        out.append(f'condition {name} "{name}"')
        out.append(f"holds({name}, {body}){suffix}")
        return name

    creates = {}
    for i in range(n):
        nm = f"New{i}"
        out.append(f'usecase {nm} "{nm}"')
        out.append(f"creates({nm}, E{i})")
        creates[i] = nm
    arms = {}
    for i in range(n):
        for a in range(r.randint(1, 3)):
            nm = f"T{i}_{a}"
            out.append(f'usecase {nm} "{nm}"')
            out.append(f"updates({nm}, E{i})")
            arms.setdefault(i, []).append(nm)
    out.append("")
    for i in range(n):
        e = ents[i]
        plan = {nm: [] for nm in arms[i]}
        v = e["vals"]
        for j in range(len(v) - 1):
            plan[r.choice(arms[i])].append((v[j], v[j + 1]))
        for _ in range(r.randint(0, 2)):
            a, b = r.sample(v, 2)
            plan[r.choice(arms[i])].append((a, b))
        for nm in arms[i]:
            if not plan[nm]:
                plan[nm].append(tuple(r.sample(v, 2)))
            for a, b in plan[nm]:
                line = f"transitions({nm}, E{i}.status, {a} -> {b})"
                if r.random() < 0.3:
                    cands = [j for j in range(n) if j == parent.get(i, -9) or parent.get(j, -9) == i or j == i]
                    line += f".when({mkcond(r.choice(cands))})"
                out.append(line)
            if e["flag"] and r.random() < 0.4:
                out.append(
                    f"sets({nm}, E{i}.flag == true)"
                    + (f".when({mkcond(i, False)})" if r.random() < 0.3 else "")
                )
            if r.random() < 0.4:
                cands = [j for j in range(n) if j == parent.get(i, -9) or parent.get(j, -9) == i]
                if cands:
                    out.append(f"applies_to({mkcond(r.choice(cands))}, {nm})")
        if e["kind"]:
            nm = r.choice(arms[i])
            if r.random() < 0.5:
                out.append(f"chooses({nm}, E{i}.kind)")
            else:
                for val in r.sample("abc", r.randint(1, 3)):
                    line = f"chooses({nm}, E{i}.kind, {val})"
                    if r.random() < 0.4:
                        line += f".when({mkcond(i, False)})"
                    out.append(line)
        if e["due"]:
            # A deadline set by a step, and a timer's use case that waits for it.
            sched = r.choice(arms[i] + [creates[i]])
            days = r.sample(range(1, 5), r.randint(1, 2))
            for k in days:
                line = f"sets({sched}, E{i}.due == {k})"
                # Two unguarded values of one column conflict; guarded ones are a choice.
                if len(days) > 1 or r.random() < 0.5:
                    line += f".when({mkcond(i, False)})"
                out.append(line)
            exp = f"Exp{i}"
            out.append(f'usecase {exp} "{exp}"')
            out.append(f'timer Tm{i} "tm{i}"')
            out.append(f"triggers(Tm{i}, {exp})")
            out.append(f"updates({exp}, E{i})")
            out.append(f'condition Due{i} "due{i}"')
            out.append(f"holds(Due{i}, E{i}.due {r.choice(['<=', '<'])} now)")
            a, b = r.sample(v, 2)
            out.append(f"transitions({exp}, E{i}.status, {a} -> {b}).when(Due{i})")
            if r.random() < 0.5:
                out.append(f"forbidden(E{i}, status == {b}, due > now)")
    if r.random() < 0.5:
        i, j = r.sample(range(n), 2)
        out.append(
            'usecase S "S"\napi SA "SA"\napi SB "SB"\ninvokes(S, SA)\n'
            f"updates(SA, E{i})\ninvokes(S, SB)\nupdates(SB, E{j})"
        )
        for q in (i, j):
            a, b = r.sample(ents[q]["vals"], 2)
            out.append(f"transitions(S, E{q}.status, {a} -> {b})")
        for suf in r.sample(["1", "2"], r.randint(1, 2)):
            k = r.randrange(n)
            a, b = r.sample(ents[k]["vals"], 2)
            out.append(f'usecase S_{suf} "S_{suf}"\nupdates(S_{suf}, E{k})\ntransitions(S_{suf}, E{k}.status, {a} -> {b})')
        if r.random() < 0.5:
            out.append(f"applies_to({mkcond(r.choice([i, j]))}, S)")
    for i in range(1, n):
        if r.random() < 0.5:
            out.append(f"applies_to({mkcond(parent[i])}, {creates[i]})")
    out.append("")
    for _ in range(r.randint(1, 3)):
        k = r.random()
        i = r.randrange(n)
        e = ents[i]
        last = lambda a: a if a != e["vals"][0] else e["vals"][-1]
        if k < 0.3 and i >= 1:
            p = parent[i]
            a, b = r.choice(ents[i]["vals"]), r.choice(ents[p]["vals"])
            out.append(f"forbidden(E{p}, E{i}, E{p}.status == {b}, E{i}.status == {a}).along(E{p}, E{i})")
        elif k < 0.5 and e["flag"]:
            out.append(f"forbidden(E{i}, status == {r.choice(e['vals'][1:])}, flag == true)")
        elif k < 0.75:
            a = r.choice(e["vals"])
            kids = kids_of(i)
            if kids and r.random() < 0.6:
                j = r.choice(kids)
                q = r.choice(["has", "none"])
                out.append(f"invariant(E{i}).when(status == {a}).{q}(E{j}.status == {r.choice(ents[j]['vals'])})")
            else:
                out.append(f"forbidden(E{i}, status == {last(a)})")
        else:
            a = r.choice(e["vals"])
            kids = kids_of(i)
            if kids:
                j = r.choice(kids)
                out.append(f"invariant(E{i}).when(status == {a}).none(E{j}.status == {r.choice(ents[j]['vals'])})")
            else:
                out.append(f"forbidden(E{i}, status == {last(a)})")
    for _ in range(r.choice([0, 0, 1, 2])):
        i = r.randrange(n)
        out.append(f"after({r.choice(arms[i])}).assert(E{i}.status == {r.choice(ents[i]['vals'])})")
    for p in range(r.choice([0, 1, 1, 2])):
        i = r.randrange(n)
        e = ents[i]
        a, b = r.choice(e["vals"]), r.choice(e["vals"])
        kind = r.choice(["leads", "leads", "ev", "alw"])
        if kind == "leads":
            f = f"leads_to(E{i}.status == {a}, E{i}.status == {b})"
            if r.random() < 0.5:
                f += f".fair({', '.join(r.sample(arms[i], r.randint(1, len(arms[i]))))})"
        elif kind == "ev":
            f = f"eventually(E{i}.status == {a})"
        else:
            f = f"always(E{i}.status != {a})"
        out.append(f"property P{p}\n  {f}")
    # Events (#112): a raiser on one side of a relation and a handler on the other, so the
    # handler reads the raiser's mark across the split. Drawn from a second generator, so
    # every seed keeps the model it had before events.
    re_ = random.Random(seed * 7919 + 1)
    rc = random.Random(seed * 104729 + 2)
    for k in range(re_.choice([0, 1, 1, 2]) if n >= 2 else 0):
        j = re_.randrange(1, n)
        i = parent[j]
        src, dst = (j, i) if re_.random() < 0.6 else (i, j)
        raiser = re_.choice(arms[src])
        handler = re_.choice(arms[dst] + ([creates[dst]] if dst == j else []))
        count = f".counted({rc.choice([1, 2])})" if counted else ""
        out.append(f'event Ev{k} "ev{k}"\nraises({raiser}, Ev{k})\ntriggers(Ev{k}, {handler}){count}')
    return "\n".join(out) + "\n", instances


def gen_stubs(seed, counted=False):
    """A model for a seed whose cone reduces with a param on a stub (claims A8, T9), and the
    instance count to verify it at. Order O is the root of a T side that reads its child
    Voucher V, which an S action Use moves; V's owner keeps O in the extended sub-lifecycle
    as a stub, absent in every state of it. Use waits on an issued Ticket K, which T does not
    read, and on rows that only these actions reach:

    - Issue, which moves K, as the handler of an event an O step raises: it has a
      `Triggered` param on O beside its key, a folded S action with a param on the stub;
    - a T action Note, guarded by O's status or by `O.hold`, which Hold and Release set, that
      creates Sheet Sh under O, whose existence Use reads: a rise with a `Parent` param on
      the stub, or an `Existing` one when Note also clears `O.hold`.

    A seed draws one or both (without the first, Issue is a plain S action),
    the value counts, extra steps, a chain under K or V that the cone's S can reduce again
    (nested reductions), and the check. With `counted`, Issue does not fold, so the seeds
    keep only the rise.
    Drawn from a generator of its own, so `gen`'s seeds keep their models."""
    r = random.Random(seed * 15485863 + 3)
    instances = r.choice([1, 2, 2, 2, 2])
    shapes = [s for s, p in (("trigger", 0.75), ("rise", 0.6)) if r.random() < p] or [r.choice(["trigger", "rise"])]
    out = []
    ents = {}

    def entity(name, vals, extra=""):
        ents[name] = vals
        out.append(
            f'entity {name} "{name.lower()}" {{ id: Int @pk  status: Enum({", ".join(vals)}) '
            f"@default({vals[0]}){extra} }}"
        )

    def vals(lo, hi):
        return [f"s{j}" for j in range(r.randint(lo, hi))]

    note_writes_o = "rise" in shapes and r.random() < 0.4
    hold = note_writes_o or ("rise" in shapes and r.random() < 0.3)
    entity("O", vals(3, 4), "  hold: Bool @default(false)" if hold else "")
    entity("V", vals(2, 3))
    out.append("relate(V, O, N:1)")
    entity("K", vals(2, 3))
    k_under = r.choice([None, None, None, "V", "V", "V", "O"])
    if k_under:
        out.append(f"relate(K, {k_under}, N:1)" + (".optional()" if r.random() < 0.5 else ""))
    if "rise" in shapes:
        entity("Sh", vals(1, 2))
        out.append("relate(Sh, O, N:1)")
    chains = []
    for top in r.sample(["K", "V"], r.choice([0, 0, 0, 1])):
        name = f"W{top}"
        entity(name, vals(2, 3))
        out.append(f"relate({name}, {top}, N:1)")
        chains.append((name, top))
    out.append("")
    cidx = [0]

    def cond(body):
        name = f"C{cidx[0]}"
        cidx[0] += 1
        out.append(f'condition {name} "{name}"\nholds({name}, {body})')
        return name

    def step(uc, e, a, b, when=None):
        out.append(f"transitions({uc}, {e}.status, {a} -> {b})" + (f".when({when})" if when else ""))

    def walk(e, prefix, when=None):
        """Use cases that move `e` along its values, one per step, the last guarded by `when`."""
        v = ents[e]
        for j in range(len(v) - 1):
            uc = f"{prefix}{j}"
            out.append(f'usecase {uc} "{uc}"\nupdates({uc}, {e})')
            step(uc, e, v[j], v[j + 1], when if j == len(v) - 2 else None)
        return [f"{prefix}{j}" for j in range(len(v) - 1)]

    o, v, k = ents["O"], ents["V"], ents["K"]
    out.append('usecase Place "place"\ncreates(Place, O)')
    o_steps = walk("O", "Go")
    # The last step of O waits on a used voucher: the T side reads S.
    used = cond(f"V.status == {v[-1]}")
    out.append(f"applies_to({used}, {o_steps[-1]})")
    out.append('usecase Grant "grant"\ncreates(Grant, V)')
    out.append('usecase Draft "draft"\ncreates(Draft, K)')
    out.append('usecase Issue "issue"\nupdates(Issue, K)')
    step("Issue", "K", k[0], k[-1])
    use_guards = [cond(f"K.status == {k[-1]}")]
    if "trigger" in shapes:
        raiser = r.choice(o_steps[:-1] or o_steps)
        count = f".counted({r.choice([1, 2])})" if counted else ""
        out.append(f'event Ev "ev"\nraises({raiser}, Ev)\ntriggers(Ev, Issue){count}')
    if "rise" in shapes:
        out.append('usecase Note "note"\ncreates(Note, Sh)')
        if note_writes_o:
            out.append("updates(Note, O)\nsets(Note, O.hold == false)")
        if hold:
            out.append(f"applies_to({cond('O.hold == true')}, Note)")
        else:
            out.append(f"applies_to({cond(f'O.status == {r.choice(o[1:-1] or o)}')}, Note)")
        use_guards.append(cond(f"Sh.status == {r.choice(ents['Sh'])}"))
    if hold:
        out.append('usecase Hold "hold"\nupdates(Hold, O)\nsets(Hold, O.hold == true)')
        if r.random() < 0.7:
            out.append('usecase Release "release"\nupdates(Release, O)\nsets(Release, O.hold == false)')
    out.append('usecase Use "use"\nupdates(Use, V)')
    step("Use", "V", v[0], v[-1])
    for g in use_guards:
        out.append(f"applies_to({g}, Use)")
    for e in ("V", "K"):
        if r.random() < 0.4 and len(ents[e]) > 2:
            uc = f"Back{e}"
            out.append(f'usecase {uc} "{uc}"\nupdates({uc}, {e})')
            a, b = r.sample(ents[e], 2)
            step(uc, e, a, b)
    for name, top in chains:
        walk(name, f"G{name}")
        out.append(f'usecase New{name} "new{name}"\ncreates(New{name}, {name})')
        # The chain's last value gates a step of its parent, which stays inside S.
        gate = cond(f"{name}.status == {ents[name][-1]}")
        uc = f"Up{top}"
        out.append(f'usecase {uc} "{uc}"\nupdates({uc}, {top})')
        a, b = r.sample(ents[top], 2)
        step(uc, top, a, b, gate)
    out.append("")
    kind = r.choice(["forbidden", "forbidden", "invariant", "leads", "always"])
    if kind == "forbidden":
        out.append(f"forbidden(O, status == {o[-1]})")
    elif kind == "invariant":
        out.append(f"invariant(O).when(status == {o[-1]}).none(V.status == {v[0]})")
    elif kind == "leads":
        out.append(f"property P0\n  leads_to(O.status == {o[1]}, O.status == {o[-1]})")
    else:
        out.append(f"property P0\n  always(O.status != {o[-1]})")
    return "\n".join(out) + "\n", instances


def sh(cmd, timeout, env=None, cwd=None):
    """(exit code, stdout+stderr), or ("timeout", "") with the process group killed."""
    p = subprocess.Popen(
        cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
        env=env, cwd=cwd, start_new_session=True,
    )
    try:
        out, _ = p.communicate(timeout=timeout)
        return p.returncode, out
    except subprocess.TimeoutExpired:
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        p.communicate()
        return "timeout", ""


VERDICT = re.compile(r"^\S+ \[([^\]]*)\]: (ok|counterexample|tool_error|not decided)\b", re.M)


# A check the native checker and TLC decide differently, or a native counterexample that
# fails its own check: a tool bug, recorded apart from the comparison of the two runs.
ENGINE_BUG = re.compile(r"^error\[(?:EngineDisagreement|LassoFault)\]: (.*)$", re.M)

# An extended sub-lifecycle that failed the stub check of claim A8, a tool bug the run then
# hides by deciding the cone unreduced.
STUB_FAULT = re.compile(r"^warning: (the extended sub-lifecycle of a cone failed its stub check.*)$", re.M)


# A state-model finding a cone left open at a bound (the state cap, the pattern cap, or the
# memory limit), as `Undecided` prints it; a scope's `--buc` or `--entity` is not a bound.
OPEN_FINDING = re.compile(
    r"^\s*(?:state (?P<w>\S+) = (?P<wv>\S+)"
    r"|whether (?P<r>\S+) can run"
    r"|whether (?P<a>\S+) can take \S+ (?P<af>\S+) -> (?P<at>\S+)"
    r"|whether (?P<c>\S+) can choose (?P<cc>\S+) = (?P<cv>\S+)"
    r"|whether some (?P<d>\S+) can get stuck at (?P<dc>\S+) = (?P<dv>\S+))"
    r" is not decided: the \S+ cone stopped at the ",
    re.M,
)


def open_name(m):
    """The name the finding `m` leaves open prints when it is decided, as `StateDiag::name`
    makes it."""
    g = m.groupdict()
    if g["w"]:
        raw = f"Witness_{g['w']}_{g['wv']}"
    elif g["r"]:
        raw = f"Runs_{g['r']}"
    elif g["a"]:
        raw = f"Arm_{g['a']}_{g['af']}_{g['at']}"
    elif g["c"]:
        raw = f"Choice_{g['c']}_{g['cc']}_{g['cv']}"
    else:
        raw = f"Deadlock_{g['d']}_{g['dc']}_{g['dv']}"
    return re.sub(r"[^A-Za-z0-9_]", "_", raw)


# What a run that stopped at a bound reports about the stop, not about the model: a name
# starting with one of these.
STOP_NAMES = ("StateCapReached", "PatternCapReached", "MemoryCapReached", "TimeBudgetReached")


def observe(rc, out):
    """What both runs must agree on, and the engine bugs each run reports."""
    disagreements = ENGINE_BUG.findall(out) + STUB_FAULT.findall(out)
    out = STUB_FAULT.sub("", ENGINE_BUG.sub("", out))
    names = sorted(set(re.findall(r"^(?:error|warning|info)\[([^\]]+)\]", out, re.M)))
    opened = sorted({open_name(m) for m in OPEN_FINDING.finditer(out)})
    # A cone's line gives the cone's outcome, and the cones group checks differently when
    # reduced; a check of a cone with a counterexample is violated when a finding names it.
    checks = {}
    for m in VERDICT.finditer(out):
        for check in m.group(1).split(", "):
            verdict = m.group(2)
            if verdict in ("ok", "counterexample"):
                verdict = "violated" if check in names else "holds"
            # A property the cone leaves undecided runs again on the whole model; the later
            # line is the verdict.
            checks[check] = verdict
    keep = ("state models", "not checked", "result:", "tlc_result", "OK:", "verification ", "Error: verification")
    lines = [l for l in out.splitlines() if l.startswith(keep)]
    return dict(rc=rc, names=names, checks=checks, lines=lines, opened=opened, disagreements=disagreements)


def disagreed(sides):
    return any(o != "timeout" and o["disagreements"] for o in sides.values())


def undecided(o):
    return (
        bool(o["opened"])
        or any(l.startswith("not checked") for l in o["lines"])
        or "not decided" in o["checks"].values()
    )


def agreement(red, plain):
    """"same"; "plain_undecided" or "reduced_undecided" when they agree on all both runs
    decided, and that side left something open at a bound (the state cap, the memory limit,
    the budget) that the other decided; or "diverged". A check is compared where both decide
    it, and a finding one side prints must be one the other prints or left open. The engine
    bugs are compared by `disagreed`, not here."""
    red, plain = (dict(o, disagreements=[]) for o in (red, plain))
    if red == plain:
        return "same"
    if not undecided(red) and not undecided(plain):
        return "diverged"
    for c in set(red["checks"]) | set(plain["checks"]):
        r, p = red["checks"].get(c), plain["checks"].get(c)
        if r != p and "not decided" not in (r, p):
            return "diverged"
    for this, other in ((red, plain), (plain, red)):
        # A check the other side left open may print its violation or vacuity here.
        open_checks = {c for c, v in other["checks"].items() if v == "not decided"}
        open_checks |= {f"Vacuous_{c}" for c in open_checks}
        extra = set(this["names"]) - set(other["names"]) - set(other["opened"]) - open_checks
        if any(not n.startswith(STOP_NAMES) for n in extra):
            return "diverged"
    # The `result:` and `Error: verification` lines sum up what each side decided (a
    # failure count, or `incomplete` for what was left open), so they differ with what the
    # other side decided; the checks and findings above already compare it.
    skip = lambda l: l.startswith(("not checked", "state models", "result:", "Error: verification"))
    lines = lambda o: [l for l in o["lines"] if not skip(l)]
    if lines(red) != lines(plain):
        return "diverged"
    # The state-model summary counts what was decided: the side that left nothing open
    # counts at least as many witnessed states, deadlocks and use cases that never run,
    # over the same cones.
    counts = lambda o: [
        [int(n) for n in re.findall(r"\d+", l)] for l in o["lines"] if l.startswith("state models")
    ]
    if len(counts(red)) != 1 or len(counts(plain)) != 1:
        return "diverged"
    r, p = counts(red)[0], counts(plain)[0]
    if len(r) != 5 or r[:2] != p[:2]:
        return "diverged"
    if not undecided(red) and any(a < b for a, b in zip(r[2:], p[2:])):
        return "diverged"
    if not undecided(plain) and any(a > b for a, b in zip(r[2:], p[2:])):
        return "diverged"
    return "plain_undecided" if undecided(plain) else "reduced_undecided"


def has_reduced_cone(d):
    return d.is_dir() and any("S_class" in f.read_text() for f in d.glob("*.tla"))


# `verify --symmetry` for both runs, when `--symmetry` is given.
SYMMETRY = None


def verify_both(bin_, model, instances, timeout, work, with_tlc=False):
    """Each side's observation, or "timeout"; and whether the users' run reduced a cone."""
    sides = {}
    for side in ("reduced", "plain"):
        shutil.rmtree(work / side, ignore_errors=True)
    sides_env = (("reduced", {"RDRA_ISH_REDUCTION_SHAPE_STATS": "1"}), ("plain", {"RDRA_ISH_NO_REDUCE": "1"}))
    for side, extra in sides_env:
        d = work / side
        env = {k: v for k, v in os.environ.items() if k not in ("RDRA_ISH_NO_REDUCE", "RDRA_ISH_REDUCTION_SHAPE_STATS")}
        env.update(extra)
        cmd = [bin_, "verify", "--deny-undecided", "--instance-count", str(instances), "-o", str(d), str(model)]
        if SYMMETRY:
            cmd += ["--symmetry", SYMMETRY]
        if with_tlc:
            cmd.append("--with-tlc")
        rc, out = sh(cmd, timeout, env=env, cwd=work)
        (work / f"{side}.out").write_text(out)
        sides[side] = "timeout" if rc == "timeout" else observe(rc, out)
        if rc == "timeout":
            # The unreduced run explores at least what the reduced one does.
            break
    if has_reduced_cone(work / "plain"):
        sys.exit("RDRA_ISH_NO_REDUCE did not keep the cones unreduced: is BIN this tree's build?")
    return sides, has_reduced_cone(work / "reduced")


SHAPE = re.compile(r"^reduction-shape: (.*)$", re.M)


def shapes(out):
    """What the reduced run's `reduction-shape:` lines say, one per reduction built (nested
    ones included): the deepest nesting, whether S ran up to `H`, and the params on a stub
    bound to any slot, summed per kind. A line a panic's message cut short counts the
    fields it kept."""
    got = dict(depth=0, up_to_h=False)
    for line in SHAPE.findall(out):
        fields = dict(re.findall(r"(\w+)=(\d+|true|false)\b", line))
        got["depth"] = max(got["depth"], int(fields.pop("depth", 0)))
        got["up_to_h"] |= fields.pop("up_to_h", "") == "true"
        for name, n in fields.items():
            got[name] = got.get(name, 0) + int(n)
    return got


# The shapes the summary counts seeds by: a fold's T param on a stub other than its key, a
# rise's param on a stub, a nested reduction, and S run up to `H`.
SHAPES = dict(
    stub_fold=lambda g: any(g.get(f"fold_{r}", 0) for r in ("existing", "parent", "triggered")),
    stub_rise=lambda g: any(g.get(f"rise_{r}", 0) for r in ("existing", "parent", "triggered")),
    nested=lambda g: g["depth"] >= 2,
    up_to_h=lambda g: g["up_to_h"],
)


def loads(bin_, model, timeout, work):
    rc, out = sh([bin_, "check", str(model)], timeout, cwd=work)
    return rc != "timeout" and not re.search(r"^(error(?!\[)|Error)", out, re.M), out


def one(args, seed):
    start = time.monotonic()
    res = dict(compare_seed(args, seed), seconds=round(time.monotonic() - start, 1))
    print(f"seed {seed}: {res['status']} ({res['seconds']} s)", file=sys.stderr, flush=True)
    return res


def compare_seed(args, seed):
    work = Path(args.work) / f"seed{seed}"
    work.mkdir(parents=True, exist_ok=True)
    text, instances = (gen_stubs if args.stubs else gen)(seed, args.counted)
    model = work / "m.rdra"
    model.write_text(text)
    res = dict(seed=seed, instances=instances)
    ok, out = loads(args.bin, model, args.timeout, work)
    if not ok:
        return dict(res, status="load_error", detail=out[:300])
    sides, reduced = verify_both(args.bin, model, instances, args.timeout, work, args.with_tlc)
    res["reduced"] = reduced
    red_out = work / "reduced.out"
    res["shape"] = shapes(red_out.read_text()) if red_out.exists() else shapes("")
    if disagreed(sides):
        bad = Path(args.work) / "disagreed"
        bad.mkdir(exist_ok=True)
        shutil.copy(model, bad / f"seed{seed}.rdra")
        found = {s: o["disagreements"] for s, o in sides.items() if o != "timeout" and o["disagreements"]}
        return dict(res, status="disagreed", disagreements=found)
    late = [s for s, o in sides.items() if o == "timeout"]
    if late:
        return dict(res, status="timeout", side=late[0])
    status = agreement(sides["reduced"], sides["plain"])
    if status == "diverged":
        div = Path(args.work) / "diverged"
        div.mkdir(exist_ok=True)
        shutil.copy(model, div / f"seed{seed}.rdra")
        return dict(res, status=status, **sides)
    return dict(res, status=status)


def seed_list(spec):
    """`A:B` (B excluded), or seeds and ranges joined by commas; `@FILE` reads the same from
    FILE, with commas or newlines between the parts and `#` starting a comment."""
    if spec.startswith("@"):
        lines = Path(spec[1:]).read_text().splitlines()
        spec = ",".join(p for l in lines for p in l.split("#")[0].replace(",", " ").split())
    seeds = []
    for part in spec.split(","):
        a, _, b = part.partition(":")
        seeds.extend(range(int(a), int(b)) if b else [int(a)])
    return seeds


def cmd_run(args):
    if args.with_tlc and shutil.which("tlc") is None:
        sys.exit("tlc is not on PATH; run under `nix develop`")
    args.bin = str(Path(args.bin).resolve())
    Path(args.work).mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(args.jobs) as ex:
        results = list(ex.map(lambda s: one(args, s), seed_list(args.seeds)))
    by = lambda st: [x for x in results if x["status"] == st]
    compared = by("same") + by("plain_undecided") + by("reduced_undecided") + by("diverged")
    for x in by("diverged"):
        print(f"seed {x['seed']} diverges at {x['instances']} instances:")
        for side in ("reduced", "plain"):
            print(f"  {side}: {json.dumps(x[side], ensure_ascii=False)}")
    for x in by("disagreed"):
        print(f"seed {x['seed']}: the engines disagree at {x['instances']} instances:")
        for side, lines in x["disagreements"].items():
            for line in lines:
                print(f"  {side}: {line}")
    (Path(args.work) / "results.json").write_text(json.dumps(results, indent=1, ensure_ascii=False))
    summary = dict(
        generated=len(results),
        loaded=len(results) - len(by("load_error")),
        compared=len(compared),
        reduced=sum(1 for x in compared if x["reduced"]),
        **{name: sum(1 for x in compared if has(x["shape"])) for name, has in SHAPES.items()},
        three_instances=sum(1 for x in compared if x["instances"] == 3),
        plain_undecided=[x["seed"] for x in by("plain_undecided")],
        reduced_undecided=[x["seed"] for x in by("reduced_undecided")],
        timed_out=[(x["seed"], x["side"]) for x in by("timeout")],
        load_errors=[x["seed"] for x in by("load_error")],
        diverged=[x["seed"] for x in by("diverged")],
        with_tlc=args.with_tlc,
        disagreed=[x["seed"] for x in by("disagreed")],
    )
    print(json.dumps(summary))
    for seed in summary["load_errors"][:5]:
        detail = next(x["detail"] for x in results if x["seed"] == seed)
        print(f"seed {seed} does not load: {detail.strip()}")
    short = [
        f"{what} {summary[what]} < {floor}"
        for what, floor in (
            ("compared", args.min_compared),
            ("reduced", args.min_reduced),
            ("stub_fold", args.min_stub_fold),
            ("stub_rise", args.min_stub_rise),
            ("up_to_h", args.min_up_to_h),
        )
        if summary[what] < floor
    ]
    if short:
        print("too little was checked: " + ", ".join(short))
    failed = summary["diverged"] or summary["disagreed"] or summary["load_errors"] or short
    return 1 if failed or (args.fail_on_timeout and summary["timed_out"]) else 0


def cmd_lemma6(args):
    work = Path(args.work) / "lemma6"
    shutil.rmtree(work, ignore_errors=True)
    work.mkdir(parents=True)
    seeds = seed_list(args.seeds)
    for seed in seeds:
        text, instances = gen(seed, args.counted)
        (work / f"seed{seed:05}.rdra").write_text(f"// flags: --instance-count {instances}\n{text}")
    test = "lifecycle::tests::reduce_steps::generated_models_step_as_their_cones_step"
    cmd = ["cargo", "test", "--profile", args.profile, "-p", "rdra-ish-core", "--lib", "--", "--ignored", "--exact", test, "--nocapture"]
    env = dict(os.environ, RDRA_LEMMA6_MODELS=str(work))
    root = Path(__file__).resolve().parent.parent
    p = subprocess.run(cmd, env=env, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    (work / "test.out").write_text(p.stdout)
    lines = [l for l in p.stdout.splitlines() if l.startswith("lemma6 seed")]
    by = lambda st: [l for l in lines if l.split(": ", 1)[1].startswith(st)]
    for line in by("FAIL"):
        print(line)
    reduced = [l for l in by("ok") if not re.search(r"reductions=0\b", l)]
    summary = dict(generated=len(seeds), checked=len(by("ok")) + len(by("FAIL")), load_error=len(by("load_error")),
                   reduced=len(reduced), failed=len(by("FAIL")))
    print(json.dumps(summary))
    if len(lines) != len(seeds):
        print(p.stdout[-4000:], file=sys.stderr)
        sys.exit(f"the test printed {len(lines)} lines for {len(seeds)} seeds; see {work / 'test.out'}")
    return 1 if by("FAIL") or p.returncode != 0 or len(reduced) < args.min_reduced else 0


def cmd_gen(args):
    text, instances = (gen_stubs if args.stubs else gen)(args.seed, args.counted)
    sys.stdout.write(f"// instance count {instances}\n{text}")
    return 0


def diverges(args, lines, work):
    model = work / "m.rdra"
    model.write_text("\n".join(lines) + "\n")
    if not loads(args.bin, model, args.timeout, work)[0]:
        return False
    sides, _ = verify_both(args.bin, model, args.instance_count, args.timeout, work, args.with_tlc)
    if disagreed(sides):
        return True
    return "timeout" not in sides.values() and agreement(sides["reduced"], sides["plain"]) == "diverged"


def cmd_compare(args):
    args.bin = str(Path(args.bin).resolve())
    work = Path(args.work) / "compare"
    work.mkdir(parents=True, exist_ok=True)
    model = Path(args.model).resolve()
    sides, reduced = verify_both(args.bin, model, args.instance_count, args.timeout, work, args.with_tlc)
    late = "timeout" in sides.values()
    status = "timeout" if late else agreement(sides["reduced"], sides["plain"])
    if disagreed(sides):
        status = "disagreed"
    print(json.dumps(dict(status=status, reduces_a_cone=reduced, sides=sides), ensure_ascii=False, indent=1))
    return 1 if status in ("timeout", "diverged", "disagreed") else 0


def cmd_minimize(args):
    args.bin = str(Path(args.bin).resolve())
    work = Path(args.work) / "minimize"
    work.mkdir(parents=True, exist_ok=True)
    cur = Path(args.model).read_text().splitlines()
    if not diverges(args, cur, work):
        sys.exit("the model neither diverges nor disagrees")
    chunks = 2
    while len(cur) >= 2:
        size = max(1, len(cur) // chunks)
        for i in range(0, len(cur), size):
            cand = cur[:i] + cur[i + size:]
            if diverges(args, cand, work):
                cur, chunks = cand, max(chunks - 1, 2)
                break
        else:
            if size == 1:
                break
            chunks = min(len(cur), chunks * 2)
    Path(args.out).write_text("\n".join(cur) + "\n")
    print(f"{len(Path(args.model).read_text().splitlines())} -> {len(cur)} lines: {args.out}")
    return 0


FIXTURES = Path(__file__).resolve().parent / "reduction-diff-fixtures"


def cmd_selfcheck(args):
    """`agreement` on each saved pair of outputs in FIXTURES: `<case>/reduced.out`,
    `<case>/plain.out`, and `<case>/expect`, the status the pair must get."""
    failed = 0
    for case in sorted(d for d in FIXTURES.iterdir() if d.is_dir()):
        sides = [observe(None, (case / f"{s}.out").read_text()) for s in ("reduced", "plain")]
        got, want = agreement(*sides), (case / "expect").read_text().strip()
        failed += got != want
        print(f"{'ok' if got == want else 'FAIL'}: {case.name}: {got}" + ("" if got == want else f", want {want}"))
    return 1 if failed else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--work", default="target/reduction-diff")
    ap.add_argument("--timeout", type=int, default=120, help="seconds per verify run")
    ap.add_argument("--symmetry", choices=["never", "always"], help="pass `verify --symmetry` to both runs")
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("run")
    p.add_argument("bin")
    p.add_argument("--seeds", required=True, help="A:B (B excluded), or seeds and ranges joined by commas; @FILE reads them from FILE")
    p.add_argument("--fail-on-timeout", action="store_true", help="exit 1 when a seed times out")
    p.add_argument("--min-compared", type=int, default=1, help="exit 1 when fewer seeds were compared")
    p.add_argument("--min-reduced", type=int, default=0, help="exit 1 when fewer compared seeds reduced a cone")
    p.add_argument("--min-stub-fold", type=int, default=0, help="exit 1 when fewer compared seeds bound a fold's T param on a stub")
    p.add_argument("--min-stub-rise", type=int, default=0, help="exit 1 when fewer compared seeds bound a rise's param on a stub")
    p.add_argument("--min-up-to-h", type=int, default=0, help="exit 1 when fewer compared seeds ran S up to H")
    p.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) // 2))
    p.add_argument("--with-tlc", action="store_true", help="run TLC beside the native checker and record where they differ")
    p.add_argument("--counted", action="store_true", help="count the raises of every triggers line (#220)")
    p.add_argument("--stubs", action="store_true", help="draw models whose reduction binds params on a stub (gen_stubs)")
    p = sub.add_parser("gen")
    p.add_argument("seed", type=int)
    p.add_argument("--counted", action="store_true", help="count the raises of every triggers line (#220)")
    p.add_argument("--stubs", action="store_true", help="draw models whose reduction binds params on a stub (gen_stubs)")
    sub.add_parser("selfcheck")
    p = sub.add_parser("lemma6")
    p.add_argument("--seeds", required=True, help="as for run")
    p.add_argument("--counted", action="store_true", help="count the raises of every triggers line (#220)")
    p.add_argument("--min-reduced", type=int, default=1, help="exit 1 when fewer seeds reduced a cone")
    p.add_argument("--profile", default="release", help="the cargo profile the test builds with")
    for name in ("compare", "minimize"):
        p = sub.add_parser(name)
        p.add_argument("bin")
        p.add_argument("model")
        if name == "minimize":
            p.add_argument("out")
        p.add_argument("--instance-count", type=int, default=2)
        p.add_argument("--with-tlc", action="store_true", help="run TLC beside the native checker")
    args = ap.parse_args()
    args.work = str(Path(args.work).resolve())
    global SYMMETRY
    SYMMETRY = args.symmetry
    cmds = dict(run=cmd_run, gen=cmd_gen, lemma6=cmd_lemma6, compare=cmd_compare, minimize=cmd_minimize, selfcheck=cmd_selfcheck)
    return cmds[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
