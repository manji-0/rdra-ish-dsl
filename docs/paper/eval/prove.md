# `--prove` as an evaluated heuristic (roadmap T16)

**Decision.** The prover's claim "for any number of rows" rests on Houdini over mined lemma
candidates and counterexample-to-induction clauses ([z3.md](../../theory/z3.md), B1–B18). It has no
decidable fragment and no cutoff theorem. Roadmap T16 offered two options: identify such a
fragment, or present the prover as an evaluated heuristic. We take the second. The papers make no
completeness claim. Soundness is the claim B2 of the inventory: a `proved` rule holds at every bound
in the printed range, relative to z3 (H1, H2, H4). Paper 1 does not use `--prove`. The technical
report gives this evaluation.

**Method.** `scripts/paper/prove-eval.py` runs `verify ROOT --prove` at `RDRA_JOBS=1` on every
root of `samples.txt` (oidc-auth at 1 instance). It records each state rule's verdict:
`proved`; `unproved` with its reason; `refuted` (BFS found a counterexample, so the rule is not
tried); `escalated` (unproved, then BFS at one and two more instances). The same run on the 91 cases of
the engine corpus, at their header flags, gives [data/prove-corpus.tsv](./data/prove-corpus.tsv). z3
5.4.1 from nixpkgs; the flake pins 5.1.0, and the determinism claim B13 is stated for that version.
Main at `9e4a123`.

## Samples

Every state rule that BFS does not refute is proved, on every sample, in at most 73 s per root:

| Root | proved | unproved | refuted | escalated | out of scope | Wall (s) |
|---|--:|--:|--:|--:|--:|--:|
| samples/ec-site | 1 | 0 | 0 | 0 | 0 | 0.16 |
| samples/clinic-ops | 13 | 0 | 0 | 0 | 0 | 40.02 |
| samples/patent-ops | 13 | 0 | 0 | 0 | 0 | 6.8 |
| samples/food-permit-ops | 14 | 0 | 0 | 0 | 0 | 41.1 |
| samples/insurance-claim-ops | 23 | 0 | 0 | 0 | 0 | 73.19 |
| samples/oidc-auth --instance-count 1 | 16 | 0 | 0 | 0 | 0 | 22.44 |
| samples/personal-info | 2 | 0 | 0 | 0 | 0 | 0.08 |
| samples/incremental-order/step-7-business-rules/src | 2 | 0 | 0 | 0 | 0 | 0.05 |
| samples/formal-verification/check_met.rdra | 2 | 0 | 0 | 0 | 0 | 0.03 |
| samples/formal-verification/check_unmet.rdra | 0 | 0 | 1 | 0 | 0 | 0.08 |
| samples/formal-verification/cross_order_payment.rdra | 1 | 0 | 0 | 0 | 0 | 0.05 |
| samples/formal-verification/cross_order_payment_split.rdra | 0 | 0 | 1 | 0 | 0 | 0.1 |
| samples/formal-verification/int_stock.rdra | 1 | 0 | 0 | 0 | 0 | 0.04 |
| samples/formal-verification/min_card_met.rdra | 1 | 0 | 0 | 0 | 0 | 0.04 |
| samples/formal-verification/min_card_unmet.rdra | 0 | 0 | 1 | 0 | 0 | 0.1 |
| samples/formal-verification/none_any_child.rdra | 1 | 0 | 0 | 0 | 0 | 0.04 |
| samples/formal-verification/none_any_child_bypassed.rdra | 0 | 0 | 1 | 0 | 0 | 0.12 |
| samples/formal-verification/now_coupon.rdra | 0 | 0 | 1 | 0 | 0 | 0.09 |
| samples/formal-verification/one_to_one_shared.rdra | 0 | 0 | 1 | 0 | 0 | 0.1 |
| samples/formal-verification/order.rdra | 2 | 0 | 0 | 0 | 0 | 0.04 |
| samples/formal-verification/quantifier_none.rdra | 0 | 0 | 1 | 0 | 0 | 0.1 |
| samples/formal-verification/quantifier_guard.rdra | 1 | 0 | 0 | 0 | 0 | 0.06 |
| samples/formal-verification/split_run_order.rdra | 2 | 0 | 0 | 0 | 0 | 0.06 |
| samples/formal-verification/untriggered_sets.rdra | 0 | 0 | 1 | 0 | 0 | 0.09 |
| samples/formal-verification/unwitnessed_state.rdra | 1 | 0 | 0 | 0 | 0 | 0.09 |
| samples/formal-verification/unique_double_booking.rdra | 0 | 0 | 1 | 0 | 0 | 0.08 |
| samples/formal-verification/unique_booking_guarded.rdra | 1 | 0 | 0 | 0 | 0 | 0.04 |
| samples/formal-verification/counting_rules.rdra | 2 | 0 | 0 | 0 | 0 | 0.1 |
| samples/formal-verification-fail/order.rdra | 0 | 0 | 1 | 0 | 0 | 0.09 |
| **Total** | **99** | **0** | **10** | **0** | **0** | |

