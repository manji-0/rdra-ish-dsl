import RdraIsh.Stutter

/-!
# The reduction by an independent sub-lifecycle (cones.md, Lemma 5, Definition 4, Lemma 6,
Theorem A8)

The cone `C` has states `T × S` (the `T_v` and `S_v` valuations of Definition 3) and actions
`Ta ⊕ Sa`. `obs : S → O` is the part of `S` that `T` reads, and `inp : T → I` the input bytes
of a `T` valuation (`Reduction::input_bytes`: the raw inputs followed by the fold bits).

The extended sub-lifecycle `X` (Definition 4) has states `S × I` and actions `XSa ⊕ Ra`: a
`.inl` edge is *silent* (an `S_a` action), a `.inr` edge is a *rise*. `x(t, s) = (s, inp t)`.

`B : S × I → Blk` is the partition of Definition 4 and `rep` picks a representative state of
a block. `reduced` builds `R` from `C`, `X`, `B` and `rep`, as Definition 4 does.

What is simplified, against cones.md:

- *Flat case only.* `X` is explored without an inner reduction, so "`x(t, s) ∈ X_n` up to the
  inner canonical form" is plain reachability in `X`.
- *Rise chains.* Lemma 6 says a `T_a` step that changes the input bytes is matched by one
  rise per stage of `Reduction::stages`. Here it is matched by some finite sequence of rises
  (`RiseStar`), and a step that keeps the input bytes by the empty sequence. `R`'s `T` move
  then goes to the block of any rise-chain end with the new input bytes, which (B1), (B3) and
  `rise_keeps_s` make unique; the code's `recanon` follows the stages instead.
- *Labels.* The bisimulation is over unlabelled steps, so the preservation of `T_a` action
  labels (Theorem A8, consequence 2) is not stated. The parameter bindings of Lemma 6
  ("Parameters") are inside the step relations, not modelled.
- *Lemma 6 and (B1)–(B3) are hypotheses here.* `Lemma6.lean` derives Lemma 6 over the action
  model of `Restrict.lean`, from per-action hypotheses read from `restrict_to` and the folds;
  (B1)–(B3) stay hypotheses, and nothing reads `quotient_blocks`.
-/

namespace RdraIsh

open LTS

variable {T S I O Blk Ta Sa XSa Ra : Type}

/-! ## Silent edges, rises, and the extended sub-lifecycle -/

/-- The silent edges of `X`, as a system of their own. -/
def silent (X : LTS (S × I) (XSa ⊕ Ra)) : LTS (S × I) XSa where
  init := X.init
  step x b y := X.step x (.inl b) y

/-- A finite sequence of rises of `X`. -/
inductive RiseStar (X : LTS (S × I) (XSa ⊕ Ra)) : S × I → S × I → Prop
  | refl (x) : RiseStar X x x
  | cons {x y z} (r : Ra) : X.step x (.inr r) y → RiseStar X y z → RiseStar X x z

/-- Definition 4's shape of `X`: a silent edge keeps the input bytes, a rise keeps `S`. -/
structure Extended (X : LTS (S × I) (XSa ⊕ Ra)) : Prop where
  silent_keeps_inp : ∀ {s i b s' i'}, X.step (s, i) (.inl b) (s', i') → i' = i
  rise_keeps_s : ∀ {s i r s' i'}, X.step (s, i) (.inr r) (s', i') → s' = s

theorem RiseStar.reach {X : LTS (S × I) (XSa ⊕ Ra)} {x y : S × I}
    (h : RiseStar X x y) (hx : X.Reach x) : X.Reach y := by
  induction h with
  | refl => exact hx
  | cons _ hs _ ih => exact ih (Reach.step hx hs)

theorem RiseStar.keeps_s {X : LTS (S × I) (XSa ⊕ Ra)} (hX : Extended X) {x y : S × I}
    (h : RiseStar X x y) : y.1 = x.1 := by
  induction h with
  | refl => rfl
  | @cons x y z _ hs _ ih =>
    have : y.1 = x.1 := hX.rise_keeps_s (s := x.1) (i := x.2) (s' := y.1) (i' := y.2) hs
    rw [ih, this]

