#!/usr/bin/env python3
"""Differential test of the sub-lifecycle reduction (#74, #102).

Generates random models from seeds and runs `verify --deny-undecided` on each twice: as
users run it, and with RDRA_ISH_NO_REDUCE set, which keeps every cone unreduced. The two
runs must agree on the exit code, the finding names, the verdict of each check, and the
state-model summary. State counts and traces are not compared: a reduced cone has fewer
states, and a lifted trace can take another path to the same violation. Where the unreduced
run leaves a finding undecided (a cone past the state cap) and the reduced run decides it, the
seed is listed under `plain_undecided` and compared on what the unreduced run decided.

  python3 -I scripts/reduction-diff.py run BIN --seeds 0:40 [--jobs N] [--fail-on-timeout]
  (--timeout S and --work DIR go before the subcommand)
  python3 -I scripts/reduction-diff.py gen SEED
  python3 -I scripts/reduction-diff.py compare BIN MODEL.rdra [--instance-count N]
  python3 -I scripts/reduction-diff.py minimize BIN MODEL.rdra OUT.rdra [--instance-count N]

`run` needs `tlc` on PATH (run it under `nix develop`). It writes each seed's model and
outputs under `--work` (default target/reduction-diff), copies a diverging model to
`<work>/diverged/seed<N>.rdra`, and ends with one JSON summary line. A seed whose run passes
`--timeout` is listed under `timed_out` with the side that timed out first (the unreduced
run is skipped after the reduced one times out), never dropped silently. The exit code is 1 when any seed diverges, or times out under `--fail-on-timeout`.

`minimize` shrinks a diverging model by deleting lines (ddmin) while it still diverges.
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


def gen(seed):
    """The model for a seed, and the instance count to verify it at."""
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
    for k in range(re_.choice([0, 1, 1, 2]) if n >= 2 else 0):
        j = re_.randrange(1, n)
        i = parent[j]
        src, dst = (j, i) if re_.random() < 0.6 else (i, j)
        raiser = re_.choice(arms[src])
        handler = re_.choice(arms[dst] + ([creates[dst]] if dst == j else []))
        out.append(f'event Ev{k} "ev{k}"\nraises({raiser}, Ev{k})\ntriggers(Ev{k}, {handler})')
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


def observe(rc, out):
    """What both runs must agree on."""
    names = sorted(set(re.findall(r"^(?:error|warning|info)\[([^\]]+)\]", out, re.M)))
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
    keep = ("state models", "not checked", "tlc_result", "OK:", "verification ", "Error: verification")
    lines = [l for l in out.splitlines() if l.startswith(keep)]
    return dict(rc=rc, names=names, checks=checks, lines=lines)


def undecided(o):
    return any(l.startswith("not checked") for l in o["lines"]) or "not decided" in o["checks"].values()


def agreement(red, plain):
    """"same"; "plain_undecided" when they agree on all the unreduced run decided, which a
    cone past the state cap or the budget can leave open where its reduced cone decides; or
    "diverged"."""
    if red == plain:
        return "same"
    if undecided(red) or not undecided(plain):
        return "diverged"
    decided = {c: v for c, v in plain["checks"].items() if v != "not decided"}
    if any(red["checks"].get(c) != v for c, v in decided.items()):
        return "diverged"
    skip = lambda l: l.startswith(("not checked", "state models", "Error: verification incomplete"))
    lines = lambda o: [l for l in o["lines"] if not skip(l)]
    if lines(red) != lines(plain) or set(plain["names"]) - set(red["names"]) - {"StateCapReached"}:
        return "diverged"
    # The state-model summary counts what was decided: the reduced run counts at least as
    # many witnessed states, deadlocks and use cases that never run, over the same cones.
    counts = lambda o: [
        [int(n) for n in re.findall(r"\d+", l)] for l in o["lines"] if l.startswith("state models")
    ]
    if len(counts(red)) != 1 or len(counts(plain)) != 1:
        return "diverged"
    r, p = counts(red)[0], counts(plain)[0]
    if len(r) != 5 or r[:2] != p[:2] or any(a < b for a, b in zip(r[2:], p[2:])):
        return "diverged"
    return "plain_undecided"


def has_reduced_cone(d):
    return d.is_dir() and any("S_class" in f.read_text() for f in d.glob("*.tla"))


def verify_both(bin_, model, instances, timeout, work):
    """Each side's observation, or "timeout"; and whether the users' run reduced a cone."""
    sides = {}
    for side in ("reduced", "plain"):
        shutil.rmtree(work / side, ignore_errors=True)
    for side, extra in (("reduced", {}), ("plain", {"RDRA_ISH_NO_REDUCE": "1"})):
        d = work / side
        env = {k: v for k, v in os.environ.items() if k != "RDRA_ISH_NO_REDUCE"}
        env.update(extra)
        cmd = [bin_, "verify", "--deny-undecided", "--instance-count", str(instances), "-o", str(d), str(model)]
        rc, out = sh(cmd, timeout, env=env, cwd=work)
        (work / f"{side}.out").write_text(out)
        sides[side] = "timeout" if rc == "timeout" else observe(rc, out)
        if rc == "timeout":
            # The unreduced run explores at least what the reduced one does.
            break
    if has_reduced_cone(work / "plain"):
        sys.exit("RDRA_ISH_NO_REDUCE did not keep the cones unreduced: is BIN this tree's build?")
    return sides, has_reduced_cone(work / "reduced")


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
    text, instances = gen(seed)
    model = work / "m.rdra"
    model.write_text(text)
    res = dict(seed=seed, instances=instances)
    ok, out = loads(args.bin, model, args.timeout, work)
    if not ok:
        return dict(res, status="load_error", detail=out[:300])
    sides, reduced = verify_both(args.bin, model, instances, args.timeout, work)
    res["reduced"] = reduced
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
    """`A:B` (B excluded), or seeds and ranges joined by commas."""
    seeds = []
    for part in spec.split(","):
        a, _, b = part.partition(":")
        seeds.extend(range(int(a), int(b)) if b else [int(a)])
    return seeds


def cmd_run(args):
    if shutil.which("tlc") is None:
        sys.exit("tlc is not on PATH; run under `nix develop`")
    args.bin = str(Path(args.bin).resolve())
    Path(args.work).mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(args.jobs) as ex:
        results = list(ex.map(lambda s: one(args, s), seed_list(args.seeds)))
    by = lambda st: [x for x in results if x["status"] == st]
    compared = by("same") + by("plain_undecided") + by("diverged")
    for x in by("diverged"):
        print(f"seed {x['seed']} diverges at {x['instances']} instances:")
        for side in ("reduced", "plain"):
            print(f"  {side}: {json.dumps(x[side], ensure_ascii=False)}")
    (Path(args.work) / "results.json").write_text(json.dumps(results, indent=1, ensure_ascii=False))
    summary = dict(
        generated=len(results),
        loaded=len(results) - len(by("load_error")),
        compared=len(compared),
        reduced=sum(1 for x in compared if x["reduced"]),
        three_instances=sum(1 for x in compared if x["instances"] == 3),
        plain_undecided=[x["seed"] for x in by("plain_undecided")],
        timed_out=[(x["seed"], x["side"]) for x in by("timeout")],
        load_errors=[x["seed"] for x in by("load_error")],
        diverged=[x["seed"] for x in by("diverged")],
    )
    print(json.dumps(summary))
    return 1 if summary["diverged"] or (args.fail_on_timeout and summary["timed_out"]) else 0


def cmd_gen(args):
    text, instances = gen(args.seed)
    sys.stdout.write(f"// instance count {instances}\n{text}")
    return 0


def diverges(args, lines, work):
    model = work / "m.rdra"
    model.write_text("\n".join(lines) + "\n")
    if not loads(args.bin, model, args.timeout, work)[0]:
        return False
    sides, _ = verify_both(args.bin, model, args.instance_count, args.timeout, work)
    return "timeout" not in sides.values() and agreement(sides["reduced"], sides["plain"]) == "diverged"


def cmd_compare(args):
    args.bin = str(Path(args.bin).resolve())
    work = Path(args.work) / "compare"
    work.mkdir(parents=True, exist_ok=True)
    model = Path(args.model).resolve()
    sides, reduced = verify_both(args.bin, model, args.instance_count, args.timeout, work)
    late = "timeout" in sides.values()
    status = "timeout" if late else agreement(sides["reduced"], sides["plain"])
    print(json.dumps(dict(status=status, reduces_a_cone=reduced, sides=sides), ensure_ascii=False, indent=1))
    return 1 if status in ("timeout", "diverged") else 0


def cmd_minimize(args):
    args.bin = str(Path(args.bin).resolve())
    work = Path(args.work) / "minimize"
    work.mkdir(parents=True, exist_ok=True)
    cur = Path(args.model).read_text().splitlines()
    if not diverges(args, cur, work):
        sys.exit("the model does not diverge")
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


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--work", default="target/reduction-diff")
    ap.add_argument("--timeout", type=int, default=120, help="seconds per verify run")
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("run")
    p.add_argument("bin")
    p.add_argument("--seeds", required=True, help="A:B (B excluded), or seeds and ranges joined by commas")
    p.add_argument("--fail-on-timeout", action="store_true", help="exit 1 when a seed times out")
    p.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) // 2))
    p = sub.add_parser("gen")
    p.add_argument("seed", type=int)
    for name in ("compare", "minimize"):
        p = sub.add_parser(name)
        p.add_argument("bin")
        p.add_argument("model")
        if name == "minimize":
            p.add_argument("out")
        p.add_argument("--instance-count", type=int, default=2)
    args = ap.parse_args()
    args.work = str(Path(args.work).resolve())
    return dict(run=cmd_run, gen=cmd_gen, compare=cmd_compare, minimize=cmd_minimize)[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
