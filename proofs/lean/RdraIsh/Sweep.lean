import RdraIsh.Reduction

/-!
# The stability certificate of the partition (cones.md, "On (B1)–(B3)")

`Classes::of_graph` certifies the partition of a run of `X` before the reduction uses it,
with `partition/reference.rs::stable`. This file models that check as the code computes it
and proves that a partition that passes it is `Stable` (Reduction.lean), with the
representative `of_graph` takes: the first state of each block. The converse holds too: a
partition `Stable` on a complete run passes the check.

**The run** (`ExplRun`). `of_graph` numbers the run's states `0, …, n - 1`. `partition`
gives each state a label, its projection onto `obs` and the inputs (`initial`), and
`split_edges` splits the run's edges into silent ones and rises, by state number. `Complete`
says the run is the reachable part of `X`: every reachable state has a number, every number
is a reachable state, the silent edges and rises are exactly `X`'s between reachable states,
and two states with the same label agree on `obs` and the input bytes. That is the flat
case: a run of `X` with no inner reduction and not up to `H`.

**The check** (`Cert`), the three parts of `stable`:

- (B1) every block lies inside one label;
- (B2) one sweep of `partition/reference.rs::stuttering_blocks` with every block as a
  splitter splits nothing. For a splitter `sp`, the sweep marks the states outside `sp` that
  have a silent edge into `sp` (two different states: `stuttering_blocks` drops self-loops),
  then, from each marked state, its silent predecessors in its own block (`Marked`). A block
  splits when some of its states are marked and some are not. A sweep that splits nothing
  never changes the blocks, so every splitter sees the partition it started from, and
  `same_partition` holds of the result exactly then, since a split adds a block;
- (B3) every state enters by a rise the same set of blocks as its block's first state
  (`first`).

`stable` renumbers the blocks in order of first appearance before the sweep. The renumbering
is injective, so the checks read the same on the blocks as given.

**What is read from the code, not derived.** That the depth-first marking of
`stuttering_blocks` computes `Marked` (the least set closed under its two rules), that
`same_partition` of the sweep's result holds iff the sweep split nothing, and that the run
is complete. The fallback, `partition/reference.rs::quotient_blocks`, passes the same
check, and `partition/reference.rs::marking_splits_nothing` besides: a marking by the two rules
of `Marked` alone, closed under them and all or none on each block, the hypotheses of
`b2_of_marking`. A fallback that fails either leaves the cone unreduced.
-/

namespace RdraIsh.Sweep

open LTS

variable {S I O L XSa Ra : Type}

/-- A run of `X` as `Classes::of_graph` reads it: `n` states numbered from `0`, `st i` the
state numbered `i`, `idx x` the number of `x`, the silent edges and the rises by number
(`split_edges`), and each state's label (`partition`'s `initial`). -/
structure ExplRun (S I L : Type) where
  n : Nat
  st : Nat → S × I
  idx : S × I → Nat
  silent : List (Nat × Nat)
  rises : List (Nat × Nat)
  initial : Nat → L

/-- The run is the reachable part of `X`, and a label fixes `obs` and the input bytes. -/
structure Complete (X : LTS (S × I) (XSa ⊕ Ra)) (obs : S → O) (R : ExplRun S I L) : Prop where
  idx_lt : ∀ {x}, X.Reach x → R.idx x < R.n
  st_idx : ∀ {x}, X.Reach x → R.st (R.idx x) = x
  st_reach : ∀ {i}, i < R.n → X.Reach (R.st i)
  idx_st : ∀ {i}, i < R.n → R.idx (R.st i) = i
  silent_sound : ∀ {i j}, (i, j) ∈ R.silent → i < R.n ∧ j < R.n ∧
    (silent X).Next (R.st i) (R.st j)
  silent_complete : ∀ {x y}, X.Reach x → (silent X).Next x y → (R.idx x, R.idx y) ∈ R.silent
  rise_sound : ∀ {i j}, (i, j) ∈ R.rises → i < R.n ∧ j < R.n ∧
    ∃ r, X.step (R.st i) (.inr r) (R.st j)
  rise_complete : ∀ {x y r}, X.Reach x → X.step x (.inr r) y → (R.idx x, R.idx y) ∈ R.rises
  label : ∀ {x y}, X.Reach x → X.Reach y → R.initial (R.idx x) = R.initial (R.idx y) →
    obs x.1 = obs y.1 ∧ x.2 = y.2

