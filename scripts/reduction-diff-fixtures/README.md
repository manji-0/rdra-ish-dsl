# reduction-diff fixtures

`python3 -I scripts/reduction-diff.py selfcheck` compares each directory's `reduced.out`
and `plain.out`, as `run` compares the two `verify` runs of a seed, and checks the status
against `expect`.

Every output is `verify --deny-undecided --instance-count 2` on the model of seed 44 or 470
(`reduction-diff.py gen <seed>`), with paths shortened. `plain.out` is the unreduced run
(`RDRA_ISH_NO_REDUCE=1`).

Seed 44 (#184):

- `reduced.out` runs at the default memory and finds `Deadlock_E2_status_s0`.
- `seed44-plain-memory-limit` runs at `--memory 256MiB`; its E2 and E3 cones stop at the
  memory limit.
- `seed44-plain-state-cap` runs at `--max-states 200000`; its E0, E2, and E3 cones stop at
  the cap.
- `seed44-both-decided` runs at `--memory 12GiB`, finishes, and finds the same deadlock.
- `seed44-reduced-memory-limit` swaps the two sides of `seed44-plain-memory-limit`.

Seed 470:

- `seed470-plain-state-cap` runs at `--max-states 500000`; its E2, E3, and E4 cones and the
  `MultiForbidden_E1_0` and `P0` cones stop at the cap, and the vacuity of `Forbidden_E1_0`
  is not printed.
- `seed470-plain-memory-limit-with-tlc` runs both sides under `--with-tlc` with `memory =
  "12GiB"` in `rdra.toml`; the unreduced E4 cone stops at the memory limit.

Four cases edit an output by hand, to check what must still diverge and what must not:

- `seed44-plain-decides-no-deadlock` drops the open E2 deadlock line from the 256MiB run.
- `seed44-plain-only-finding` adds a `Witness_E1_status_s1` error to the 256MiB run.
- `seed470-reduced-violates-open-check` makes the reduced run violate `MultiForbidden_E1_0`,
  which the capped unreduced run leaves open.
- `seed470-reduced-violates-decided-check` makes the reduced `--with-tlc` run violate
  `MultiForbidden_E1_0`, which the unreduced run decides holds.
