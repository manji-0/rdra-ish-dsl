import RdraIsh.ReductionLabels

/-!
# `Reduction::stages` and `recanon`

`reduced` moves the block on a `T_a` step to the block of any rise chain from the
representative that ends at the new input bytes. The code instead walks the stages:
`Reduction::stages(before, after)` lists the input bytes after each stage (first the raw
inputs, then each bit slot that flips), and `Reduction::recanon` folds the rise table
`Reduction::rise(class, inputs)` over that list. A missing entry is a fault (`None`).

This file states that structure and proves it changes nothing:

- `RiseThrough X x is z`: one rise per listed stage, each ending at the listed input bytes.
  It implies `RiseStar` (`RiseThrough.riseStar`), so `Lemma6Staged` (Lemma 6 with the rise
  conjunct as the code states it, "a sequence of rises through the stages") implies `Lemma6`.
- `RiseTable`: the table holds, for every reachable representative, the block of each of its
  rises, keyed by the input bytes the rise ends at. That is how `Classes` fills `rises`.
- `recanon_of_through`: walking the table over the stages of a chain from a reachable `x`
  ends at the block of the chain's end.
- `reducedStaged` is `R` with the `T` move of the code; `staged_step_iff` shows its steps are
  those of `reduced` at every state related to a state of `C`, so `a8_staged` follows.

`stages` is a parameter here. `Restrict.lean` defines it as `Reduction::stages` does (the raw
inputs, then each bit that changes), and `Lemma6.lean::rise_fwd` proves that the rises of its
`X` go through that list.
-/

namespace RdraIsh

open LTS

universe u v w x y

/-- Changing a system's steps at states outside a relation's image keeps a branching
bisimulation. -/
theorem branchBisim_congr {σ : Type u} {α : Type v} {τ : Type w} {β : Type x} {L : Type y}
    {C : LTS σ α} {R₁ R₂ : LTS τ β} {vC : α → Option L} {vX : β → Option L}
    {R : σ → τ → Prop} (hstep : ∀ s x b x', R s x → (R₁.step x b x' ↔ R₂.step x b x'))
    (h : BranchBisim C R₁ vC vX R) : BranchBisim C R₂ vC vX R := by
  constructor
  · intro s x a s' hr hs
    rcases h.1 s x a s' hr hs with h' | ⟨y, b, y', hp, hb, hv, hr'⟩
    · exact .inl h'
    · refine .inr ⟨y, b, y', hp.map ?_, (hstep s y b y' hp.last).1 hb, hv, hr'⟩
      rintro u w hu ⟨c, hc, hvc⟩
      exact ⟨c, (hstep s u c w hu).1 hc, hvc⟩
  · intro x s b x' hr hs
    exact h.2 x s b x' hr ((hstep s x b x' hr).2 hs)

variable {T S I O Blk Ta Sa XSa Ra : Type}

/-- One rise per listed stage: the `k`-th rise ends at the `k`-th listed input bytes. -/
inductive RiseThrough (X : LTS (S × I) (XSa ⊕ Ra)) : S × I → List I → S × I → Prop
  | nil (x : S × I) : RiseThrough X x [] x
  | cons {x y z : S × I} {i : I} {is : List I} (r : Ra) :
      X.step x (.inr r) y → y.2 = i → RiseThrough X y is z → RiseThrough X x (i :: is) z

theorem RiseThrough.riseStar {X : LTS (S × I) (XSa ⊕ Ra)} {x z : S × I} {is : List I}
    (h : RiseThrough X x is z) : RiseStar X x z := by
  induction h with
  | nil x => exact .refl x
  | cons r hs _ _ ih => exact .cons r hs ih

