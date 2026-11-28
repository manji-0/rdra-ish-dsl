import RdraIsh.Stutter

/-!
# Labelled steps: divergence-blind branching bisimulation, and the finite-path verdicts

`Stutter.lean` relates unlabelled steps. Here each side carries a visibility map
`v : α → Option L`: `none` marks a silent step, `some g` a step that shows the label `g`.
A labelled simulation matches a visible step by a path of silent steps through states related
to the source, then one step that shows the same label; a silent step may also be a stutter.
A visible step is never a stutter. This is divergence-blind branching bisimulation (De Nicola
and Vaandrager 1995), the labelled form of `DBSB`.

The second half states the verdicts of consequences 1 to 3 of cones.md, Theorem A8, over an
abstract labelled system with an observation `o : σ → O`, and proves that such a
bisimulation keeps each:

- `Edges`: the reachable edges by their source observation, label and target observation
  (consequence 2: `NeverRuns`, arms taken, `after` assertions);
- `CanLeave`, `Stuck`: whether some path from a state takes an edge whose observations are
  related by `ch` (consequence 3, the deadlock finding of semantics.md, D25).
-/

namespace RdraIsh

universe u v w x y z

open LTS

variable {σ : Type u} {α : Type v} {τ : Type w} {β : Type x} {O : Type y} {L : Type z}

/-- The silent steps of `M`, as a system of their own. -/
def invis (M : LTS σ α) (vis : α → Option L) : LTS σ α where
  init := M.init
  step s a t := M.step s a t ∧ vis a = none

/-- A labelled stuttering simulation from `C` to `X`: a step of `C` is a silent stutter, or
a silent path of `X` through states related to the source followed by one step with the same
visibility into a state related to the target. -/
def BranchSim (C : LTS σ α) (X : LTS τ β) (vC : α → Option L) (vX : β → Option L)
    (R : σ → τ → Prop) : Prop :=
  ∀ s x a s', R s x → C.step s a s' →
    (vC a = none ∧ R s' x) ∨
      ∃ y b y', (invis X vX).PathWithin (R s) x y ∧ X.step y b y' ∧ vX b = vC a ∧ R s' y'

/-- A divergence-blind branching bisimulation: a labelled stuttering simulation both ways. -/
def BranchBisim (C : LTS σ α) (X : LTS τ β) (vC : α → Option L) (vX : β → Option L)
    (R : σ → τ → Prop) : Prop :=
  BranchSim C X vC vX R ∧ BranchSim X C vX vC (fun x s => R s x)

theorem LTS.PathWithin.of_invis {M : LTS σ α} {vis : α → Option L} {inv : σ → Prop} {s t : σ}
    (p : (invis M vis).PathWithin inv s t) : M.PathWithin inv s t := by
  induction p with
  | refl h => exact .refl h
  | cons h hn _ ih =>
    obtain ⟨a, ha, _⟩ := hn
    exact .cons h ⟨a, ha⟩ ih

/-- Forgetting the labels: a labelled simulation is a stuttering simulation. -/
theorem BranchSim.stutterSim {C : LTS σ α} {X : LTS τ β} {vC : α → Option L}
    {vX : β → Option L} {R : σ → τ → Prop} (h : BranchSim C X vC vX R) :
    StutterSim C X R := by
  intro s x s' hr ⟨a, ha⟩
  rcases h s x a s' hr ha with ⟨_, h'⟩ | ⟨y, b, y', hp, hb, _, hr'⟩
  · exact .inl h'
  · exact .inr ⟨y, y', hp.of_invis, ⟨b, hb⟩, hr'⟩

theorem BranchBisim.dbsb {C : LTS σ α} {X : LTS τ β} {vC : α → Option L}
    {vX : β → Option L} {R : σ → τ → Prop} (h : BranchBisim C X vC vX R) : DBSB C X R :=
  ⟨h.1.stutterSim, h.2.stutterSim⟩

/-! ## Consequence 2: the edges by label -/

/-- A reachable edge of `M` from a state observed as `p`, showing the label `g`, into a
state observed as `q`. `NeverRuns` of `g` is `¬ ∃ p q, Edges M v o g p q`; an arm taken is
an edge whose label is that arm; an `after g` assertion `Q` holds iff
`∀ p q, Edges M v o g p q → Q p q`. -/
def Edges (M : LTS σ α) (v : α → Option L) (o : σ → O) (g : L) (p q : O) : Prop :=
  ∃ s a s', M.Reach s ∧ M.step s a s' ∧ v a = some g ∧ o s = p ∧ o s' = q

theorem edges_of_branchSim {C : LTS σ α} {X : LTS τ β} {vC : α → Option L}
    {vX : β → Option L} {R : σ → τ → Prop} {oC : σ → O} {oX : τ → O}
    (hsim : BranchSim C X vC vX R) (hobs : PreservesObs R oC oX)
    (hinit : InitCovered C X R) {g : L} {p q : O} :
    Edges C vC oC g p q → Edges X vX oX g p q := by
  rintro ⟨s, a, s', hs, ha, hv, rfl, rfl⟩
  obtain ⟨x, hx, hr⟩ := reach_of_stutterSim hsim.stutterSim hinit s hs
  rcases hsim s x a s' hr ha with ⟨hn, _⟩ | ⟨y, b, y', hp, hb, hvb, hr'⟩
  · rw [hn] at hv
    cases hv
  · refine ⟨y, b, y', hp.of_invis.reach hx, hb, hvb.trans hv, ?_, (hobs s' y' hr').symm⟩
    exact (hobs s y hp.last).symm