| Unproved: reason | Rules |
|---|--:|

The 79 rules of the five domain models are all proved for any number of rows. On these models the
heuristic is complete: no rule that holds at the bound is left unproved. The models were written
alongside the prover, so this is not evidence about models in general.

## Corpus

The corpus includes cases written to defeat the prover (`z3_*`) and cases whose rule fails only
with more rows. Over its 91 cases, 118 rule verdicts:

| Verdict | Rules |
|---|--:|
| proved | 53 |
| refuted by BFS at the header's bounds | 39 |
| unproved: not inductive | 12 |
| unproved: an initial state of the wider encoding violates it | 1 |

Each unproved rule is escalated: BFS runs its cone at one and two more instances.

| Case | Rule | Escalation |
|---|---|---|
| presence_per_entity_slots.rdra | Unique_Booking_tag | counterexample at Booking=2 Room=4 instances (BFS on its cone; ok at Booking=1 Room=3) |
| presence_per_entity_slots.rdra | Unique_Member_tag | counterexample at 2 instances (BFS on its cone; ok at 1) |
| safety_at_most_bound.rdra | Quantifier_Member_0 | ok at 3 instances, counterexample at 4 instances (BFS on its cone; ok at 2) |
| safety_exactly_along.rdra | Quantifier_Member_1 | counterexample at 3 instances (BFS on its cone; ok at 2) |
| safety_exactly_along.rdra | Quantifier_Member_2 | counterexample at 3 instances (BFS on its cone; ok at 2) |
| safety_max_card_range.rdra | MaxCard_Copy_Shelf | counterexample at 3 instances (BFS on its cone; ok at 2) |
| z3_guarded_null_beside_present.rdra | Forbidden_Item_0 | ok at 2 instances, not decided at 3 instances (BFS on its cone; ok at 1) |
| z3_guarded_present_past_the_top.rdra | Forbidden_Item_0 | ok at 2 instances, not decided at 3 instances (BFS on its cone; ok at 1) |
| z3_int_and_now_boundary.rdra | Forbidden_Coupon_2 | ok at 3 instances, ok at 4 instances (BFS on its cone; ok at 2) |
| z3_int_listing_guard.rdra | Forbidden_Item_0 | ok at 3 instances, ok at 4 instances (BFS on its cone; ok at 2) |
| z3_int_listing_whole_range.rdra | Forbidden_Item_0 | ok at 3 instances, ok at 4 instances (BFS on its cone; ok at 2) |
| z3_needs_run_mark.rdra | MultiForbidden_Light_0 | ok at 3 instances, not decided at 4 instances (BFS on its cone; ok at 2) |
| z3_two_ok_three_fails.rdra | Forbidden_Order_0 | counterexample at 3 instances (BFS on its cone; ok at 2) |

- **Seven rules fail at a larger bound** than the header's. Five of them are `ok` at the `verify`
  default of 2 instances and fail at 3 (four rules) or 4 (one rule): `Quantifier_Member_0`,
  `Quantifier_Member_1`, `Quantifier_Member_2`, `MaxCard_Copy_Shelf` and `Forbidden_Order_0` of
  `z3_two_ok_three_fails`. For these the prover's `unproved` is the right answer, and escalation
  turns it into a counterexample. These are cardinality and counting rules (`at_most`,
  `exactly`, maximum cardinality), which need more rows than the bound they count to.
- **Six rules hold at every bound tried and stay unproved.** The encoding widens the system (any
  absent row may be created, and Int terms are unbounded), and Houdini's candidate shapes do not hold
  an inductive strengthening. Three of the six are `not decided` at the larger bound.

## What this says about "any number of rows"

- On the domain samples, `proved` covers every rule that BFS passes, so the bounded verdict extends
  to any number of rows there.
- On adversarial inputs, 13 of 66 rules that BFS passes at the header's bounds (20%) are not proved.
  For 7 of them the prover is right and the bounded verdict was wrong (more rows break the rule). For
  6 the prover is incomplete.
- The bounded checker's default of 2 instances is not enough for counting rules: the seven above fail
  first at 3 or 4 instances. [minimal.md](./minimal.md) measured the corpus's *expected*
  counterexamples at their header bounds. This table adds counterexamples that no header expects,
  found by escalation. Together they say that 2 instances find the known counterexamples of most
  rule kinds, while counting rules need the prover or escalation.

**Baselines not run.** Ivy, mypyvy and DistAI take first-order specifications, and the models would
have to be encoded by hand. A comparison with them is left to the technical report's future work.
