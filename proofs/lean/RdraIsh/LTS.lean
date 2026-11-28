/-!
# Labelled transition systems, reachability, invariants

The abstract setting of docs/theory/semantics.md, D13 (a transition system `(S, I, →)`),
with the step relation labelled by an action. A state set is a type `σ`; the initial set and
the step relation are predicates, so nothing here is finite or decidable.
-/

namespace RdraIsh

universe u v w x y

/-- A labelled transition system: initial states and a labelled step relation (D13). -/
structure LTS (σ : Type u) (α : Type v) where
  init : σ → Prop
  step : σ → α → σ → Prop

namespace LTS

variable {σ : Type u} {α : Type v}

/-- The unlabelled step relation: some action moves `s` to `t`. -/
def Next (M : LTS σ α) (s t : σ) : Prop := ∃ a, M.step s a t

/-- `Reach M s`: `s` is the last state of a finite trace of `M` (D14). -/
inductive Reach (M : LTS σ α) : σ → Prop
  | init {s} : M.init s → Reach M s
  | step {s a t} : Reach M s → M.step s a t → Reach M t

/-- The safety verdict of an invariant `P`: it holds on every reachable state. -/
def Holds (M : LTS σ α) (P : σ → Prop) : Prop := ∀ s, M.Reach s → P s

/-- A path of `Next` steps from `s` to `t` whose every state, `s` and `t` included,
satisfies `inv`. With `inv := fun _ => True` it is the reflexive-transitive closure of
`Next`. -/
inductive PathWithin (M : LTS σ α) (inv : σ → Prop) : σ → σ → Prop
  | refl {s} : inv s → PathWithin M inv s s
  | cons {s u t} : inv s → M.Next s u → PathWithin M inv u t → PathWithin M inv s t

theorem PathWithin.start {M : LTS σ α} {inv : σ → Prop} {s t : σ} :
    M.PathWithin inv s t → inv s
  | .refl h => h
  | .cons h _ _ => h

theorem PathWithin.last {M : LTS σ α} {inv : σ → Prop} {s t : σ} :
    M.PathWithin inv s t → inv t
  | .refl h => h
  | .cons _ _ p => p.last

/-- A path from a reachable state ends in a reachable state. -/
theorem PathWithin.reach {M : LTS σ α} {inv : σ → Prop} {s t : σ}
    (p : M.PathWithin inv s t) (hs : M.Reach s) : M.Reach t := by
  induction p with
  | refl _ => exact hs
  | cons _ hn _ ih =>
    obtain ⟨a, ha⟩ := hn
    exact ih (Reach.step hs ha)

end LTS

end RdraIsh
