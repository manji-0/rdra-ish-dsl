import RdraIsh.LTS

/-!
# Divergence-blind stuttering bisimulation, and Theorem A

The relation of docs/theory/cones.md, Theorem A8 (Groote and Vaandrager 1990; De Nicola and
Vaandrager 1995): a step of one side is either a stutter, its target still related to the
other side's state, or is matched by a path of the other side through states related to the
source followed by one step into a state related to the target. There is no divergence
clause, which is what "divergence-blind" means: a silent cycle on one side may be matched by
standing still on the other.

The relation is over unlabelled steps (`LTS.Next`). The preservation of the `T_a` action
labels that Theorem A8 also states is not modelled here.
-/

namespace RdraIsh

universe u v w x y

open LTS

variable {σ : Type u} {α : Type v} {τ : Type w} {β : Type x} {O : Type y}

/-- One direction: every step of `C` from a state related to `x` is a stutter or is matched
by a path of `X` from `x` within the states related to the source, then one step. -/
def StutterSim (C : LTS σ α) (X : LTS τ β) (R : σ → τ → Prop) : Prop :=
  ∀ s x s', R s x → C.Next s s' →
    R s' x ∨ ∃ y y', X.PathWithin (R s) x y ∧ X.Next y y' ∧ R s' y'

/-- A divergence-blind stuttering bisimulation: a stuttering simulation both ways. -/
def DBSB (C : LTS σ α) (X : LTS τ β) (R : σ → τ → Prop) : Prop :=
  StutterSim C X R ∧ StutterSim X C (fun x s => R s x)

/-- The relation preserves the observations `oC` and `oX`. -/
def PreservesObs (R : σ → τ → Prop) (oC : σ → O) (oX : τ → O) : Prop :=
  ∀ s x, R s x → oC s = oX x

/-- Every initial state of `C` is related to an initial state of `X`. -/
def InitCovered (C : LTS σ α) (X : LTS τ β) (R : σ → τ → Prop) : Prop :=
  ∀ s, C.init s → ∃ x, X.init x ∧ R s x

/-- Under a stuttering simulation that covers the initial states, every reachable state of
`C` is related to a reachable state of `X`. -/
theorem reach_of_stutterSim {C : LTS σ α} {X : LTS τ β} {R : σ → τ → Prop}
    (hsim : StutterSim C X R) (hinit : InitCovered C X R) :
    ∀ s, C.Reach s → ∃ x, X.Reach x ∧ R s x := by
  intro s hs
  induction hs with
  | init h =>
    obtain ⟨x, hx, hr⟩ := hinit _ h
    exact ⟨x, Reach.init hx, hr⟩
  | @step s a s' _ hstep ih =>
    obtain ⟨x, hx, hr⟩ := ih
    rcases hsim s x s' hr ⟨a, hstep⟩ with h | ⟨y, y', hp, ⟨b, hb⟩, hr'⟩
    · exact ⟨x, hx, h⟩
    · exact ⟨y', Reach.step (hp.reach hx) hb, hr'⟩

/-- **Theorem A, one direction** (the inclusion every `ok` of an invariant reads, Theorem A8
point 1): a stuttering simulation from `C` to `X` that preserves the observation and covers
`C`'s initial states carries an invariant over the observation from `X` to `C`. -/
theorem holds_of_stutterSim {C : LTS σ α} {X : LTS τ β} {R : σ → τ → Prop}
    {oC : σ → O} {oX : τ → O}
    (hsim : StutterSim C X R) (hobs : PreservesObs R oC oX) (hinit : InitCovered C X R)
    (P : O → Prop) : X.Holds (fun x => P (oX x)) → C.Holds (fun s => P (oC s)) := by
  intro hX s hs
  obtain ⟨x, hx, hr⟩ := reach_of_stutterSim hsim hinit s hs
  rw [hobs s x hr]
  exact hX x hx

/-- **Theorem A** (the core of cones.md, Theorem A8, consequence 1): if `R` is a
divergence-blind stuttering bisimulation between `C` and `X` that preserves the observation
and relates the initial states both ways, an invariant over the observation holds on every
reachable state of `C` iff it holds on every reachable state of `X`. -/
theorem holds_iff_of_dbsb {C : LTS σ α} {X : LTS τ β} {R : σ → τ → Prop}
    {oC : σ → O} {oX : τ → O}
    (hR : DBSB C X R) (hobs : PreservesObs R oC oX)
    (hinitC : InitCovered C X R) (hinitX : InitCovered X C (fun x s => R s x))
    (P : O → Prop) : C.Holds (fun s => P (oC s)) ↔ X.Holds (fun x => P (oX x)) :=
  ⟨holds_of_stutterSim hR.2 (fun x s h => (hobs s x h).symm) hinitX P,
   holds_of_stutterSim hR.1 hobs hinitC P⟩

/-- The reachable observations of the two sides are equal (consequence 1 as a set
equality: "which valuations of `T_v ∪ obs` are reachable"). -/
theorem reachable_obs_eq_of_dbsb {C : LTS σ α} {X : LTS τ β} {R : σ → τ → Prop}
    {oC : σ → O} {oX : τ → O}
    (hR : DBSB C X R) (hobs : PreservesObs R oC oX)
    (hinitC : InitCovered C X R) (hinitX : InitCovered X C (fun x s => R s x)) (o : O) :
    (∃ s, C.Reach s ∧ oC s = o) ↔ (∃ x, X.Reach x ∧ oX x = o) := by
  constructor
  · rintro ⟨s, hs, rfl⟩
    obtain ⟨x, hx, hr⟩ := reach_of_stutterSim hR.1 hinitC s hs
    exact ⟨x, hx, (hobs s x hr).symm⟩
  · rintro ⟨x, hx, rfl⟩
    obtain ⟨s, hs, hr⟩ := reach_of_stutterSim hR.2 hinitX x hx
    exact ⟨s, hs, hobs s x hr⟩

end RdraIsh
