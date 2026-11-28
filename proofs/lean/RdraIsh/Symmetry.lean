import RdraIsh.Stutter

/-!
# Instance symmetry (cones.md, Theorem A18, A20, A21)

A group `G` acts on the states and the labels of `M`; `G_s` (`mem s`) is the subgroup that a
state allows (cones.md, Definition 6: the permutations that fix the absent slots). The
fields `step`, `mem_step` and `init` are the conclusion of Theorem A18's first sentence
(steps commute with every `g ∈ G_s`, and `g ∈ G_t`) and of (G3); cones.md derives them from
(G1)–(G6), which are not modelled here. Two differences from cones.md:

- `step` is assumed on every state, where Theorem A18 states it for reachable states. The
  quotient's states are images `g(s)` of reachable states, which are reachable only once
  the theorem is proved, so the statement over all states is the simple one to use.
- The group laws are only what the proofs use: `act` composes and has an inverse, `G_s` is
  closed under both, and `G_{g(s)} = G_s` for `g ∈ G_s` (`mem_act`). No group structure is
  imported.

The canonical form `can s = act (π s) s` with `π s ∈ G_s` is "some member" of the orbit
(A21); the proofs never use that it is the least.
-/

namespace RdraIsh

open LTS

/-- A state-dependent symmetry of `M` (cones.md, Definition 6 and Theorem A18). -/
structure Sym {σ α : Type} (M : LTS σ α) (G : Type) where
  mul : G → G → G
  inv : G → G
  act : G → σ → σ
  actL : G → α → α
  /-- `mem s g`: `g ∈ G_s`. -/
  mem : σ → G → Prop
  act_mul : ∀ g h s, act (mul g h) s = act g (act h s)
  act_inv : ∀ g s, act (inv g) (act g s) = s
  act_inv' : ∀ g s, act g (act (inv g) s) = s
  mul_mem : ∀ {s g h}, mem s g → mem s h → mem s (mul g h)
  inv_mem : ∀ {s g}, mem s g → mem s (inv g)
  /-- `G_{g(s)} = G_s` for `g ∈ G_s`: `g` preserves the absent set. -/
  mem_act : ∀ {s g h}, mem s g → (mem (act g s) h ↔ mem s h)
  /-- Theorem A18: a step commutes with every `g ∈ G_s`. -/
  step : ∀ {s a t g}, M.step s a t → mem s g → M.step (act g s) (actL g a) (act g t)
  /-- Theorem A18: `g ∈ G_s` stays in `G_t` (a slot leaves `absent` only). -/
  mem_step : ∀ {s a t g}, M.step s a t → mem s g → mem t g
  /-- (G3): the initial set is closed under every `G_s`. -/
  init : ∀ {s g}, M.init s → mem s g → M.init (act g s)

variable {σ α G : Type} {M : LTS σ α}

/-- `s ≈ u`: `u = g(s)` for some `g ∈ G_s`. -/
def Sym.orbit (h : Sym M G) (s u : σ) : Prop := ∃ g, h.mem s g ∧ u = h.act g s