/-- Lemma 6 with the rise conjunct as the code states it: a `T_a` step is matched by one rise
per stage of `stages (inp t) (inp t')`. -/
structure Lemma6Staged (C : LTS (T × S) (Ta ⊕ Sa)) (X : LTS (S × I) (XSa ⊕ Ra))
    (inp : T → I) (stages : I → I → List I) : Prop where
  init : ∀ {t s}, C.init (t, s) → X.init (s, inp t)
  s_step : ∀ {t s a s'}, C.Reach (t, s) → C.step (t, s) (.inr a) (t, s') →
    ∃ b, X.step (s, inp t) (.inl b) (s', inp t)
  rise_staged : ∀ {t s a t'}, C.Reach (t, s) → C.step (t, s) (.inl a) (t', s) →
    RiseThrough X (s, inp t) (stages (inp t) (inp t')) (s, inp t')
  s_step_back : ∀ {t s b s'}, C.Reach (t, s) → X.step (s, inp t) (.inl b) (s', inp t) →
    ∃ a, C.step (t, s) (.inr a) (t, s')

theorem Lemma6Staged.toLemma6 {C : LTS (T × S) (Ta ⊕ Sa)} {X : LTS (S × I) (XSa ⊕ Ra)}
    {inp : T → I} {stages : I → I → List I} (h : Lemma6Staged C X inp stages) :
    Lemma6 C X inp where
  init := h.init
  s_step := h.s_step
  rise hr hs := (h.rise_staged hr hs).riseStar
  s_step_back := h.s_step_back

/-- The rise table `Reduction::rises`: every rise of a reachable representative is entered
under its block and the input bytes it ends at, with the block it enters. -/
structure RiseTable (X : LTS (S × I) (XSa ⊕ Ra)) (B : S × I → Blk) (rep : Blk → S × I)
    (rt : Blk → I → Option Blk) : Prop where
  complete : ∀ {c r y}, X.Reach (rep c) → X.step (rep c) (.inr r) y → rt c y.2 = some (B y)

/-- `Reduction::recanon`: the table, folded over the stages. -/
def recanon (rt : Blk → I → Option Blk) : Blk → List I → Option Blk
  | c, [] => some c
  | c, i :: is => (rt c i).bind fun c' => recanon rt c' is

/-- `R` with the code's `T` move: the block moves by `recanon` over the stages. -/
def reducedStaged (C : LTS (T × S) (Ta ⊕ Sa)) (X : LTS (S × I) (XSa ⊕ Ra)) (inp : T → I)
    (B : S × I → Blk) (rep : Blk → S × I) (rt : Blk → I → Option Blk)
    (stages : I → I → List I) : LTS (T × Blk) (Ta ⊕ Unit) where
  init p := ∃ s, C.init (p.1, s) ∧ p.2 = B (s, inp p.1)
  step p l q := match l with
    | .inl a => C.step (p.1, (rep p.2).1) (.inl a) (q.1, (rep p.2).1) ∧
        recanon rt p.2 (stages (inp p.1) (inp q.1)) = some q.2
    | .inr () => q.1 = p.1 ∧ SilentExit X B (rep p.2) q.2

section Proofs

variable {C : LTS (T × S) (Ta ⊕ Sa)} {X : LTS (S × I) (XSa ⊕ Ra)} {obs : S → O}
  {inp : T → I} {B : S × I → Blk} {rep : Blk → S × I} {rt : Blk → I → Option Blk}
  {stages : I → I → List I}

/-- Walking the table over the stages of a chain from a reachable `x` ends at the block of
the chain's end. It needs (B1), (B3) and the table, and no particular choice of stages. -/
theorem recanon_of_through (hX : Stable X obs B rep) (ht : RiseTable X B rep rt)
    {x z : S × I} {is : List I} (h : RiseThrough X x is z) (hx : X.Reach x) :
    recanon rt (B x) is = some (B z) := by
  induction h with
  | nil => rfl
  | @cons x y z i is r hs hi _ ih =>
    have hy : X.Reach y := Reach.step hx hs
    have hrr := hX.rep_reach hx
    have hrb := hX.rep_block hx
    obtain ⟨r', y', hy', hb'⟩ := hX.b3 hx hrr hrb.symm hs
    have hc := ht.complete hrr hy'
    have hyr' : X.Reach y' := Reach.step hrr hy'
    obtain ⟨_, hi'⟩ := hX.b1 hyr' hy hb'
    rw [hi', hi, hb'] at hc
    simp only [recanon, hc, Option.bind_some]
    exact ih hy

/-- At every state related to a state of `C`, `reducedStaged` and `reduced` have the same
steps. -/
theorem staged_step_iff (h5 : Split C obs) (h6 : Lemma6Staged C X inp stages)
    (hE : Extended X) (hX : Stable X obs B rep) (ht : RiseTable X B rep rt)
    {p : T × S} {q q' : T × Blk} (l : Ta ⊕ Unit) (hr : rel C inp B p q) :
    (reducedStaged C X inp B rep rt stages).step q l q' ↔
      (reduced C X inp B rep).step q l q' := by
  have h6' := h6.toLemma6
  obtain ⟨t, s⟩ := p
  obtain ⟨t₀, c⟩ := q
  obtain ⟨t', c'⟩ := q'
  have hr₀ := hr
  obtain ⟨hr, ht₀, hc⟩ := hr
  simp only at ht₀ hc
  subst ht₀ hc
  have hx := x_reach h5 h6'.toLemma6Fwd (t₀, s) hr
  obtain ⟨hrr, hrb, hro, _⟩ := rep_facts hX hx
  cases l with
  | inr u => cases u; exact Iff.rfl
  | inl a =>
    constructor
    · rintro ⟨hstep, hre⟩
      simp only at hstep hre
      have hsC : C.step (t₀, s) (.inl a) (t', s) := h5.t_reads_obs hro hstep
      have hthr := h6.rise_staged hr hsC
      rw [recanon_of_through hX ht hthr hx] at hre
      cases hre
      have hrise := hthr.riseStar
      obtain ⟨w, hw, hwb⟩ := b3_star hX hx hrr hrb.symm hrise
      obtain ⟨_, hwi⟩ := hX.b1 (hw.reach hrr) (hrise.reach hx) hwb
      exact ⟨hstep, w, hw, hwi, hwb⟩
    · intro hs
      obtain ⟨hsC, _, _, hc'⟩ := t_step_back h5 h6' hE hX hr₀ hs
      simp only at hsC hc'
      refine ⟨hs.1, ?_⟩
      simp only
      rw [hc', recanon_of_through hX ht (h6.rise_staged hr hsC) hx]

/-- **Theorem A8 with the code's stages**: the relation of A8 is a divergence-blind branching
bisimulation between `C` and `reducedStaged`, the reduced cone whose `T` move folds the rise
table over `Reduction::stages`, and it preserves `T_v` and `obs`. -/
theorem a8_staged (h5 : Split C obs) (h6 : Lemma6Staged C X inp stages) (hE : Extended X)
    (hX : Stable X obs B rep) (ht : RiseTable X B rep rt) :
    BranchBisim C (reducedStaged C X inp B rep rt stages) tLabel tLabel (rel C inp B) ∧
      PreservesObs (rel C inp B) (obsC obs) (obsR obs rep) := by
  obtain ⟨hb, ho⟩ := a8_branching h5 h6.toLemma6 hE hX
  refine ⟨branchBisim_congr (fun p q l q' hr => ?_) hb, ho⟩
  exact (staged_step_iff h5 h6 hE hX ht l hr).symm

theorem staged_init_forward :
    InitCovered C (reducedStaged C X inp B rep rt stages) (rel C inp B) := by
  rintro ⟨t, s⟩ h
  exact ⟨(t, B (s, inp t)), ⟨s, h, rfl⟩, ⟨Reach.init h, rfl, rfl⟩⟩

theorem staged_init_backward :
    InitCovered (reducedStaged C X inp B rep rt stages) C (fun q p => rel C inp B p q) := by
  rintro ⟨t, c⟩ ⟨s, h, hc⟩
  exact ⟨(t, s), h, ⟨Reach.init h, rfl, hc⟩⟩

/-- Consequence 1 on the staged `R`: an invariant over `T_v ∪ obs` holds on `C` iff on it. -/
theorem a8_staged_holds_iff (h5 : Split C obs) (h6 : Lemma6Staged C X inp stages)
    (hE : Extended X) (hX : Stable X obs B rep) (ht : RiseTable X B rep rt)
    (P : T × O → Prop) :
    C.Holds (fun p => P (obsC obs p)) ↔
      (reducedStaged C X inp B rep rt stages).Holds (fun q => P (obsR obs rep q)) := by
  obtain ⟨hR, hobs⟩ := a8_staged h5 h6 hE hX ht
  exact holds_iff_of_dbsb hR.dbsb hobs staged_init_forward staged_init_backward P

end Proofs

end RdraIsh
