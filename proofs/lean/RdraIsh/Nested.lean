import RdraIsh.ReductionLabels

/-!
# Nested reductions

When `X` is itself reduced, the outer partition is computed on the states of the inner
reduced run, not on `Reach X` (`reduce/partition.rs::Classes::of` explores `X` with
`reduced_in`, whose seeds are `obs` with the rises kept). So the inner split puts `obs` and
the input bytes on its `T` side and the rises among its kept, visible `T_a` actions; the other
inner `T_a` actions and the inner `S_a` actions are silent edges of `X`.

The proof is by induction on the depth of nesting:

- `InnerMap X Y κ` states what an inner reduction gives the outer one: `κ` maps the reachable
  states of `X` onto those of `Y`, keeps the observation, maps a silent edge to a silent edge
  or a stutter, maps a rise to a rise with the same label, lifts a silent edge of `Y` to a
  silent path through one `κ`-class then a silent edge, and lifts a rise of `Y` to a rise
  from every state of the class, with no silent prefix. Visibility is a map
  `r : α → Option Ra`: `none` for an edge the outer reduction calls silent, `some ρ` for the
  rise `ρ`.
- `a8_innerMap`: the construction of `Reduction.lean`, applied to an inner cone `C'` whose
  rises are among its `T_a` actions, gives an `InnerMap` from `C'` to its reduced cone.
  `InnerMap.ofIso` covers the reindexing of `X`'s states `S × I` as the inner cone's
  `T' × S'`, and `InnerMap.trans` composes the levels.
- `stableV_pullback`: a partition stable on `Y` pulls back along an `InnerMap` to a partition
  stable on `X`. `Tower` is the inductive definition of "explored under nested reductions to
  some depth, with a stable partition on the innermost run"; `Tower.stable` pulls the
  partition back level by level.
- `a8_nested`: Theorem A8 for `C` with an `X` explored under a tower of reductions.

What stays open: the outer representative. The code takes the representative of an outer
block among the inner run's states, so its `S` bytes are an inner representative's, which
need not be a reachable state of `X`. Here the representative is a reachable state of `X` in
the block (`exists_rep`). `R` reads the representative only through `obs` and its rises, which
(B1) and (B3) fix on the block, but that the inner representative's `obs` and rises are those
of a reachable member is not derived here.
-/

namespace RdraIsh

open LTS

variable {P Q W K Blk Ra : Type} {α β γ : Type}