/-- **Theorem A18, the bisimulation**: `≈` is a bisimulation of `M` with itself. It maps
each step to one step, so it is in particular a divergence-blind stuttering bisimulation. -/
theorem a18_dbsb (h : Sym M G) : DBSB M M h.orbit := by
  constructor
  · rintro s u t ⟨g, hg, rfl⟩ ⟨a, hst⟩
    exact .inr ⟨h.act g s, h.act g t, .refl ⟨g, hg, rfl⟩, ⟨_, h.step hst hg⟩,
      ⟨g, h.mem_step hst hg, rfl⟩⟩
  · rintro u s u' ⟨g, hg, rfl⟩ ⟨a, hst⟩
    have hi : h.mem (h.act g s) (h.inv g) := (h.mem_act hg).2 (h.inv_mem hg)
    have hback := h.step hst hi
    rw [h.act_inv] at hback
    have hgt : h.mem (h.act (h.inv g) u') g := h.mem_step hback hg
    exact .inr ⟨s, h.act (h.inv g) u', .refl ⟨g, hg, rfl⟩, ⟨_, hback⟩,
      ⟨g, hgt, (h.act_inv' g u').symm⟩⟩

/-- **A20**: a subgroup of `G_s` that is closed under the group operations, preserved by its
own elements, and kept along steps is again a symmetry. The commutation fields carry over
from `G_s`, so leaving classes out loses no soundness. -/
def Sym.sub (h : Sym M G) (mem' : σ → G → Prop) (hsub : ∀ {s g}, mem' s g → h.mem s g)
    (mul_mem' : ∀ {s g k}, mem' s g → mem' s k → mem' s (h.mul g k))
    (inv_mem' : ∀ {s g}, mem' s g → mem' s (h.inv g))
    (mem_act' : ∀ {s g k}, mem' s g → (mem' (h.act g s) k ↔ mem' s k))
    (mem_step' : ∀ {s a t g}, M.step s a t → mem' s g → mem' t g) : Sym M G :=
  { h with
    mem := mem'
    mul_mem := mul_mem'
    inv_mem := inv_mem'
    mem_act := mem_act'
    step := fun hst hg => h.step hst (hsub hg)
    mem_step := mem_step'
    init := fun hi hg => h.init hi (hsub hg) }

/-- The quotient a run up to symmetry explores: it stores `can t` for every successor `t`
of a stored state, and the canonical forms of the initial states. -/
def quotient (M : LTS σ α) (can : σ → σ) : LTS σ α where
  init r := ∃ s, M.init s ∧ r = can s
  step r a r' := ∃ t, M.step r a t ∧ r' = can t

section Quotient

variable (h : Sym M G) (can : σ → σ) (π : σ → G)
  (hcan : ∀ s, can s = h.act (π s) s) (hπ : ∀ s, h.mem s (π s))

include hcan hπ

/-- Every reachable state of `M` has a reachable image in the quotient. -/
theorem quotient_covers : ∀ s, M.Reach s → ∃ g, h.mem s g ∧ (quotient M can).Reach (h.act g s) := by
  intro s hs
  induction hs with
  | @init s hi => exact ⟨π s, hπ s, Reach.init ⟨s, hi, (hcan s).symm⟩⟩
  | @step s a t _ hst ih =>
    obtain ⟨g, hg, hq⟩ := ih
    have hgt := h.mem_step hst hg
    have hm : h.mem (h.act g t) (π (h.act g t)) := hπ _
    have hm' : h.mem t (π (h.act g t)) := (h.mem_act hgt).1 hm
    refine ⟨h.mul (π (h.act g t)) g, h.mul_mem hm' hgt, ?_⟩
    rw [h.act_mul]
    exact Reach.step hq ⟨h.act g t, h.step hst hg, (hcan _).symm⟩

/-- Every reachable state of the quotient is an image of a reachable state of `M`. -/
theorem quotient_sound : ∀ r, (quotient M can).Reach r →
    ∃ s g, M.Reach s ∧ h.mem s g ∧ r = h.act g s := by
  intro r hr
  induction hr with
  | @init r hi =>
    obtain ⟨s, hs, rfl⟩ := hi
    exact ⟨s, π s, Reach.init hs, hπ s, hcan s⟩
  | @step r a r' _ hst ih =>
    obtain ⟨s, g, hs, hg, rfl⟩ := ih
    obtain ⟨t, hrt, rfl⟩ := hst
    have hi : h.mem (h.act g s) (h.inv g) := (h.mem_act hg).2 (h.inv_mem hg)
    have hback := h.step hrt hi
    rw [h.act_inv] at hback
    obtain ⟨t₀, ht₀⟩ : ∃ t₀, h.act (h.inv g) t = t₀ := ⟨_, rfl⟩
    rw [ht₀] at hback
    have hgt : h.mem t₀ g := h.mem_step hback hg
    have ht : h.act g t₀ = t := by rw [← ht₀]; exact h.act_inv' g t
    have hπt : h.mem t₀ (π t) := by
      have : h.mem (h.act g t₀) (π t) := by rw [ht]; exact hπ t
      exact (h.mem_act hgt).1 this
    refine ⟨t₀, h.mul (π t) g, Reach.step hs hback, h.mul_mem hπt hgt, ?_⟩
    rw [h.act_mul, ht, hcan]

/-- **A21 (and A18's verdict sentence) for invariants**: for an invariant whose value is the
same on `s` and `g(s)` for every `g ∈ G_s`, the run that stores one member of each orbit
gives the plain run's verdict. -/
theorem quotient_holds_iff (P : σ → Prop) (hP : ∀ s g, h.mem s g → (P (h.act g s) ↔ P s)) :
    M.Holds P ↔ (quotient M can).Holds P := by
  constructor
  · intro hM r hr
    obtain ⟨s, g, hs, hg, rfl⟩ := quotient_sound h can π hcan hπ r hr
    exact (hP s g hg).2 (hM s hs)
  · intro hQ s hs
    obtain ⟨g, hg, hq⟩ := quotient_covers h can π hcan hπ s hs
    exact (hP s g hg).1 (hQ _ hq)

end Quotient

end RdraIsh
