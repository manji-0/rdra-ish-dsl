import RdraIsh.Reduction
import RdraIsh.Branching

/-!
# Theorem A8 with action labels, and its consequences 1 to 3

`Reduction.lean` proves A8 over unlabelled steps. Here the steps carry their labels:

- on `C`, a `T_a` step `.inl a` shows `a`, an `S_a` step is silent;
- on `R`, a `T_a` step `.inl a` shows `a`, a class move `.inr ()` is silent.

`T_a` labels are abstract: a label of `Ta` stands for an action together with its binding
(D13's step is a triple `(s, (a, p), s')`), so "the same label" means the same action with
the same parameters. `R` runs `T_a` actions unchanged, so the label a `T_a` step shows is the
same on both sides.

The `S_a` side needs one more thing, the binding of Lemma 6 ("Parameters"): an `S_a` action of
`C` and a silent edge of `X` are related by `stands : Sa → XSa → Prop`, "the edge's action
stands for the cone's action, with the parameters it binds outside the folded `T`
parameters". `Lemma6By stands` is Lemma 6 with that relation, and `class_move_lifts` says
every class move of `R` lifts to a path of `C` whose every step is matched, through `stands`,
by a silent edge of `X` between the two projections. Lemma 6 as `Reduction.lean` states it is
`Lemma6By` with `stands` everywhere true (`Lemma6.toBy`, `Lemma6By.toLemma6`).

The rises carry no `stands`: consequences 2 to 4 read only the `T_a` labels of `C` and `R`,
and `R`'s `T` move reads a rise only through the input bytes it ends at.
-/

namespace RdraIsh

open LTS

variable {T S I O Blk Ta Sa XSa Ra : Type}

/-- The visibility of a `T ⊕ S` label: a `.inl` label shows itself, a `.inr` label is silent. -/
def tLabel {A B : Type} : A ⊕ B → Option A
  | .inl a => some a
  | .inr _ => none

/-- Lemma 6 with the binding relation `stands` between `C`'s `S_a` actions and `X`'s silent
edges: an `S_a` step is a silent edge by an action that stands for it, and a silent edge is an
`S_a` step by an action it stands for. -/
structure Lemma6By (C : LTS (T × S) (Ta ⊕ Sa)) (X : LTS (S × I) (XSa ⊕ Ra)) (inp : T → I)
    (stands : Sa → XSa → Prop) : Prop where
  init : ∀ {t s}, C.init (t, s) → X.init (s, inp t)
  s_by : ∀ {t s a s'}, C.Reach (t, s) → C.step (t, s) (.inr a) (t, s') →
    ∃ b, stands a b ∧ X.step (s, inp t) (.inl b) (s', inp t)
  rise : ∀ {t s a t'}, C.Reach (t, s) → C.step (t, s) (.inl a) (t', s) →
    RiseStar X (s, inp t) (s, inp t')
  s_back_by : ∀ {t s b s'}, C.Reach (t, s) → X.step (s, inp t) (.inl b) (s', inp t) →
    ∃ a, stands a b ∧ C.step (t, s) (.inr a) (t, s')

section

variable {C : LTS (T × S) (Ta ⊕ Sa)} {X : LTS (S × I) (XSa ⊕ Ra)} {inp : T → I}
  {stands : Sa → XSa → Prop}

theorem Lemma6By.toLemma6 (h : Lemma6By C X inp stands) : Lemma6 C X inp where
  init := h.init
  s_step hr hs := let ⟨b, _, hb⟩ := h.s_by hr hs; ⟨b, hb⟩
  rise := h.rise
  s_step_back hr hb := let ⟨a, _, ha⟩ := h.s_back_by hr hb; ⟨a, ha⟩

theorem Lemma6.toBy (h : Lemma6 C X inp) : Lemma6By C X inp (fun _ _ => True) where
  init := h.init
  s_by hr hs := let ⟨b, hb⟩ := h.s_step hr hs; ⟨b, trivial, hb⟩
  rise := h.rise
  s_back_by hr hb := let ⟨a, ha⟩ := h.s_step_back hr hb; ⟨a, trivial, ha⟩

end

/-- The `S_a` steps of `C` matched, through `stands`, by a silent edge of `X` between the
projections `x(t, s)` and `x(t, s')`. -/
def matched (C : LTS (T × S) (Ta ⊕ Sa)) (X : LTS (S × I) (XSa ⊕ Ra)) (inp : T → I)
    (stands : Sa → XSa → Prop) : LTS (T × S) Sa where
  init := C.init
  step p a q := q.1 = p.1 ∧ C.step p (.inr a) q ∧
    ∃ b, stands a b ∧ X.step (p.2, inp p.1) (.inl b) (q.2, inp p.1)

/-- A path of one system is a path of another whose steps include the first's. -/
theorem LTS.PathWithin.map {σ α β : Type} {M : LTS σ α} {N : LTS σ β} {inv : σ → Prop}
    (f : ∀ s t, inv s → M.Next s t → N.Next s t) {s t : σ} (p : M.PathWithin inv s t) :
    N.PathWithin inv s t := by
  induction p with
  | refl h => exact .refl h
  | cons h hn _ ih => exact .cons h (f _ _ h hn) ih

section Proofs

variable {C : LTS (T × S) (Ta ⊕ Sa)} {X : LTS (S × I) (XSa ⊕ Ra)} {obs : S → O}
  {inp : T → I} {B : S × I → Blk} {rep : Blk → S × I} {stands : Sa → XSa → Prop}

theorem matched_invis {p q : T × S} :
    (matched C X inp stands).Next p q → (invis C tLabel).Next p q := by
  rintro ⟨a, _, hs, _⟩
  exact ⟨.inr a, hs, rfl⟩

/-- A silent path of `X` from `x(t, s)` through one block lifts to a path of matched `S_a`
steps of `C` from `(t, s)` through states related to `(t, c)`. -/
theorem lift_silent_by (h6 : Lemma6By C X inp stands) (hE : Extended X) (t : T) (c : Blk) :
    ∀ {x z}, (silent X).PathWithin (fun w => B w = c) x z →
      ∀ s, x = (s, inp t) → C.Reach (t, s) →
        ∃ s₁, z = (s₁, inp t) ∧
          (matched C X inp stands).PathWithin (fun p => rel C inp B p (t, c)) (t, s) (t, s₁) := by
  intro x z hp
  induction hp with
  | refl hin =>
    intro s hx hr
    subst hx
    exact ⟨s, rfl, .refl ⟨hr, rfl, hin.symm⟩⟩
  | @cons x u z hin hn _ ih =>
    intro s hx hr
    subst hx
    obtain ⟨b, hb⟩ := hn
    obtain ⟨s₂, i₂⟩ := u
    have hi : i₂ = inp t := hE.silent_keeps_inp hb
    subst hi
    obtain ⟨a, hst, ha⟩ := h6.s_back_by hr hb
    have hr₂ : C.Reach (t, s₂) := Reach.step hr ha
    obtain ⟨s₁, hz, hpath⟩ := ih s₂ rfl hr₂
    have hm : (matched C X inp stands).Next (t, s) (t, s₂) := ⟨a, rfl, ha, b, hst, hb⟩
    exact ⟨s₁, hz, .cons ⟨hr, rfl, hin.symm⟩ hm hpath⟩

/-- **A class move lifts** (the statement `reduce/lift.rs` realizes, claim A16, with the
binding of Lemma 6): from a state related to `(t, c)`, a class move of `R` into `c'` is a path
of matched `S_a` steps of `C` through states related to `(t, c)`, then one more matched step
into a state related to `(t, c')`. -/
theorem class_move_lifts (h5 : Split C obs) (h6 : Lemma6By C X inp stands) (hE : Extended X)
    (hX : Stable X obs B rep) {p : T × S} {q q' : T × Blk} (hr : rel C inp B p q)
    (hs : (reduced C X inp B rep).step q (.inr ()) q') :
    ∃ u u', (matched C X inp stands).PathWithin (fun p' => rel C inp B p' q) p u ∧
      (matched C X inp stands).Next u u' ∧ rel C inp B u' q' := by
  obtain ⟨t, s⟩ := p
  obtain ⟨t₀, c⟩ := q
  obtain ⟨t', c'⟩ := q'
  obtain ⟨hr, ht₀, hc⟩ := hr
  simp only at ht₀ hc
  subst ht₀ hc
  have hx := x_reach h5 h6.toLemma6.toLemma6Fwd (t₀, s) hr
  obtain ⟨hrr, hrb, _, _⟩ := rep_facts hX hx
  obtain ⟨ht, hexit⟩ := hs
  simp only at ht
  subst ht
  obtain ⟨_, z, z', hp, ⟨b, hb⟩, hz'⟩ := hX.b2 hrr hx hrb hexit
  obtain ⟨s₁, hz, hpath⟩ := lift_silent_by (c := B (s, inp t')) h6 hE t' hp s rfl hr
  subst hz
  obtain ⟨s₂, i₂⟩ := z'
  have hi : i₂ = inp t' := hE.silent_keeps_inp hb
  subst hi
  have hr₁ : C.Reach (t', s₁) := (hpath.map fun _ _ h n => matched_reach_next h n).reach hr
  obtain ⟨a, hst, ha⟩ := h6.s_back_by hr₁ hb
  exact ⟨(t', s₁), (t', s₂), hpath, ⟨a, rfl, ha, b, hst, hb⟩,
    ⟨Reach.step hr₁ ha, rfl, hz'.symm⟩⟩
where
  matched_reach_next {inv : T × S → Prop} {p q : T × S} (_ : inv p)
      (h : (matched C X inp stands).Next p q) : C.Next p q :=
    let ⟨a, _, hs, _⟩ := h; ⟨.inr a, hs⟩

/-- The forward half of Theorem A8 with labels: a `T_a` step of `C` is matched by the `T_a`
step of `R` with the same label, an `S_a` step by a stutter or a class move. -/
theorem branch_forward (h5 : Split C obs) (h6 : Lemma6Fwd C X inp)
    (hX : Stable X obs B rep) :
    BranchSim C (reduced C X inp B rep) tLabel tLabel (rel C inp B) := by
  rintro ⟨t, s⟩ ⟨t₀, c⟩ l ⟨t', s'⟩ ⟨hr, ht₀, hc⟩ hs
  simp only at ht₀ hc
  have ht₀' := ht₀.symm
  subst ht₀' hc
  have hx := x_reach h5 h6 (t, s) hr
  obtain ⟨hrr, hrb, hro, _⟩ := rep_facts hX hx
  cases l with
  | inr a =>
    have ht : t' = t := h5.s_keeps_t hs
    subst ht
    have hr' : C.Reach (t', s') := Reach.step hr hs
    obtain ⟨b, hb⟩ := h6.s_step hr hs
    by_cases hsame : B (s', inp t') = B (s, inp t')
    · exact .inl ⟨rfl, hr', rfl, hsame.symm⟩
    · refine .inr ⟨(t', B (s, inp t')), .inr (), (t', B (s', inp t')), .refl ⟨hr, rfl, rfl⟩,
        ⟨rfl, ?_⟩, rfl, ⟨hr', rfl, rfl⟩⟩
      have hexit : SilentExit X B (s, inp t') (B (s', inp t')) :=
        ⟨hsame, (s, inp t'), (s', inp t'), .refl rfl, ⟨b, hb⟩, rfl⟩
      exact hX.b2 hx hrr hrb.symm hexit
  | inl a =>
    have hs' : s' = s := h5.t_keeps_s hs
    subst hs'
    have hr' : C.Reach (t', s') := Reach.step hr hs
    have hrise := h6.rise hr hs
    obtain ⟨y, hy, hby⟩ := b3_star hX hx hrr hrb.symm hrise
    have hyr : X.Reach y := hy.reach hrr
    have hx' : X.Reach (s', inp t') := hrise.reach hx
    obtain ⟨_, hyi⟩ := hX.b1 hyr hx' hby
    refine .inr ⟨(t, B (s', inp t)), .inl a, (t', B (s', inp t')), .refl ⟨hr, rfl, rfl⟩,
      ⟨?_, y, hy, hyi, hby⟩, rfl, ⟨hr', rfl, rfl⟩⟩
    exact h5.t_reads_obs hro.symm hs

/-- A `T_a` step of `R` from `(t, c)` is the `T_a` step of `C` with the same label from every
state related to `(t, c)`, with no silent prefix. -/
theorem t_step_back (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) {p : T × S} {q q' : T × Blk} {a : Ta} (hr : rel C inp B p q)
    (hs : (reduced C X inp B rep).step q (.inl a) q') :
    C.step p (.inl a) (q'.1, p.2) ∧ rel C inp B (q'.1, p.2) q' := by
  obtain ⟨t, s⟩ := p
  obtain ⟨t₀, c⟩ := q
  obtain ⟨t', c'⟩ := q'
  obtain ⟨hr, ht₀, hc⟩ := hr
  simp only at ht₀ hc
  subst ht₀ hc
  have hx := x_reach h5 h6.toLemma6Fwd (t₀, s) hr
  obtain ⟨hrr, hrb, hro, _⟩ := rep_facts hX hx
  obtain ⟨hstep, y, hy, hyi, hyb⟩ := hs
  simp only at hstep hy hyi hyb
  have hsC : C.step (t₀, s) (.inl a) (t', s) := h5.t_reads_obs hro hstep
  have hr' : C.Reach (t', s) := Reach.step hr hsC
  have hrise := h6.rise hr hsC
  obtain ⟨w, hw, hwb⟩ := b3_star hX hx hrr hrb.symm hrise
  have hx' : X.Reach (s, inp t') := hrise.reach hx
  obtain ⟨_, hwi⟩ := hX.b1 (hw.reach hrr) hx' hwb
  have hw1 := hw.keeps_s hE
  have hy1 := hy.keeps_s hE
  have hwy : w = y := by
    obtain ⟨w1, w2⟩ := w
    obtain ⟨y1, y2⟩ := y
    simp only at hw1 hy1 hwi hyi
    rw [hw1, hy1, hwi, hyi]
  subst hwy
  exact ⟨hsC, hr', rfl, hyb.symm.trans hwb⟩

/-- The backward half of Theorem A8 with labels. -/
theorem branch_backward (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) :
    BranchSim (reduced C X inp B rep) C tLabel tLabel (fun q p => rel C inp B p q) := by
  rintro q p l q' hr hs
  cases l with
  | inr u =>
    cases u
    obtain ⟨u, u', hp, hn, hr'⟩ := class_move_lifts h5 h6.toBy hE hX hr hs
    obtain ⟨a, _, ha, _⟩ := hn
    exact .inr ⟨u, .inr a, u', hp.map fun _ _ _ h => matched_invis h, ha, rfl, hr'⟩
  | inl a =>
    obtain ⟨hsC, hr'⟩ := t_step_back h5 h6 hE hX hr hs
    exact .inr ⟨p, .inl a, (q'.1, p.2), .refl hr, hsC, rfl, hr'⟩

/-- **Theorem A8, with labels** (cones.md, Theorem A8: "preserves `T_v`, `obs`, and the
action label of every `T_a` step"): `~` is a divergence-blind branching bisimulation between
`C` and `R` in which a `T_a` step shows its label and an `S_a` step or a class move is silent,
and it preserves `T_v` and `obs`. -/
theorem a8_branching (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) :
    BranchBisim C (reduced C X inp B rep) tLabel tLabel (rel C inp B) ∧
      PreservesObs (rel C inp B) (obsC obs) (obsR obs rep) :=
  ⟨⟨branch_forward h5 h6.toLemma6Fwd hX, branch_backward h5 h6 hE hX⟩,
   rel_preserves_obs h5 h6.toLemma6Fwd hX⟩

/-- **Consequence 1, as a set**: the reachable valuations of `T_v ∪ obs` (witnesses,
vacuity) are the same on `C` and `R`. -/
theorem a8_reachable_obs_iff (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (o : T × O) :
    (∃ p, C.Reach p ∧ obsC obs p = o) ↔
      (∃ q, (reduced C X inp B rep).Reach q ∧ obsR obs rep q = o) :=
  let h := a8_branching h5 h6 hE hX
  reachable_obs_eq_of_dbsb h.1.dbsb h.2 init_forward init_backward o

/-- **Consequence 2**: the reachable `T_a` edges, by the `T_v ∪ obs` values before and after
and the label, are the same on `C` and `R`. -/
theorem a8_edges_iff (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (a : Ta) (o o' : T × O) :
    Edges C tLabel (obsC obs) a o o' ↔
      Edges (reduced C X inp B rep) tLabel (obsR obs rep) a o o' :=
  let h := a8_branching h5 h6 hE hX
  edges_iff_of_branchBisim h.1 h.2 init_forward init_backward a o o'

/-- Consequence 2 for `NeverRuns` and arms taken: a `T_a` label is on some reachable edge of
`C` iff on one of `R`. -/
theorem a8_runs_iff (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (a : Ta) :
    (∃ o o', Edges C tLabel (obsC obs) a o o') ↔
      (∃ o o', Edges (reduced C X inp B rep) tLabel (obsR obs rep) a o o') := by
  constructor
  · rintro ⟨o, o', h⟩
    exact ⟨o, o', (a8_edges_iff h5 h6 hE hX a o o').1 h⟩
  · rintro ⟨o, o', h⟩
    exact ⟨o, o', (a8_edges_iff h5 h6 hE hX a o o').2 h⟩

/-- Consequence 2 for an `after` assertion on a `T_a` action: a relation `Q` between the
`T_v ∪ obs` values before and after holds of every reachable `a` edge of `C` iff of every one
of `R`. -/
theorem a8_after_iff (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (a : Ta) (Q : T × O → T × O → Prop) :
    (∀ o o', Edges C tLabel (obsC obs) a o o' → Q o o') ↔
      (∀ o o', Edges (reduced C X inp B rep) tLabel (obsR obs rep) a o o' → Q o o') :=
  ⟨fun h o o' e => h o o' ((a8_edges_iff h5 h6 hE hX a o o').2 e),
   fun h o o' e => h o o' ((a8_edges_iff h5 h6 hE hX a o o').1 e)⟩

/-- **Consequence 3**: for an irreflexive change relation `ch` on `T_v ∪ obs` (D25: the edge
changes the column at the slot or deletes the row), `C` has a reachable state with values `o`
from which no path takes a changing edge iff `R` has one. -/
theorem a8_stuck_iff (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) {ch : T × O → T × O → Prop} (hirr : ∀ o, ¬ ch o o)
    (o : T × O) :
    Stuck C (obsC obs) ch o ↔ Stuck (reduced C X inp B rep) (obsR obs rep) ch o :=
  let h := a8_branching h5 h6 hE hX
  stuck_iff_of_dbsb hirr h.1.dbsb h.2 init_forward init_backward o

/-- Consequence 3 as a verdict: no reachable state of `C` is stuck iff none of `R` is. -/
theorem a8_deadlock_free_iff (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) {ch : T × O → T × O → Prop} (hirr : ∀ o, ¬ ch o o) :
    (∀ p, C.Reach p → CanLeave C (obsC obs) ch p) ↔
      (∀ q, (reduced C X inp B rep).Reach q → CanLeave (reduced C X inp B rep) (obsR obs rep) ch q) :=
  let h := a8_branching h5 h6 hE hX
  canLeave_all_iff_of_dbsb hirr h.1.dbsb h.2 init_forward init_backward

end Proofs

end RdraIsh
