# Wave 3 timings

`bench.sh <binary> <label>` prints the median wall-clock time of 5 runs per command on the
largest samples. `all.tsv` holds one run on an idle 4-core machine, release builds:

- `base`: before wave 3 (5f8a005)
- `c1`: import scopes by reference (C1)
- `par`: plus parallel parse (F14)
- `f1`: plus one lifecycle search per command (F1)

Profiles came from `valgrind --tool=callgrind` on a release build with
`CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`.