/-- The states a sweep of `stuttering_blocks` marks for the splitter `sp`: a state outside
`sp` with a silent edge into `sp`, and a silent predecessor in its own block of a marked
state. Self-loops are dropped (`inter`). -/
inductive Marked (E : List (Nat × Nat)) (blk : Nat → Nat) (sp : Nat) : Nat → Prop
  | front {p t} : (p, t) ∈ E → p ≠ t → blk t = sp → blk p ≠ sp → Marked E blk sp p
  | back {p x} : (p, x) ∈ E → p ≠ x → blk p = blk x → Marked E blk sp x → Marked E blk sp p

/-- The first state of block `b` among `0, …, n - 1` (`0` when there is none): the state
`stable` compares each state's rises with, and the representative `of_graph` takes. -/
def first (blk : Nat → Nat) (n b : Nat) : Nat :=
  ((List.range n).find? (fun i => blk i == b)).getD 0

/-- `partition/reference.rs::stable`, the certificate of the partition `blk`. -/
structure Cert (R : ExplRun S I L) (blk : Nat → Nat) : Prop where
  /-- (B1): a block lies inside one label. -/
  b1 : ∀ {i j}, i < R.n → j < R.n → blk i = blk j → R.initial i = R.initial j
  /-- (B2): the sweep with every block as a splitter splits no block. -/
  b2 : ∀ {sp i j}, i < R.n → j < R.n → blk i = blk j → Marked R.silent blk sp i →
    Marked R.silent blk sp j
  /-- (B3): a state enters by a rise the blocks its block's first state enters. -/
  b3 : ∀ {i c}, i < R.n → ((∃ j, (i, j) ∈ R.rises ∧ blk j = c) ↔
    ∃ j, (first blk R.n (blk i), j) ∈ R.rises ∧ blk j = c)

theorem first_spec {blk : Nat → Nat} {n i : Nat} (hi : i < n) :
    first blk n (blk i) < n ∧ blk (first blk n (blk i)) = blk i := by
  unfold first
  have hsome : ((List.range n).find? (fun k => blk k == blk i)).isSome := by
    rw [List.find?_isSome]
    exact ⟨i, List.mem_range.mpr hi, by simp⟩
  obtain ⟨f, hf⟩ := Option.isSome_iff_exists.mp hsome
  rw [hf]
  simp only [Option.getD_some]
  have hm := List.mem_of_find?_eq_some hf
  have hp := List.find?_some hf
  exact ⟨List.mem_range.mp hm, by simpa using hp⟩

section Proofs

variable {X : LTS (S × I) (XSa ⊕ Ra)} {obs : S → O} {R : ExplRun S I L} {blk : Nat → Nat}

/-- The blocks of `X`'s states, read through their numbers. -/
def blockOf (R : ExplRun S I L) (blk : Nat → Nat) (x : S × I) : Nat := blk (R.idx x)

/-- `of_graph`'s representative: the first state of the block. -/
def repOf (R : ExplRun S I L) (blk : Nat → Nat) (c : Nat) : S × I := R.st (first blk R.n c)

theorem blockOf_st (hC : Complete X obs R) {i : Nat} (hi : i < R.n) :
    blockOf R blk (R.st i) = blk i := by
  unfold blockOf; rw [hC.idx_st hi]

/-- A marked state is a state of the run that leaves its block into `sp` by a silent path
through its own block and one more silent edge. -/
theorem exit_of_marked (hC : Complete X obs R) {sp i : Nat} (h : Marked R.silent blk sp i) :
    i < R.n ∧ SilentExit X (blockOf R blk) (R.st i) sp := by
  induction h with
  | @front p t he _ ht hp =>
    obtain ⟨hpn, htn, hn⟩ := hC.silent_sound he
    refine ⟨hpn, ?_, R.st p, R.st t, .refl rfl, hn, ?_⟩
    · rw [blockOf_st hC hpn]; exact fun h => hp h.symm
    · rw [blockOf_st hC htn, ht]
  | @back p x he _ hb _ ih =>
    obtain ⟨hpn, hxn, hn⟩ := hC.silent_sound he
    obtain ⟨_, hne, z, z', hpath, hzn, hz⟩ := ih
    have hpx : blockOf R blk (R.st p) = blockOf R blk (R.st x) := by
      rw [blockOf_st hC hpn, blockOf_st hC hxn, hb]
    refine ⟨hpn, by rw [hpx]; exact hne, z, z', .cons rfl hn ?_, hzn, hz⟩
    rw [hpx]; exact hpath