/-- **Consequence 2, generic**: under a divergence-blind branching bisimulation that keeps
the observation and relates the initial states both ways, the two sides have the same
reachable edges by source observation, visible label and target observation. -/
theorem edges_iff_of_branchBisim {C : LTS σ α} {X : LTS τ β} {vC : α → Option L}
    {vX : β → Option L} {R : σ → τ → Prop} {oC : σ → O} {oX : τ → O}
    (hR : BranchBisim C X vC vX R) (hobs : PreservesObs R oC oX)
    (hinitC : InitCovered C X R) (hinitX : InitCovered X C (fun x s => R s x))
    (g : L) (p q : O) : Edges C vC oC g p q ↔ Edges X vX oX g p q :=
  ⟨edges_of_branchSim hR.1 hobs hinitC,
   edges_of_branchSim hR.2 (fun x s h => (hobs s x h).symm) hinitX⟩

/-! ## Consequence 3: leaving, and stuck states -/

/-- Some path from `s` takes an edge whose observations before and after are related by
`ch`. For D25, `ch p q` says that the edge changes the column at the slot or deletes the
row, so `ch` is irreflexive: a stutter never changes anything. -/
def CanLeave (M : LTS σ α) (o : σ → O) (ch : O → O → Prop) (s : σ) : Prop :=
  ∃ u u', M.PathWithin (fun _ => True) s u ∧ M.Next u u' ∧ ch (o u) (o u')

/-- A reachable state observed as `p` from which no path leaves (D25's *stuck*). -/
def Stuck (M : LTS σ α) (o : σ → O) (ch : O → O → Prop) (p : O) : Prop :=
  ∃ s, M.Reach s ∧ o s = p ∧ ¬ CanLeave M o ch s

/-- A path through any states is a path through all states. -/
theorem LTS.PathWithin.toTrue {M : LTS σ α} {inv : σ → Prop} {a b : σ}
    (p : M.PathWithin inv a b) : M.PathWithin (fun _ => True) a b := by
  induction p with
  | refl _ => exact .refl trivial
  | cons _ hn _ ih => exact .cons trivial hn ih

/-- Paths compose. -/
theorem LTS.PathWithin.trans' {M : LTS σ α} {inv : σ → Prop} {a b c : σ}
    (p : M.PathWithin (fun _ => True) a b) (q : M.PathWithin inv b c) :
    M.PathWithin (fun _ => True) a c := by
  induction p with
  | refl _ => exact q.toTrue
  | cons _ hn _ ih => exact .cons trivial hn (ih q)

/-- A path, then one step. -/
theorem LTS.PathWithin.snoc {M : LTS σ α} {inv : σ → Prop} {a b c : σ}
    (p : M.PathWithin inv a b) (h : M.Next b c) : M.PathWithin (fun _ => True) a c :=
  p.toTrue.trans' (inv := fun _ => True) (.cons trivial h (.refl trivial))

/-- A path of `C` from a state related to `x` is matched by a path of `X` from `x` into a
state related to its end. -/
theorem path_of_stutterSim {C : LTS σ α} {X : LTS τ β} {R : σ → τ → Prop}
    (hsim : StutterSim C X R) {s u : σ} (p : C.PathWithin (fun _ => True) s u) :
    ∀ x, R s x → ∃ y, X.PathWithin (fun _ => True) x y ∧ R u y := by
  induction p with
  | refl _ => exact fun x hr => ⟨x, .refl trivial, hr⟩
  | @cons s w u _ hn _ ih =>
    intro x hr
    rcases hsim s x w hr hn with h | ⟨y, y', hp, hy, hr'⟩
    · exact ih x h
    · obtain ⟨z, hz, hrz⟩ := ih y' hr'
      exact ⟨z, (hp.snoc hy).trans' hz, hrz⟩

theorem canLeave_of_stutterSim {C : LTS σ α} {X : LTS τ β} {R : σ → τ → Prop}
    {oC : σ → O} {oX : τ → O} {ch : O → O → Prop} (hirr : ∀ p, ¬ ch p p)
    (hsim : StutterSim C X R) (hobs : PreservesObs R oC oX) {s : σ} {x : τ}
    (hr : R s x) : CanLeave C oC ch s → CanLeave X oX ch x := by
  rintro ⟨u, u', hp, hn, hch⟩
  obtain ⟨y, hy, hry⟩ := path_of_stutterSim hsim hp x hr
  rcases hsim u y u' hry hn with h | ⟨z, z', hz, hzn, hrz'⟩
  · rw [hobs u y hry, hobs u' y h] at hch
    exact absurd hch (hirr _)
  · refine ⟨z, z', hy.trans' hz, hzn, ?_⟩
    rw [← hobs u z hz.last, ← hobs u' z' hrz']
    exact hch

/-- **Consequence 3, generic**: under a divergence-blind stuttering bisimulation that keeps
the observation and relates the initial states both ways, and for an irreflexive `ch`, a
reachable stuck state observed as `p` exists on one side iff on the other. A silent cycle
with no way out is stuck on both sides, since `CanLeave` reads paths, not single steps. -/
theorem stuck_iff_of_dbsb {C : LTS σ α} {X : LTS τ β} {R : σ → τ → Prop}
    {oC : σ → O} {oX : τ → O} {ch : O → O → Prop} (hirr : ∀ p, ¬ ch p p)
    (hR : DBSB C X R) (hobs : PreservesObs R oC oX)
    (hinitC : InitCovered C X R) (hinitX : InitCovered X C (fun x s => R s x)) (p : O) :
    Stuck C oC ch p ↔ Stuck X oX ch p := by
  have hobs' : PreservesObs (fun x s => R s x) oX oC := fun x s h => (hobs s x h).symm
  constructor
  · rintro ⟨s, hs, rfl, hn⟩
    obtain ⟨x, hx, hr⟩ := reach_of_stutterSim hR.1 hinitC s hs
    exact ⟨x, hx, (hobs s x hr).symm, fun h => hn (canLeave_of_stutterSim hirr hR.2 hobs' hr h)⟩
  · rintro ⟨x, hx, rfl, hn⟩
    obtain ⟨s, hs, hr⟩ := reach_of_stutterSim hR.2 hinitX x hx
    exact ⟨s, hs, hobs s x hr, fun h => hn (canLeave_of_stutterSim hirr hR.1 hobs hr h)⟩

/-- Consequence 3 as the verdict "no reachable state is stuck": the same on both sides. -/
theorem canLeave_all_iff_of_dbsb {C : LTS σ α} {X : LTS τ β} {R : σ → τ → Prop}
    {oC : σ → O} {oX : τ → O} {ch : O → O → Prop} (hirr : ∀ p, ¬ ch p p)
    (hR : DBSB C X R) (hobs : PreservesObs R oC oX)
    (hinitC : InitCovered C X R) (hinitX : InitCovered X C (fun x s => R s x)) :
    (∀ s, C.Reach s → CanLeave C oC ch s) ↔ (∀ x, X.Reach x → CanLeave X oX ch x) := by
  constructor
  · intro h x hx
    refine Classical.byContradiction fun hn => ?_
    obtain ⟨s, hs, hp, hns⟩ :=
      (stuck_iff_of_dbsb hirr hR hobs hinitC hinitX (oX x)).2 ⟨x, hx, rfl, hn⟩
    exact hns (h s hs)
  · intro h s hs
    refine Classical.byContradiction fun hn => ?_
    obtain ⟨x, hx, hp, hnx⟩ :=
      (stuck_iff_of_dbsb hirr hR hobs hinitC hinitX (oC s)).1 ⟨s, hs, rfl, hn⟩
    exact hnx (h x hx)

end RdraIsh