/-- (B2) for any visibility: `x` leaves its block into `c'` by a silent path through the block,
then one silent edge. -/
def SilentExitV (M : LTS P α) (r : α → Option Ra) (B : P → Blk) (x : P) (c' : Blk) : Prop :=
  c' ≠ B x ∧ ∃ z z', (invis M r).PathWithin (fun w => B w = B x) x z ∧
    (invis M r).Next z z' ∧ B z' = c'

/-- (B1)–(B3) for any visibility, on the reachable states. -/
structure StableV (M : LTS P α) (r : α → Option Ra) (o : P → K) (B : P → Blk) : Prop where
  b1 : ∀ {x y}, M.Reach x → M.Reach y → B x = B y → o x = o y
  b2 : ∀ {x y c'}, M.Reach x → M.Reach y → B x = B y → SilentExitV M r B x c' →
    SilentExitV M r B y c'
  b3 : ∀ {x y a ρ x'}, M.Reach x → M.Reach y → B x = B y → M.step x a x' → r a = some ρ →
    ∃ a' ρ' y', M.step y a' y' ∧ r a' = some ρ' ∧ B y' = B x'

/-- What an inner reduction gives the outer one (see the module doc). -/
structure InnerMap (X : LTS P α) (rX : α → Option Ra) (Y : LTS Q β) (rY : β → Option Ra)
    (κ : P → Q) (oP : P → K) (oQ : Q → K) : Prop where
  init : ∀ {x}, X.init x → Y.init (κ x)
  init_back : ∀ {q}, Y.init q → ∃ x, X.init x ∧ κ x = q
  silent_fwd : ∀ {x a x'}, X.Reach x → X.step x a x' → rX a = none →
    κ x' = κ x ∨ ∃ b, Y.step (κ x) b (κ x') ∧ rY b = none
  rise_fwd : ∀ {x a ρ x'}, X.Reach x → X.step x a x' → rX a = some ρ →
    ∃ b, Y.step (κ x) b (κ x') ∧ rY b = some ρ
  silent_back : ∀ {x b q'}, X.Reach x → Y.step (κ x) b q' → rY b = none →
    ∃ z z', (invis X rX).PathWithin (fun w => κ w = κ x) x z ∧ (invis X rX).Next z z' ∧
      κ z' = q'
  rise_back : ∀ {x b ρ q'}, X.Reach x → Y.step (κ x) b q' → rY b = some ρ →
    ∃ a x', X.step x a x' ∧ rX a = some ρ ∧ κ x' = q'
  obs : ∀ {x}, X.Reach x → oQ (κ x) = oP x

/-- Glue: a path, one step, a path, all within `inv`. -/
theorem LTS.PathWithin.glue {σ δ : Type} {M : LTS σ δ} {inv : σ → Prop} {a b c d : σ}
    (p : M.PathWithin inv a b) (h : M.Next b c) (q : M.PathWithin inv c d) :
    M.PathWithin inv a d := by
  induction p with
  | refl hb => exact .cons hb h q
  | cons ha hn _ ih => exact .cons ha hn (ih h)

theorem LTS.PathWithin.mono {σ δ : Type} {M : LTS σ δ} {inv inv' : σ → Prop} {a b : σ}
    (f : ∀ s, inv s → inv' s) (p : M.PathWithin inv a b) : M.PathWithin inv' a b := by
  induction p with
  | refl h => exact .refl (f _ h)
  | cons h hn _ ih => exact .cons (f _ h) hn ih

theorem invis_reach {σ δ L : Type} {M : LTS σ δ} {v : δ → Option L} {inv : σ → Prop} {a b : σ}
    (p : (invis M v).PathWithin inv a b) (ha : M.Reach a) : M.Reach b :=
  p.of_invis.reach ha

namespace InnerMap

variable {X : LTS P α} {rX : α → Option Ra} {Y : LTS Q β} {rY : β → Option Ra}
  {κ : P → Q} {oP : P → K} {oQ : Q → K}

theorem reach (h : InnerMap X rX Y rY κ oP oQ) {x : P} (hx : X.Reach x) : Y.Reach (κ x) := by
  induction hx with
  | init hi => exact Reach.init (h.init hi)
  | @step x a x' hx hs ih =>
    cases hr : rX a with
    | none =>
      rcases h.silent_fwd hx hs hr with he | ⟨b, hb, _⟩
      · rw [he]; exact ih
      · exact Reach.step ih hb
    | some ρ =>
      obtain ⟨b, hb, _⟩ := h.rise_fwd hx hs hr
      exact Reach.step ih hb

theorem reach_back (h : InnerMap X rX Y rY κ oP oQ) {q : Q} (hq : Y.Reach q) :
    ∃ x, X.Reach x ∧ κ x = q := by
  induction hq with
  | init hi =>
    obtain ⟨x, hx, he⟩ := h.init_back hi
    exact ⟨x, Reach.init hx, he⟩
  | @step q b q' _ hs ih =>
    obtain ⟨x, hx, rfl⟩ := ih
    cases hr : rY b with
    | none =>
      obtain ⟨z, z', hp, ⟨a, ha, _⟩, he⟩ := h.silent_back hx hs hr
      exact ⟨z', Reach.step (invis_reach hp hx) ha, he⟩
    | some ρ =>
      obtain ⟨a, x', ha, _, he⟩ := h.rise_back hx hs hr
      exact ⟨x', Reach.step hx ha, he⟩

/-- A silent path of `X` within a pulled-back block maps to a silent path of `Y` within the
block. -/
theorem path_fwd (h : InnerMap X rX Y rY κ oP oQ) {B : Q → Blk} {c : Blk} {x z : P}
    (p : (invis X rX).PathWithin (fun w => B (κ w) = c) x z) (hx : X.Reach x) :
    (invis Y rY).PathWithin (fun w => B w = c) (κ x) (κ z) := by
  induction p with
  | refl hb => exact .refl hb
  | @cons x u z hb hn _ ih =>
    obtain ⟨a, ha, hr⟩ := hn
    have hu : X.Reach u := Reach.step hx ha
    rcases h.silent_fwd hx ha hr with he | ⟨b, hb', hrb⟩
    · rw [← he]; exact ih hu
    · exact .cons hb ⟨b, hb', hrb⟩ (ih hu)

/-- A silent path of `Y` within a block from `κ y`, then a silent edge, lifts to a silent path
of `X` from `y` within the pulled-back block, then a silent edge. -/
theorem path_back (h : InnerMap X rX Y rY κ oP oQ) {B : Q → Blk} {c : Blk} {q q₁ q' : Q}
    (p : (invis Y rY).PathWithin (fun w => B w = c) q q₁) (hn : (invis Y rY).Next q₁ q')
    {y : P} (hy : X.Reach y) (he : κ y = q) :
    ∃ z z', (invis X rX).PathWithin (fun w => B (κ w) = c) y z ∧ (invis X rX).Next z z' ∧
      κ z' = q' := by
  induction p generalizing y with
  | refl hb =>
    subst he
    obtain ⟨b, hb', hr⟩ := hn
    obtain ⟨z, z', hp, hz, hz'⟩ := h.silent_back hy hb' hr
    exact ⟨z, z', hp.mono fun w (hw : κ w = κ y) => by rw [hw]; exact hb, hz, hz'⟩
  | @cons q u q₁ hb hn₀ _ ih =>
    subst he
    obtain ⟨b, hb', hr⟩ := hn₀
    obtain ⟨z₁, z₁', hp₁, hz₁, hz₁'⟩ := h.silent_back hy hb' hr
    have hr₁ : X.Reach z₁' := by
      obtain ⟨a, ha, _⟩ := hz₁
      exact Reach.step (invis_reach hp₁ hy) ha
    obtain ⟨z, z', hp, hz, hz'⟩ := ih hn hr₁ hz₁'
    exact ⟨z, z', (hp₁.mono fun w (hw : κ w = κ y) => by rw [hw]; exact hb).glue hz₁ hp,
      hz, hz'⟩

/-- The identity is an inner map: depth zero. -/
theorem id (X : LTS P α) (rX : α → Option Ra) (oP : P → K) :
    InnerMap X rX X rX (fun x => x) oP oP where
  init h := h
  init_back h := ⟨_, h, rfl⟩
  silent_fwd _ hs hr := .inr ⟨_, hs, hr⟩
  rise_fwd _ hs hr := ⟨_, hs, hr⟩
  silent_back _ hs hr := ⟨_, _, .refl rfl, ⟨_, hs, hr⟩, rfl⟩
  rise_back _ hs hr := ⟨_, _, hs, hr, rfl⟩
  obs _ := rfl

/-- Inner maps compose: one more level of nesting. -/
theorem trans {Z : LTS W γ} {rZ : γ → Option Ra} {κ' : Q → W} {oW : W → K}
    (h : InnerMap X rX Y rY κ oP oQ) (h' : InnerMap Y rY Z rZ κ' oQ oW) :
    InnerMap X rX Z rZ (fun x => κ' (κ x)) oP oW where
  init hi := h'.init (h.init hi)
  init_back hi := by
    obtain ⟨q, hq, rfl⟩ := h'.init_back hi
    obtain ⟨x, hx, rfl⟩ := h.init_back hq
    exact ⟨x, hx, rfl⟩
  silent_fwd hx hs hr := by
    rcases h.silent_fwd hx hs hr with he | ⟨b, hb, hrb⟩
    · exact .inl (by rw [he])
    · rcases h'.silent_fwd (h.reach hx) hb hrb with he | ⟨c, hc, hrc⟩
      · exact .inl he
      · exact .inr ⟨c, hc, hrc⟩
  rise_fwd hx hs hr := by
    obtain ⟨b, hb, hrb⟩ := h.rise_fwd hx hs hr
    exact h'.rise_fwd (h.reach hx) hb hrb
  silent_back {x _ _} hx hs hr := by
    obtain ⟨u, u', hp, hn, hu'⟩ := h'.silent_back (h.reach hx) hs hr
    have hp' : (invis Y rY).PathWithin (fun w => (fun w => κ' w) w = κ' (κ x)) (κ x) u :=
      hp
    obtain ⟨z, z', hz, hzn, hz'⟩ := h.path_back hp' hn hx rfl
    exact ⟨z, z', hz, hzn, by rw [hz', hu']⟩
  rise_back hx hs hr := by
    obtain ⟨b, q', hb, hrb, rfl⟩ := h'.rise_back (h.reach hx) hs hr
    obtain ⟨a, x', ha, hra, rfl⟩ := h.rise_back hx hb hrb
    exact ⟨a, x', ha, hra, rfl⟩
  obs hx := by rw [h'.obs (h.reach hx), h.obs hx]

/-- An isomorphism of systems that keeps visibility and observation is an inner map: the
reindexing of `X`'s states `S × I` as an inner cone's `T' × S'`. -/
theorem ofIso (e : P → Q) (e' : Q → P) (fa : α → β) (ga : β → α)
    (he : ∀ x, e' (e x) = x) (he' : ∀ q, e (e' q) = q)
    (hfa : ∀ a, rY (fa a) = rX a) (hga : ∀ b, rX (ga b) = rY b)
    (hinit : ∀ {x}, X.init x → Y.init (e x)) (hinit' : ∀ {q}, Y.init q → X.init (e' q))
    (hstep : ∀ {x a x'}, X.step x a x' → Y.step (e x) (fa a) (e x'))
    (hstep' : ∀ {q b q'}, Y.step q b q' → X.step (e' q) (ga b) (e' q'))
    (hobs : ∀ x, oQ (e x) = oP x) : InnerMap X rX Y rY e oP oQ where
  init h := hinit h
  init_back {q} h := ⟨e' q, hinit' h, he' q⟩
  silent_fwd _ hs hr := .inr ⟨_, hstep hs, (hfa _).trans hr⟩
  rise_fwd _ hs hr := ⟨_, hstep hs, (hfa _).trans hr⟩
  silent_back {x b q'} _ hs hr := by
    have := hstep' hs
    rw [he] at this
    exact ⟨x, e' q', .refl rfl, ⟨ga b, this, (hga b).trans hr⟩, he' q'⟩
  rise_back {x b ρ q'} _ hs hr := by
    have := hstep' hs
    rw [he] at this
    exact ⟨ga b, e' q', this, (hga b).trans hr, he' q'⟩
  obs _ := hobs _

/-- The observation can be read through any function. -/
theorem comap (h : InnerMap X rX Y rY κ oP oQ) {K' : Type} (g : K → K') :
    InnerMap X rX Y rY κ (fun x => g (oP x)) (fun q => g (oQ q)) :=
  { h with obs := fun hx => by simp only [h.obs hx] }

end InnerMap

/-- **A stable partition pulls back** along an inner map: `B ∘ κ` satisfies (B1)–(B3) on `X`
when `B` does on `Y`. -/
theorem stableV_pullback {X : LTS P α} {rX : α → Option Ra} {Y : LTS Q β}
    {rY : β → Option Ra} {κ : P → Q} {oP : P → K} {oQ : Q → K} {B : Q → Blk}
    (h : InnerMap X rX Y rY κ oP oQ) (hY : StableV Y rY oQ B) :
    StableV X rX oP (fun x => B (κ x)) where
  b1 hx hy hb := by
    rw [← h.obs hx, ← h.obs hy]
    exact hY.b1 (h.reach hx) (h.reach hy) hb
  b2 {x y c'} hx hy hb hexit := by
    obtain ⟨hne, z, z', hp, ⟨a, ha, hr⟩, hz'⟩ := hexit
    have hpY := h.path_fwd hp hx
    have hz : X.Reach z := invis_reach hp hx
    have hexitY : SilentExitV Y rY B (κ x) c' := by
      rcases h.silent_fwd hz ha hr with he | ⟨b, hb', hrb⟩
      · exfalso
        apply hne
        have hl := hp.last
        simp only at hz' hl ⊢
        rw [← hz', he]
        exact hl
      · exact ⟨hne, κ z, κ z', hpY, ⟨b, hb', hrb⟩, hz'⟩
    obtain ⟨hne', q₁, q', hq, hqn, hq'⟩ := hY.b2 (h.reach hx) (h.reach hy) hb hexitY
    obtain ⟨w, w', hw, hwn, hw'⟩ := h.path_back hq hqn hy rfl
    exact ⟨hne', w, w', hw, hwn, by simp only [hw', hq']⟩
  b3 {x y a ρ x'} hx hy hb hs hr := by
    obtain ⟨b, hb', hrb⟩ := h.rise_fwd hx hs hr
    obtain ⟨b', ρ', q', hq, hrq, hq'⟩ := hY.b3 (h.reach hx) (h.reach hy) hb hb' hrb
    obtain ⟨a', y', ha', hra', rfl⟩ := h.rise_back hy hq hrq
    exact ⟨a', ρ', y', ha', hra', hq'⟩

/-- **`X` explored under nested reductions to some depth, with the partition `B`**: either
`B` is stable on `X` itself (depth zero, the flat case), or `X` has an inner map `κ` onto a
system explored so with a partition `B'`, and `B` is `B'` read through `κ`. The index `B` is
the partition the outer reduction uses: the innermost one, read through every level. -/
inductive Tower : {P : Type} → {α : Type} → {Blk : Type} → LTS P α → (α → Option Ra) →
    (P → K) → (P → Blk) → Prop
  | flat {P α Blk : Type} {X : LTS P α} {rX : α → Option Ra} {oP : P → K}
      (B : P → Blk) : StableV X rX oP B → Tower X rX oP B
  | inner {P Q α β Blk : Type} {X : LTS P α} {rX : α → Option Ra} {oP : P → K}
      {Y : LTS Q β} {rY : β → Option Ra} {oQ : Q → K} (κ : P → Q) {B : Q → Blk} :
      InnerMap X rX Y rY κ oP oQ → Tower Y rY oQ B → Tower X rX oP (fun x => B (κ x))

/-- By induction on the depth: the partition of a tower is stable on `X`. -/
theorem Tower.stable {X : LTS P α} {rX : α → Option Ra} {oP : P → K} {B : P → Blk}
    (h : Tower X rX oP B) : StableV X rX oP B := by
  induction h with
  | flat B hB => exact hB
  | inner κ hκ _ ih => exact stableV_pullback hκ ih

/-! ## The inner reduction is an inner map -/

section Inner

variable {T' S' I' O' Blk' Ta' Sa' XSa' Ra' : Type}
  {C' : LTS (T' × S') (Ta' ⊕ Sa')} {X' : LTS (S' × I') (XSa' ⊕ Ra')} {obs' : S' → O'}
  {inp' : T' → I'} {B' : S' × I' → Blk'} {rep' : Blk' → S' × I'}

/-- The map of the inner reduction: `(t, s) ↦ (t, B(x(t, s)))`. -/
def innerKey (inp' : T' → I') (B' : S' × I' → Blk') (p : T' × S') : T' × Blk' :=
  (p.1, B' (p.2, inp' p.1))

theorem rel_iff_key {p : T' × S'} {q : T' × Blk'} (hp : C'.Reach p) :
    rel C' inp' B' p q ↔ q = innerKey inp' B' p := by
  obtain ⟨t, s⟩ := p
  obtain ⟨t', c⟩ := q
  constructor
  · rintro ⟨_, h1, h2⟩
    simp only at h1 h2
    simp only [innerKey, h1, h2]
  · intro h
    simp only [innerKey, Prod.mk.injEq] at h
    obtain ⟨rfl, rfl⟩ := h
    exact ⟨hp, rfl, rfl⟩

/-- **The inner reduction is an inner map** (A8, read as what an outer reduction needs): for
an inner cone whose rises are among its `T_a` actions (`ρv a = some ρ`), the map
`(t, s) ↦ (t, B(x(t, s)))` from `C'` to its reduced cone keeps `T_v ∪ obs`, maps silent edges
to silent edges or stutters, maps a rise to the same rise, and lifts back both ways. -/
theorem a8_innerMap (h5 : Split C' obs') (h6 : Lemma6 C' X' inp') (hE : Extended X')
    (hX : Stable X' obs' B' rep') (ρv : Ta' → Option Ra) :
    InnerMap C' (fun l => (tLabel l).bind ρv) (reduced C' X' inp' B' rep')
      (fun l => (tLabel l).bind ρv) (innerKey inp' B') (obsC obs') (obsR obs' rep') where
  init {p} h := ⟨p.2, h, rfl⟩
  init_back {q} h := by
    obtain ⟨s, hs, hc⟩ := h
    exact ⟨(q.1, s), hs, by simp only [innerKey, ← hc]⟩
  silent_fwd {p a p'} hp hs hr := by
    have hrel : rel C' inp' B' p (innerKey inp' B' p) := (rel_iff_key hp).2 rfl
    have hp' : C'.Reach p' := Reach.step hp hs
    rcases branch_forward h5 h6.toLemma6Fwd hX p _ a p' hrel hs with
      ⟨_, hr'⟩ | ⟨y, b, y', hpath, hb, hv, hr'⟩
    · exact .inl ((rel_iff_key hp').1 hr').symm
    · have hy : y = innerKey inp' B' p := by
        have := hpath.last
        exact (rel_iff_key hp).1 this
      rw [hy, (rel_iff_key hp').1 hr'] at hb
      exact .inr ⟨b, hb, by simp only [hv, hr]⟩
  rise_fwd {p a ρ p'} hp hs hr := by
    have hrel : rel C' inp' B' p (innerKey inp' B' p) := (rel_iff_key hp).2 rfl
    have hp' : C'.Reach p' := Reach.step hp hs
    rcases branch_forward h5 h6.toLemma6Fwd hX p _ a p' hrel hs with
      ⟨hn, _⟩ | ⟨y, b, y', hpath, hb, hv, hr'⟩
    · simp only [hn, Option.bind_none] at hr
      cases hr
    · have hy : y = innerKey inp' B' p := (rel_iff_key hp).1 hpath.last
      rw [hy, (rel_iff_key hp').1 hr'] at hb
      exact ⟨b, hb, by simp only [hv, hr]⟩
  silent_back {p b q'} hp hs hr := by
    have hrel : rel C' inp' B' p (innerKey inp' B' p) := (rel_iff_key hp).2 rfl
    cases b with
    | inr u =>
      cases u
      obtain ⟨u, u', hpath, hn, hr'⟩ := class_move_lifts h5 h6.toBy hE hX hrel hs
      obtain ⟨a, _, ha, _⟩ := hn
      refine ⟨u, u', ?_, ⟨.inr a, ha, rfl⟩, ((rel_iff_key hr'.1).1 hr').symm⟩
      refine (PathWithin.map (N := invis C' (fun l => (tLabel l).bind ρv))
        (fun w w' _ hn => ?_) hpath).mono fun w hw => ((rel_iff_key hw.1).1 hw).symm
      obtain ⟨a, _, hs, _⟩ := hn
      exact ⟨.inr a, hs, rfl⟩
    | inl a =>
      obtain ⟨hsC, hr'⟩ := t_step_back h5 h6 hE hX hrel hs
      exact ⟨p, _, .refl rfl, ⟨.inl a, hsC, hr⟩, ((rel_iff_key hr'.1).1 hr').symm⟩
  rise_back {p b ρ q'} hp hs hr := by
    have hrel : rel C' inp' B' p (innerKey inp' B' p) := (rel_iff_key hp).2 rfl
    cases b with
    | inr u => simp only [tLabel, Option.bind_none] at hr; cases hr
    | inl a =>
      obtain ⟨hsC, hr'⟩ := t_step_back h5 h6 hE hX hrel hs
      exact ⟨.inl a, _, hsC, hr,
        ((rel_iff_key (Reach.step hp hsC)).1 hr').symm⟩
  obs {p} hp := (rel_preserves_obs h5 h6.toLemma6Fwd hX p _ ((rel_iff_key hp).2 rfl)).symm

end Inner

/-! ## Theorem A8 with a nested `X` -/

section Outer

variable {T S I O Ta Sa XSa : Type}
  {C : LTS (T × S) (Ta ⊕ Sa)} {X : LTS (S × I) (XSa ⊕ Ra)} {obs : S → O} {inp : T → I}

/-- The visibility of `X`'s own labels: a `.inl` edge is silent, a `.inr` edge the rise. -/
def riseView {A : Type} : A ⊕ Ra → Option Ra
  | .inl _ => none
  | .inr r => some r

/-- A reachable representative in every block of a reachable state exists. -/
theorem exists_rep {B : S × I → Blk} (d : S × I) :
    ∃ rep : Blk → S × I, ∀ {x}, X.Reach x → X.Reach (rep (B x)) ∧ B (rep (B x)) = B x := by
  classical
  refine ⟨fun c => if h : ∃ x, X.Reach x ∧ B x = c then Classical.choose h else d, ?_⟩
  intro x hx
  have h : ∃ y, X.Reach y ∧ B y = B x := ⟨x, hx, rfl⟩
  simp only [h, ↓reduceDIte]
  exact Classical.choose_spec h

/-- `StableV` with `riseView` and the observation `(obs, inputs)`, plus a reachable
representative per block, is `Stable`. -/
theorem stable_of_stableV {B : S × I → Blk} {rep : Blk → S × I}
    (h : StableV X riseView (fun x => (obs x.1, x.2)) B)
    (hrep : ∀ {x}, X.Reach x → X.Reach (rep (B x)) ∧ B (rep (B x)) = B x) :
    Stable X obs B rep where
  b1 hx hy hb := by
    have := h.b1 hx hy hb
    simp only [Prod.mk.injEq] at this
    exact this
  b2 {x y c'} hx hy hb hexit := by
    obtain ⟨hne, z, z', hp, ⟨b, hb'⟩, hz'⟩ := hexit
    have hexitV : SilentExitV X riseView B x c' :=
      ⟨hne, z, z', PathWithin.map (N := invis X riseView)
        (fun _ _ _ hn => by obtain ⟨b, hb⟩ := hn; exact ⟨.inl b, hb, rfl⟩) hp,
        ⟨.inl b, hb', rfl⟩, hz'⟩
    obtain ⟨hne', w, w', hw, ⟨l, hl, hv⟩, hw'⟩ := h.b2 hx hy hb hexitV
    cases l with
    | inr r => cases hv
    | inl b =>
      refine ⟨hne', w, w', PathWithin.map (N := silent X) (fun _ _ _ hn => ?_) hw,
        ⟨b, hl⟩, hw'⟩
      obtain ⟨l, hl, hv⟩ := hn
      cases l with
      | inl b => exact ⟨b, hl⟩
      | inr r => cases hv
  b3 {x y r x'} hx hy hb hs := by
    obtain ⟨a', ρ', y', ha', hv, hb'⟩ := h.b3 hx hy hb hs rfl
    cases a' with
    | inl b => cases hv
    | inr r' => exact ⟨r', y', ha', hb'⟩
  rep_block hx := (hrep hx).2
  rep_reach hx := (hrep hx).1

/-- The converse of `stable_of_stableV`: a flat `Stable` partition is a `StableV` one, so it is
the base of a `Tower`. -/
theorem stableV_of_stable {B : S × I → Blk} {rep : Blk → S × I} (h : Stable X obs B rep) :
    StableV X riseView (fun x => (obs x.1, x.2)) B where
  b1 hx hy hb := by
    obtain ⟨h1, h2⟩ := h.b1 hx hy hb
    simp only [h1, h2]
  b2 {x y c'} hx hy hb hexit := by
    obtain ⟨hne, z, z', hp, ⟨l, hl, hv⟩, hz'⟩ := hexit
    cases l with
    | inr r => cases hv
    | inl b =>
      have hp' : (silent X).PathWithin (fun w => B w = B x) x z :=
        PathWithin.map (N := silent X) (fun _ _ _ hn => by
          obtain ⟨l, hl, hv⟩ := hn
          cases l with
          | inl b => exact ⟨b, hl⟩
          | inr r => cases hv) hp
      obtain ⟨hne', w, w', hw, ⟨b', hb'⟩, hw'⟩ := h.b2 hx hy hb ⟨hne, z, z', hp', ⟨b, hl⟩, hz'⟩
      exact ⟨hne', w, w', PathWithin.map (N := invis X riseView)
        (fun _ _ _ hn => by obtain ⟨b, hb⟩ := hn; exact ⟨.inl b, hb, rfl⟩) hw,
        ⟨.inl b', hb', rfl⟩, hw'⟩
  b3 {x y a ρ x'} hx hy hb hs hr := by
    cases a with
    | inl b => cases hr
    | inr r =>
      obtain ⟨r', y', hy', hb'⟩ := h.b3 hx hy hb hs
      exact ⟨.inr r', r', y', hy', rfl, hb'⟩

/-- **Theorem A8 with a nested `X`**: if `X`, with its silent edges and rises and the
observation `(obs, inputs)`, is explored under a tower of reductions whose partition is `B`
(the innermost partition read through every level), then for any representative that is a
reachable state of the block of each reachable state (`exists_rep` gives one), the relation
of A8 is a divergence-blind branching bisimulation between `C` and the reduced cone built on
`B`, and it preserves `T_v` and `obs`. -/
theorem a8_nested {B : S × I → Blk} {rep : Blk → S × I} (h5 : Split C obs)
    (h6 : Lemma6 C X inp) (hE : Extended X)
    (hT : Tower X riseView (fun x => (obs x.1, x.2)) B)
    (hrep : ∀ {x}, X.Reach x → X.Reach (rep (B x)) ∧ B (rep (B x)) = B x) :
    BranchBisim C (reduced C X inp B rep) tLabel tLabel (rel C inp B) ∧
      PreservesObs (rel C inp B) (obsC obs) (obsR obs rep) :=
  a8_branching h5 h6 hE (stable_of_stableV hT.stable hrep)

/-- The nested case for invariants. -/
theorem a8_nested_holds_iff {B : S × I → Blk} {rep : Blk → S × I} (h5 : Split C obs)
    (h6 : Lemma6 C X inp) (hE : Extended X)
    (hT : Tower X riseView (fun x => (obs x.1, x.2)) B)
    (hrep : ∀ {x}, X.Reach x → X.Reach (rep (B x)) ∧ B (rep (B x)) = B x)
    (P : T × O → Prop) :
    C.Holds (fun p => P (obsC obs p)) ↔
      (reduced C X inp B rep).Holds (fun q => P (obsR obs rep q)) := by
  obtain ⟨hR, hobs⟩ := a8_nested h5 h6 hE hT hrep
  exact holds_iff_of_dbsb hR.dbsb hobs init_forward init_backward P

/-- **One level of nesting, assembled**: `X` is, up to an isomorphism `e` that keeps its
silent edges and rises, an inner cone `C'` whose rises are among its `T_a` actions (`ρv`) and
whose `T_v ∪ obs` determine `X`'s `obs` and input bytes through `g`. If `C'` satisfies A8's
hypotheses with its own extended system `X'` and partition `B'`, and `Bo` is a partition of
the inner reduced cone stable on it, then `X` is explored under a tower of depth two (the
isomorphism, then the inner reduction) whose partition is `Bo` read through both. -/
theorem tower_of_inner {T' S' I' O' Blk' Ta' Sa' XSa' Ra' Bk : Type}
    {C' : LTS (T' × S') (Ta' ⊕ Sa')} {X' : LTS (S' × I') (XSa' ⊕ Ra')} {obs' : S' → O'}
    {inp' : T' → I'} {B' : S' × I' → Blk'} {rep' : Blk' → S' × I'}
    (h5 : Split C' obs') (h6 : Lemma6 C' X' inp') (hE : Extended X')
    (hX : Stable X' obs' B' rep') (ρv : Ta' → Option Ra)
    (e : S × I → T' × S') (e' : T' × S' → S × I) (fa : XSa ⊕ Ra → Ta' ⊕ Sa')
    (ga : Ta' ⊕ Sa' → XSa ⊕ Ra) (he : ∀ x, e' (e x) = x) (he' : ∀ q, e (e' q) = q)
    (hfa : ∀ a, (tLabel (fa a)).bind ρv = riseView a)
    (hga : ∀ b, riseView (ga b) = (tLabel b).bind ρv)
    (hinit : ∀ {x}, X.init x → C'.init (e x)) (hinit' : ∀ {q}, C'.init q → X.init (e' q))
    (hstep : ∀ {x a x'}, X.step x a x' → C'.step (e x) (fa a) (e x'))
    (hstep' : ∀ {q b q'}, C'.step q b q' → X.step (e' q) (ga b) (e' q'))
    (g : T' × O' → O × I) (hg : ∀ x, g (obsC obs' (e x)) = (obs x.1, x.2))
    (Bo : T' × Blk' → Bk)
    (hBo : StableV (reduced C' X' inp' B' rep') (fun l => (tLabel l).bind ρv)
      (fun q => g (obsR obs' rep' q)) Bo) :
    Tower X riseView (fun x => (obs x.1, x.2)) (fun x => Bo (innerKey inp' B' (e x))) := by
  have hiso : InnerMap X riseView C' (fun l => (tLabel l).bind ρv) e
      (fun x => (obs x.1, x.2)) (fun q => g (obsC obs' q)) :=
    InnerMap.ofIso e e' fa ga he he' hfa hga hinit hinit' hstep hstep' hg
  exact .inner e hiso (.inner (innerKey inp' B') ((a8_innerMap h5 h6 hE hX ρv).comap g)
    (.flat Bo hBo))

end Outer

end RdraIsh
