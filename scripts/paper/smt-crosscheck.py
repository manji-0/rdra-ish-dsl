#!/usr/bin/env python3
"""T15 of #594: re-check every z3 query behind `verify --prove` with a second solver.

Usage:
  cargo build --release -p rdra-ish-core --example prove_transcript
  target/release/examples/prove_transcript OUTDIR ROOT...      # z3 on PATH
  scripts/paper/smt-crosscheck.py OUTDIR/<root>.smt2 [Z3] [CVC5]

The transcript holds every SMT-LIB command z3 received during one prover run: the encoding, every
initiation and consecution query of Houdini, the lemma mining and the counterexample-to-induction
clauses. The script replays it twice, through z3 with the transcript as written and through cvc5
with z3's own options removed (`:rlimit`, `:smt.mbqi`), and compares the answers of the `check-sat`
queries in order. A `proved` rests on the `unsat` answers. So the claim to check is that every
query z3 answered `unsat`, cvc5 does not answer `sat`. Prints the counts and every disagreement.
Standard library only.
"""

import re
import subprocess
import sys

STRIP = re.compile(r"^\(set-option :(rlimit|smt\.[a-z_.]+) [^)]*\)\s*$")


def answers(solver, text, extra):
    p = subprocess.run([solver, *extra], input=text, capture_output=True, text=True, timeout=7200)
    out = [l.strip() for l in p.stdout.splitlines() if l.strip() in ("sat", "unsat", "unknown")]
    errors = [l for l in (p.stdout + p.stderr).splitlines() if "error" in l.lower()]
    return out, errors


def main(argv):
    path = argv[0]
    z3 = argv[1] if len(argv) > 1 else "z3"
    cvc5 = argv[2] if len(argv) > 2 else "cvc5"
    text = open(path).read().replace(";;--", "")
    # cvc5 forgets the logic at each (reset), so it is set again after each one.
    portable = "(set-logic ALL)\n" + "\n".join(
        "(reset)\n(set-logic ALL)" if l.strip() == "(reset)" else l
        for l in text.splitlines() if not STRIP.match(l))
    a, ea = answers(z3, text, ["-in"])
    b, eb = answers(cvc5, portable, ["--lang", "smt2", "--incremental", "--tlimit-per=60000"])
    print(f"{path}: z3 {len(a)} answers, cvc5 {len(b)} answers")
    if ea or eb:
        print(f"  solver errors: z3 {ea[:3]} cvc5 {eb[:3]}")
    pairs = list(zip(a, b))
    count = {}
    for x, y in pairs:
        count[(x, y)] = count.get((x, y), 0) + 1
    for (x, y), n in sorted(count.items()):
        print(f"  z3 {x:7} cvc5 {y:7} {n}")
    bad = [i for i, (x, y) in enumerate(pairs) if x == "unsat" and y == "sat"]
    print(f"  queries z3 answers unsat and cvc5 sat: {len(bad)}" + (f" (first at {bad[:5]})" if bad else ""))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