/-! ## Lemma 5 and Lemma 6 as hypotheses -/

/-- Lemma 5 (a) and (b), the closure facts of `split_at`: an `S_a` step keeps `T`; a `T_a`
step keeps `S` and reads `S` only through `obs`. -/
structure Split (C : LTS (T × S) (Ta ⊕ Sa)) (obs : S → O) : Prop where
  s_keeps_t : ∀ {t s a t' s'}, C.step (t, s) (.inr a) (t', s') → t' = t
  t_keeps_s : ∀ {t s a t' s'}, C.step (t, s) (.inl a) (t', s') → s' = s
  t_reads_obs : ∀ {t s₁ s₂ a t'}, obs s₁ = obs s₂ →
    C.step (t, s₁) (.inl a) (t', s₁) → C.step (t, s₂) (.inl a) (t', s₂)

/-- The `C ⇒ X` half of Lemma 6: initial states project to initial states, every `S_a` step
of a reachable state of `C` is a silent edge of `X` from `x(t, s)`, and every `T_a` step is
matched by rises from `x(t, s)` to `x(t', s)`. This is the half the stub-param bug (P67)
broke, and the half the soundness of an `ok` needs (`holds_of_reduced`). -/
structure Lemma6Fwd (C : LTS (T × S) (Ta ⊕ Sa)) (X : LTS (S × I) (XSa ⊕ Ra))
    (inp : T → I) : Prop where
  init : ∀ {t s}, C.init (t, s) → X.init (s, inp t)
  s_step : ∀ {t s a s'}, C.Reach (t, s) → C.step (t, s) (.inr a) (t, s') →
    ∃ b, X.step (s, inp t) (.inl b) (s', inp t)
  rise : ∀ {t s a t'}, C.Reach (t, s) → C.step (t, s) (.inl a) (t', s) →
    RiseStar X (s, inp t) (s, inp t')

/-- Lemma 6, both halves: also every silent edge of `X` from `x(t, s)` is an `S_a` step of
`C`. -/
structure Lemma6 (C : LTS (T × S) (Ta ⊕ Sa)) (X : LTS (S × I) (XSa ⊕ Ra)) (inp : T → I) :
    Prop extends Lemma6Fwd C X inp where
  s_step_back : ∀ {t s b s'}, C.Reach (t, s) → X.step (s, inp t) (.inl b) (s', inp t) →
    ∃ a, C.step (t, s) (.inr a) (t, s')

/-! ## The partition -/

/-- `x` leaves its block into `c'` by a silent path through its own block followed by one
silent edge (the condition of (B2)). -/
def SilentExit (X : LTS (S × I) (XSa ⊕ Ra)) (B : S × I → Blk) (x : S × I) (c' : Blk) :
    Prop :=
  c' ≠ B x ∧ ∃ z z', (silent X).PathWithin (fun w => B w = B x) x z ∧
    (silent X).Next z z' ∧ B z' = c'

/-- (B1)–(B3) on the reachable states of `X` (Definition 4), with `rep` picking a
reachable state of the block of every reachable state. -/
structure Stable (X : LTS (S × I) (XSa ⊕ Ra)) (obs : S → O) (B : S × I → Blk)
    (rep : Blk → S × I) : Prop where
  /-- (B1): a block agrees on `obs` and the input bytes. -/
  b1 : ∀ {x y}, X.Reach x → X.Reach y → B x = B y → obs x.1 = obs y.1 ∧ x.2 = y.2
  /-- (B2): divergence-blind stuttering stability under silent edges. -/
  b2 : ∀ {x y c'}, X.Reach x → X.Reach y → B x = B y → SilentExit X B x c' →
    SilentExit X B y c'
  /-- (B3): strong stability under rises. -/
  b3 : ∀ {x y r x'}, X.Reach x → X.Reach y → B x = B y → X.step x (.inr r) x' →
    ∃ r' y', X.step y (.inr r') y' ∧ B y' = B x'
  rep_block : ∀ {x}, X.Reach x → B (rep (B x)) = B x
  rep_reach : ∀ {x}, X.Reach x → X.Reach (rep (B x))

/-! ## The reduced cone `R` -/

/-- `R` (Definition 4): states are `T` valuations with a block; a `T_a` action runs at the
block's representative, then moves the block by rises to the new input bytes; a class move
`.inr ()` is a silent exit of the representative. -/
def reduced (C : LTS (T × S) (Ta ⊕ Sa)) (X : LTS (S × I) (XSa ⊕ Ra)) (inp : T → I)
    (B : S × I → Blk) (rep : Blk → S × I) : LTS (T × Blk) (Ta ⊕ Unit) where
  init p := ∃ s, C.init (p.1, s) ∧ p.2 = B (s, inp p.1)
  step p l q := match l with
    | .inl a => C.step (p.1, (rep p.2).1) (.inl a) (q.1, (rep p.2).1) ∧
        ∃ y, RiseStar X (rep p.2) y ∧ y.2 = inp q.1 ∧ B y = q.2
    | .inr () => q.1 = p.1 ∧ SilentExit X B (rep p.2) q.2

/-- The relation of Theorem A8: `(t, s) ~ (t, c)` when `(t, s)` is reachable and `c` is the
block of `x(t, s)`. -/
def rel (C : LTS (T × S) (Ta ⊕ Sa)) (inp : T → I) (B : S × I → Blk) (p : T × S)
    (q : T × Blk) : Prop :=
  C.Reach p ∧ q.1 = p.1 ∧ q.2 = B (p.2, inp p.1)

/-- The observation of both sides: `T_v` and `obs`. -/
def obsC (obs : S → O) (p : T × S) : T × O := (p.1, obs p.2)
def obsR (obs : S → O) (rep : Blk → S × I) (q : T × Blk) : T × O := (q.1, obs (rep q.2).1)

section Proofs

variable {C : LTS (T × S) (Ta ⊕ Sa)} {X : LTS (S × I) (XSa ⊕ Ra)} {obs : S → O}
  {inp : T → I} {B : S × I → Blk} {rep : Blk → S × I}

/-- Lemma 6's first conjunct, derived: `x(t, s) ∈ Reach(X)` for every reachable `(t, s)`. -/
theorem x_reach (h5 : Split C obs) (h6 : Lemma6Fwd C X inp) :
    ∀ p, C.Reach p → X.Reach (p.2, inp p.1) := by
  intro p hp
  induction hp with
  | init h => exact Reach.init (h6.init h)
  | @step p l q hp hs ih =>
    obtain ⟨t, s⟩ := p
    obtain ⟨t', s'⟩ := q
    cases l with
    | inr a =>
      have ht : t' = t := h5.s_keeps_t hs
      subst ht
      obtain ⟨b, hb⟩ := h6.s_step hp hs
      exact Reach.step ih hb
    | inl a =>
      have hs' : s' = s := h5.t_keeps_s hs
      subst hs'
      exact (h6.rise hp hs).reach ih

/-- (B3) along a sequence of rises. -/
theorem b3_star (hX : Stable X obs B rep) {x y x' : S × I} (hx : X.Reach x)
    (hy : X.Reach y) (hb : B x = B y) (h : RiseStar X x x') :
    ∃ y', RiseStar X y y' ∧ B y' = B x' := by
  induction h generalizing y with
  | refl => exact ⟨y, .refl y, hb.symm⟩
  | @cons x x₁ z r hs _ ih =>
    obtain ⟨r', y₁, hy₁, hb₁⟩ := hX.b3 hx hy hb hs
    obtain ⟨y', hy', hb'⟩ :=
      ih (Reach.step hx hs) (Reach.step hy hy₁) hb₁.symm
    exact ⟨y', .cons r' hy₁ hy', hb'⟩

/-- The representative of the block of a reachable `x(t, s)` is reachable, in that block,
and agrees with `x(t, s)` on `obs` and the input bytes. -/
theorem rep_facts (hX : Stable X obs B rep) {x : S × I} (hx : X.Reach x) :
    X.Reach (rep (B x)) ∧ B (rep (B x)) = B x ∧ obs (rep (B x)).1 = obs x.1 ∧
      (rep (B x)).2 = x.2 := by
  have hr := hX.rep_reach hx
  have hb := hX.rep_block hx
  obtain ⟨ho, hi⟩ := hX.b1 hr hx hb
  exact ⟨hr, hb, ho, hi⟩

/-- A silent path of `X` from `x(t, s)` through one block lifts to a path of `S_a` steps of
`C` from `(t, s)` through states related to `(t, c)` (Theorem A8 point 3, converse). -/
theorem lift_silent (h6 : Lemma6 C X inp) (hE : Extended X) (t : T) (c : Blk) :
    ∀ {x z}, (silent X).PathWithin (fun w => B w = c) x z →
      ∀ s, x = (s, inp t) → C.Reach (t, s) →
        ∃ s₁, z = (s₁, inp t) ∧
          C.PathWithin (fun p => rel C inp B p (t, c)) (t, s) (t, s₁) := by
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
    obtain ⟨a, ha⟩ := h6.s_step_back hr hb
    have hr₂ : C.Reach (t, s₂) := Reach.step hr ha
    obtain ⟨s₁, hz, hpath⟩ := ih s₂ rfl hr₂
    exact ⟨s₁, hz, .cons ⟨hr, rfl, hin.symm⟩ ⟨_, ha⟩ hpath⟩

/-- The forward half of Theorem A8: `~` is a stuttering simulation from `C` to `R`. It
uses only the `C ⇒ X` half of Lemma 6. -/
theorem sim_forward (h5 : Split C obs) (h6 : Lemma6Fwd C X inp)
    (hX : Stable X obs B rep) :
    StutterSim C (reduced C X inp B rep) (rel C inp B) := by
  rintro ⟨t, s⟩ ⟨t₀, c⟩ ⟨t', s'⟩ ⟨hr, ht₀, hc⟩ ⟨l, hs⟩
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
    · exact .inl ⟨hr', rfl, hsame.symm⟩
    · refine .inr ⟨(t', B (s, inp t')), (t', B (s', inp t')), .refl ⟨hr, rfl, rfl⟩,
        ⟨.inr (), rfl, ?_⟩, ⟨hr', rfl, rfl⟩⟩
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
    refine .inr ⟨(t, B (s', inp t)), (t', B (s', inp t')), .refl ⟨hr, rfl, rfl⟩,
      ⟨.inl a, ?_, y, hy, hyi, hby⟩, ⟨hr', rfl, rfl⟩⟩
    exact h5.t_reads_obs hro.symm hs

/-- The backward half of Theorem A8: `~` is a stuttering simulation from `R` to `C`. It
needs the `X ⇒ C` half of Lemma 6. -/
theorem sim_backward (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) :
    StutterSim (reduced C X inp B rep) C (fun q p => rel C inp B p q) := by
  rintro ⟨t₀, c⟩ ⟨t, s⟩ ⟨t', c'⟩ ⟨hr, ht₀, hc⟩ ⟨l, hs⟩
  simp only at ht₀ hc
  subst ht₀ hc
  have hx := x_reach h5 h6.toLemma6Fwd (t₀, s) hr
  obtain ⟨hrr, hrb, hro, _⟩ := rep_facts hX hx
  cases l with
  | inr u =>
    obtain ⟨ht, hexit⟩ := hs
    simp only at ht
    subst ht
    obtain ⟨hne, z, z', hp, ⟨b, hb⟩, hz'⟩ := hX.b2 hrr hx hrb hexit
    obtain ⟨s₁, hz, hpath⟩ := lift_silent (c := B (s, inp t')) h6 hE t' hp s rfl hr
    subst hz
    obtain ⟨s₂, i₂⟩ := z'
    have hi : i₂ = inp t' := hE.silent_keeps_inp hb
    subst hi
    have hr₁ : C.Reach (t', s₁) := hpath.reach hr
    obtain ⟨a, ha⟩ := h6.s_step_back hr₁ hb
    exact .inr ⟨(t', s₁), (t', s₂), hpath, ⟨_, ha⟩, ⟨Reach.step hr₁ ha, rfl, hz'.symm⟩⟩
  | inl a =>
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
    exact .inr ⟨(t₀, s), (t', s), .refl ⟨hr, rfl, rfl⟩, ⟨_, hsC⟩,
      ⟨hr', rfl, hyb.symm.trans hwb⟩⟩

theorem init_forward : InitCovered C (reduced C X inp B rep) (rel C inp B) := by
  rintro ⟨t, s⟩ h
  exact ⟨(t, B (s, inp t)), ⟨s, h, rfl⟩, ⟨Reach.init h, rfl, rfl⟩⟩

theorem init_backward :
    InitCovered (reduced C X inp B rep) C (fun q p => rel C inp B p q) := by
  rintro ⟨t, c⟩ ⟨s, h, hc⟩
  exact ⟨(t, s), h, ⟨Reach.init h, rfl, hc⟩⟩

theorem rel_preserves_obs (h5 : Split C obs) (h6 : Lemma6Fwd C X inp)
    (hX : Stable X obs B rep) : PreservesObs (rel C inp B) (obsC obs) (obsR obs rep) := by
  rintro ⟨t, s⟩ ⟨t₀, c⟩ ⟨hr, ht₀, hc⟩
  simp only at ht₀ hc
  subst ht₀ hc
  obtain ⟨_, _, hro, _⟩ := rep_facts hX (x_reach h5 h6 (t₀, s) hr)
  simp only [obsC, obsR, hro]

/-- **Theorem A8, the bisimulation** (cones.md, Theorem A8, from Lemma 5 (a)(b), Lemma 6 and
(B1)–(B3)): `~` is a divergence-blind stuttering bisimulation between `C` and `R` that
preserves `T_v` and `obs`. -/
theorem a8_dbsb (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) :
    DBSB C (reduced C X inp B rep) (rel C inp B) ∧
      PreservesObs (rel C inp B) (obsC obs) (obsR obs rep) :=
  ⟨⟨sim_forward h5 h6.toLemma6Fwd hX, sim_backward h5 h6 hE hX⟩,
   rel_preserves_obs h5 h6.toLemma6Fwd hX⟩

/-- **Theorem A8, consequence 1, for invariants**: an invariant over `T_v ∪ obs` holds on
`C` iff it holds on `R`. -/
theorem a8_holds_iff (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (P : T × O → Prop) :
    C.Holds (fun p => P (obsC obs p)) ↔
      (reduced C X inp B rep).Holds (fun q => P (obsR obs rep q)) := by
  obtain ⟨hR, hobs⟩ := a8_dbsb h5 h6 hE hX
  exact holds_iff_of_dbsb hR hobs init_forward init_backward P

/-- **The soundness of an `ok`** (Theorem A8 point 1, "the inclusion of `C`'s in `R`'s"):
an invariant that holds on `R` holds on `C`, from the `C ⇒ X` half of Lemma 6 alone. -/
theorem holds_of_reduced (h5 : Split C obs) (h6 : Lemma6Fwd C X inp)
    (hX : Stable X obs B rep) (P : T × O → Prop) :
    (reduced C X inp B rep).Holds (fun q => P (obsR obs rep q)) →
      C.Holds (fun p => P (obsC obs p)) :=
  holds_of_stutterSim (sim_forward h5 h6 hX) (rel_preserves_obs h5 h6 hX) init_forward P

end Proofs

end RdraIsh