/-- A reachable state that leaves its block `b` into `sp` by a silent path through `b` is
marked for the splitter `sp`. -/
theorem marked_of_path (hC : Complete X obs R) {b sp : Nat} {z z' : S × I}
    (hsp : sp ≠ b) (hzn : (silent X).Next z z') (hz : blockOf R blk z' = sp) :
    ∀ {x}, (silent X).PathWithin (fun w => blockOf R blk w = b) x z → X.Reach x →
      Marked R.silent blk sp (R.idx x) := by
  intro x hp hx
  induction hp with
  | @refl x hin =>
    refine .front (hC.silent_complete hx hzn) ?_ hz ?_
    · intro h
      have : blockOf R blk x = blockOf R blk z' := by unfold blockOf; rw [h]
      rw [hin, hz] at this; exact hsp this.symm
    · intro h; exact hsp (by rw [← h]; exact hin)
  | @cons x u z hin hn hrest ih =>
    obtain ⟨a, ha⟩ := hn
    have hu : X.Reach u := Reach.step hx ha
    have hmu := ih hzn hu
    by_cases hxu : R.idx x = R.idx u
    · rw [hxu]; exact hmu
    · refine .back (hC.silent_complete hx ⟨a, ha⟩) hxu ?_ hmu
      have := hrest.start
      exact hin.trans this.symm

/-- A reachable state is marked for `sp` iff it leaves its block into `sp` in the sense of
(B2) (`SilentExit`). -/
theorem marked_iff (hC : Complete X obs R) {x : S × I} (hx : X.Reach x) (sp : Nat) :
    Marked R.silent blk sp (R.idx x) ↔ SilentExit X (blockOf R blk) x sp := by
  constructor
  · intro h
    have := (exit_of_marked hC h).2
    rwa [hC.st_idx hx] at this
  · rintro ⟨hne, z, z', hp, hzn, hz⟩
    exact marked_of_path hC hne hzn hz hp hx

/-- **The certificate gives (B1)–(B3).** On a complete run of `X`, a partition that passes
`partition/reference.rs::stable` is `Stable`, with the first state of each block as its
representative. -/
theorem stable_of_cert (hC : Complete X obs R) (hc : Cert R blk) :
    Stable X obs (blockOf R blk) (repOf R blk) where
  b1 := by
    intro x y hx hy hb
    exact hC.label hx hy (hc.b1 (hC.idx_lt hx) (hC.idx_lt hy) hb)
  b2 := by
    intro x y c' hx hy hb hexit
    have hm := (marked_iff hC hx c').mpr hexit
    exact (marked_iff hC hy c').mp (hc.b2 (hC.idx_lt hx) (hC.idx_lt hy) hb hm)
  b3 := by
    intro x y r x' hx hy hb hs
    have hx'i := hC.rise_complete hx hs
    have hfx := (hc.b3 (c := blockOf R blk x') (hC.idx_lt hx)).mp ⟨_, hx'i, rfl⟩
    have hb' : blk (R.idx x) = blk (R.idx y) := hb
    rw [hb'] at hfx
    obtain ⟨j, hj, hjb⟩ := (hc.b3 (c := blockOf R blk x') (hC.idx_lt hy)).mpr hfx
    obtain ⟨_, hjn, r', hr'⟩ := hC.rise_sound hj
    rw [hC.st_idx hy] at hr'
    exact ⟨r', R.st j, hr', by rw [blockOf_st hC hjn]; exact hjb⟩
  rep_block := by
    intro x hx
    obtain ⟨hf, hfb⟩ := first_spec (blk := blk) (hC.idx_lt hx)
    show blockOf R blk (R.st (first blk R.n (blk (R.idx x)))) = blk (R.idx x)
    rw [blockOf_st hC hf, hfb]
  rep_reach := by
    intro x hx
    exact hC.st_reach (first_spec (blk := blk) (hC.idx_lt hx)).1

/-- **The converse.** On a complete run whose labels are exactly the values of `obs` and the
input bytes, a partition that is `Stable` (with any representative) passes the
certificate. So the check rejects only partitions that are not stable. -/
theorem cert_of_stable (hC : Complete X obs R)
    (hlabel : ∀ {x y}, X.Reach x → X.Reach y → obs x.1 = obs y.1 → x.2 = y.2 →
      R.initial (R.idx x) = R.initial (R.idx y))
    {rep : Nat → S × I} (hX : Stable X obs (blockOf R blk) rep) : Cert R blk where
  b1 := by
    intro i j hi hj hb
    have hri := hC.st_reach hi
    have hrj := hC.st_reach hj
    have hbb : blockOf R blk (R.st i) = blockOf R blk (R.st j) := by
      rw [blockOf_st hC hi, blockOf_st hC hj, hb]
    obtain ⟨ho, hin⟩ := hX.b1 hri hrj hbb
    have := hlabel hri hrj ho hin
    rwa [hC.idx_st hi, hC.idx_st hj] at this
  b2 := by
    intro sp i j hi hj hb hm
    have hri := hC.st_reach hi
    have hrj := hC.st_reach hj
    have hbb : blockOf R blk (R.st i) = blockOf R blk (R.st j) := by
      rw [blockOf_st hC hi, blockOf_st hC hj, hb]
    rw [← hC.idx_st hi] at hm
    have := (marked_iff hC hrj sp).mpr (hX.b2 hri hrj hbb ((marked_iff hC hri sp).mp hm))
    rwa [hC.idx_st hj] at this
  b3 := by
    intro i c hi
    obtain ⟨hf, hfb⟩ := first_spec (blk := blk) hi
    have hri := hC.st_reach hi
    have hrf := hC.st_reach hf
    have hbb : blockOf R blk (R.st i) = blockOf R blk (R.st (first blk R.n (blk i))) := by
      rw [blockOf_st hC hi, blockOf_st hC hf, hfb]
    -- A rise of one of two states of a block is matched by a rise of the other into the
    -- same block, read back as an edge of the run.
    have transfer : ∀ {u v : Nat}, u < R.n → v < R.n →
        blockOf R blk (R.st u) = blockOf R blk (R.st v) →
        (∃ j, (u, j) ∈ R.rises ∧ blk j = c) → ∃ j, (v, j) ∈ R.rises ∧ blk j = c := by
      intro u v hu hv huv ⟨j, hj, hjc⟩
      obtain ⟨_, hjn, r, hr⟩ := hC.rise_sound hj
      obtain ⟨r', y', hy', hyb⟩ := hX.b3 (hC.st_reach hu) (hC.st_reach hv) huv hr
      refine ⟨R.idx y', ?_, ?_⟩
      · have := hC.rise_complete (hC.st_reach hv) hy'
        rwa [hC.idx_st hv] at this
      · have : blockOf R blk y' = blk j := by rw [hyb, blockOf_st hC hjn]
        exact this.trans hjc
    exact ⟨transfer hi hf hbb, transfer hf hi hbb.symm⟩

end Proofs

/-! ## A Boolean checker for `Cert`

`check` decides a sufficient condition for `Cert` over the run's lists, for a checker that
reads a dumped partition. (B2) is reduced to one fact about any marking a checker computes
(`b2_of_marking`): a set of states for each splitter that lies inside `Marked` and is closed
under its two rules is all of `Marked`, so it is enough that the set is all-or-none on each
block. `check` computes such a set by rounds (`marks`), tests that it is closed (`closedB`),
and compares each state with its block's first state. -/

section Check

variable {S I L : Type}

/-- Some state has a silent edge into `sp`, for a marked state. -/
theorem Marked.splitter {E : List (Nat × Nat)} {blk : Nat → Nat} {sp p : Nat}
    (h : Marked E blk sp p) : ∃ q t, (q, t) ∈ E ∧ blk t = sp := by
  induction h with
  | front he _ ht _ => exact ⟨_, _, he, ht⟩
  | back _ _ _ _ ih => exact ih

/-- A set of states that is closed under the two rules of `Marked` holds every marked
state. -/
theorem marked_sub {E : List (Nat × Nat)} {blk : Nat → Nat} {sp : Nat} {M : Nat → Prop}
    (hfront : ∀ {p t}, (p, t) ∈ E → p ≠ t → blk t = sp → blk p ≠ sp → M p)
    (hback : ∀ {p x}, (p, x) ∈ E → p ≠ x → blk p = blk x → M x → M p) :
    ∀ {p}, Marked E blk sp p → M p := by
  intro p h
  induction h with
  | front he hne ht hp => exact hfront he hne ht hp
  | back he hne hb _ ih => exact hback he hne hb ih

/-- **(B2) from any marking.** If, for every splitter that some silent edge enters, a set
`M sp` lies inside `Marked`, is closed under its rules, and holds either every state of a
block or none, then the sweep splits nothing (`Cert.b2`). -/
theorem b2_of_marking {R : ExplRun S I L} {blk : Nat → Nat} (M : Nat → Nat → Prop)
    (hsound : ∀ {sp p}, M sp p → Marked R.silent blk sp p)
    (hfront : ∀ {sp p t}, (p, t) ∈ R.silent → p ≠ t → blk t = sp → blk p ≠ sp → M sp p)
    (hback : ∀ {sp p x}, (p, x) ∈ R.silent → p ≠ x → blk p = blk x → M sp x → M sp p)
    (hall : ∀ {sp i j}, i < R.n → j < R.n → blk i = blk j → M sp i → M sp j) :
    ∀ {sp i j}, i < R.n → j < R.n → blk i = blk j → Marked R.silent blk sp i →
      Marked R.silent blk sp j := fun hi hj hb hm =>
  hsound (hall hi hj hb (marked_sub hfront hback hm))

/-- One round of marking for the splitter `sp`: a state is marked when it was, or when one
of its silent edges meets a rule of `Marked`. -/
def markRound (E : List (Nat × Nat)) (blk : Nat → Nat) (sp : Nat) (m : Nat → Bool)
    (p : Nat) : Bool :=
  m p || E.any fun e => e.1 == p && e.1 != e.2 &&
    ((blk e.2 == sp && blk p != sp) || (blk p == blk e.2 && m e.2))

/-- `k` rounds of marking from nothing. -/
def marks (E : List (Nat × Nat)) (blk : Nat → Nat) (sp : Nat) : Nat → Nat → Bool
  | 0 => fun _ => false
  | k + 1 => markRound E blk sp (marks E blk sp k)

theorem marked_of_marks {E : List (Nat × Nat)} {blk : Nat → Nat} {sp : Nat} :
    ∀ {k p}, marks E blk sp k p = true → Marked E blk sp p := by
  intro k
  induction k with
  | zero => intro p h; exact absurd h (by simp [marks])
  | succ k ih =>
    intro p h
    simp only [marks, markRound, Bool.or_eq_true, List.any_eq_true, Bool.and_eq_true,
      beq_iff_eq, bne_iff_ne, ne_eq] at h
    rcases h with h | ⟨⟨q, t⟩, he, ⟨hq, hne⟩, hr⟩
    · exact ih h
    · simp only at hq hne hr
      subst hq
      rcases hr with ⟨ht, hp⟩ | ⟨hb, hm⟩
      · exact .front he hne ht hp
      · exact .back he hne hb (ih hm)

/-- The marking `m` is closed under the rules of `Marked`. -/
def closedB (E : List (Nat × Nat)) (blk : Nat → Nat) (sp : Nat) (m : Nat → Bool) : Bool :=
  E.all fun e => e.1 == e.2 ||
    ((!(blk e.2 == sp && blk e.1 != sp) || m e.1) && (!(blk e.1 == blk e.2 && m e.2) || m e.1))

/-- The rises of `u` enter only blocks that a rise of `v` enters. -/
def entersB (rises : List (Nat × Nat)) (blk : Nat → Nat) (u v : Nat) : Bool :=
  rises.all fun e => e.1 != u || rises.any fun e' => e'.1 == v && blk e'.2 == blk e.2

variable [DecidableEq L]

/-- The checker: every silent edge joins two states of the run; and for every state `i`, with
`f` its block's first state, `i` has `f`'s label (B1), `i` and `f` enter the same blocks by
a rise (B3), and for the splitter `blk i` the marking after `n + 1` rounds is closed and
holds every state of a block or none (B2). -/
def check (R : ExplRun S I L) (blk : Nat → Nat) : Bool :=
  R.silent.all (fun e => decide (e.1 < R.n) && decide (e.2 < R.n)) &&
  (List.range R.n).all fun i =>
    decide (R.initial i = R.initial (first blk R.n (blk i))) &&
    entersB R.rises blk i (first blk R.n (blk i)) &&
    entersB R.rises blk (first blk R.n (blk i)) i &&
    closedB R.silent blk (blk i) (marks R.silent blk (blk i) (R.n + 1)) &&
    (List.range R.n).all fun j =>
      marks R.silent blk (blk i) (R.n + 1) j ==
        marks R.silent blk (blk i) (R.n + 1) (first blk R.n (blk j))

theorem entersB_sound {rises : List (Nat × Nat)} {blk : Nat → Nat} {u v : Nat}
    (h : entersB rises blk u v = true) {c : Nat} :
    (∃ j, (u, j) ∈ rises ∧ blk j = c) → ∃ j, (v, j) ∈ rises ∧ blk j = c := by
  rintro ⟨j, hj, rfl⟩
  have := List.all_eq_true.mp h _ hj
  simp only [Bool.or_eq_true, bne_iff_ne, ne_eq, not_true_eq_false, false_or,
    List.any_eq_true, Bool.and_eq_true, beq_iff_eq] at this
  obtain ⟨⟨v', j'⟩, hm, hv, hb⟩ := this
  simp only at hv hb
  subst hv
  exact ⟨j', hm, hb⟩

/-- **The checker is sound**: a partition that `check` accepts passes the certificate, so on
a complete run it is `Stable` (`stable_of_cert`). -/
theorem check_sound {R : ExplRun S I L} {blk : Nat → Nat} (h : check R blk = true) :
    Cert R blk := by
  simp only [check, Bool.and_eq_true, List.all_eq_true, List.mem_range, decide_eq_true_eq]
    at h
  obtain ⟨hE, hi⟩ := h
  have first_eq : ∀ {i j}, blk i = blk j → first blk R.n (blk i) = first blk R.n (blk j) :=
    fun hb => by rw [hb]
  refine ⟨fun hi' hj' hb => ?_, ?_, fun {i c} hi' => ?_⟩
  · rw [(hi _ hi').1.1.1.1, (hi _ hj').1.1.1.1, first_eq hb]
  · intro sp i j hi' hj' hb hm
    obtain ⟨q, t, hqt, ht⟩ := hm.splitter
    have htn : t < R.n := (hE _ hqt).2
    obtain ⟨⟨⟨_, _⟩, hcl⟩, hall⟩ := hi t htn
    rw [ht] at hcl hall
    have hcl' := List.all_eq_true.mp hcl
    have hmi : marks R.silent blk sp (R.n + 1) i = true := by
      refine marked_sub (M := fun p => marks R.silent blk sp (R.n + 1) p = true) ?_ ?_ hm
      · intro p t he hne ht' hp
        have := hcl' _ he
        simp only [Bool.or_eq_true, beq_iff_eq, hne, false_or, Bool.and_eq_true,
          Bool.not_eq_true', Bool.and_eq_false_imp] at this
        exact this.1.resolve_left (by simp [ht', hp])
      · intro p x he hne hb' hx
        have := hcl' _ he
        simp only [Bool.or_eq_true, beq_iff_eq, hne, false_or, Bool.and_eq_true] at this
        exact this.2.resolve_left (by simp [hb', hx])
    have hai := hall i hi'
    have haj := hall j hj'
    simp only [beq_iff_eq] at hai haj
    rw [hai, first_eq hb, ← haj] at hmi
    exact marked_of_marks hmi
  · exact ⟨entersB_sound (hi _ hi').1.1.1.2, entersB_sound (hi _ hi').1.1.2⟩

/-- **A checked partition is stable**: on a complete run, a partition `check` accepts is
`Stable`, with each block's first state as its representative. -/
theorem stable_of_check {O XSa Ra : Type} {X : LTS (S × I) (XSa ⊕ Ra)} {obs : S → O}
    {R : ExplRun S I L} {blk : Nat → Nat} (hC : Complete X obs R) (h : check R blk = true) :
    Stable X obs (blockOf R blk) (repOf R blk) :=
  stable_of_cert hC (check_sound h)

end Check

end RdraIsh.Sweep
