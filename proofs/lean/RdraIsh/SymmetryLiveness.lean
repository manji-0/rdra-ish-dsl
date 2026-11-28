import RdraIsh.Symmetry
import RdraIsh.Behaviour

/-!
# Liveness up to instance symmetry (cones.md, Theorem A23)

A run up to symmetry stores one member of each orbit and decides `eventually` and `leads_to`
on an *annotated* quotient (cones.md, "Which checks read a run up to symmetry soundly";
`explore/orbit/liveness.rs`). An edge from a stored state `r` keeps its concrete successor
`t` and the permutation `p ∈ G_t` that took `t` to the stored target `p(t)`. A binding and a
fair instance move along an edge by its permutation: the binding `b` at `r` becomes `p(b)`
at `p(t)`, and a *thread* follows an instance the same way.

This file states that reading and proves it gives the plain run's verdict, under D20's
fairness (`Fair_P`) and any stop rule that the group preserves (`WF_vars(Next)` among them,
`LTS.wfNext_iff`):

- An *annotated run* (`ARun`) is an infinite sequence of stored states `r n`, concrete
  successors `t n` and permutations `p n`; `RunOf` adds that the stored states are the
  canonical forms `can` and the permutations those of `π`, as the code stores them.
- `AFair`, `AStops` and `AViolates` are `Fair_P`, `StopsOnly` and `Violates` read on the
  annotated run: a fair instance and the binding are followed through the permutations, an
  instance is taken by the step `r n → t n` (so an orbit self-edge, `t n ≠ r n` with
  `p(t n) = r n`, is a move), and a stutter is `t n = r n`.
- `a23_iff`: a liveness property holds for every binding under `Fair_P` and the stop rule
  iff no annotated run of the quotient that is thread-fair and stops only where the stop
  rule allows violates it for any binding. `a23` is the same with the quotient's side
  restricted to annotated lassos, which is what a search over a finite graph finds, given
  finitely many reachable states and a finite `G_s` per state.
- `lasso_closure` is the lift of an annotated lasso: the group is the same at every stored
  state of the cycle (`mem_const`), so the composite permutation of the cycle lies in that
  group and has a power `d > 0` that acts as the identity (`order_exists`), and `d` copies
  of the cycle close the concrete lasso.
- `threadFair_iff_pieces` reads thread fairness on an annotated lasso as the finite rule
  the code applies to one cycle: every thread piece either takes its instance on some edge
  or meets a state where the instance is not enabled.

The hypotheses are those of `Sym` (Theorem A18's commutation on every state) and `SymLive`
(the actions on bindings and fair instances, and that a label of an instance is moved to a
label of the moved instance), with `LiveEqui` (the trigger and the stop rule read
the same on `s` and `g(s)` for `g ∈ G_s`, and the goal does on the states after a trigger). `SymOn` weakens the commutation to an invariant
`W` that holds on every reachable state and is closed under steps and under `G_s`;
`SymOn.toSym` and `restrict_liveHolds_iff` carry the theorems over to it.
-/

namespace RdraIsh

open LTS

variable {σ α G Φ β : Type} {M : LTS σ α}

/-! ## The actions on fair instances and bindings -/

/-- The data a liveness check up to symmetry adds to `Sym`: the actions of `G` on fair
instances (`Φ`) and on bindings (`β`), and the instances' labels (`inst f a`, as in
`LTS.Takes`), which the action moves along. -/
structure SymLive (h : Sym M G) (Φ β : Type) where
  actF : G → Φ → Φ
  actB : G → β → β
  actF_mul : ∀ g k f, actF (h.mul g k) f = actF g (actF k f)
  actF_inv : ∀ g f, actF (h.inv g) (actF g f) = f
  actF_inv' : ∀ g f, actF g (actF (h.inv g) f) = f
  actB_mul : ∀ g k b, actB (h.mul g k) b = actB g (actB k b)
  actB_inv' : ∀ g b, actB g (actB (h.inv g) b) = b
  inst : Φ → α → Prop
  /-- A label of the instance `f` is moved to a label of `g(f)`: the binding `p` of an
  action goes to `g(p)` and its slots `κ` with it. -/
  inst_act : ∀ g f a, inst f a → inst (actF g f) (h.actL g a)

/-- The trigger and the stop rule read the same on `s` and `g(s)`, the binding moved by `g`,
for `g ∈ G_s`, and so does the goal on the states `after b`: a set that holds wherever the
trigger of `b` holds and is kept by every step. This is what (G6) gives the reads a verdict
depends on (cones.md, Theorem A18 (G6)): the trigger is gated by presence, and the goal counts
only on states after a trigger, where a row present then is present or deleted. -/
structure LiveEqui (h : Sym M G) (L : SymLive h Φ β) (trig goal : β → σ → Prop)
    (stop : σ → Prop) (after : β → σ → Prop) : Prop where
  trig_act : ∀ {s g} b, h.mem s g → (trig (L.actB g b) (h.act g s) ↔ trig b s)
  trig_after : ∀ b s, trig b s → after b s
  after_next : ∀ b s t, after b s → M.Next s t → after b t
  goal_act : ∀ {s g} b, h.mem s g → after b s → (goal (L.actB g b) (h.act g s) ↔ goal b s)
  stop_act : ∀ {s g}, h.mem s g → (stop (h.act g s) ↔ stop s)

/-! ## Elementary facts about `Sym` -/

namespace Sym

variable (h : Sym M G)

theorem act_inj {g : G} {x y : σ} (e : h.act g x = h.act g y) : x = y := by
  have := congrArg (h.act (h.inv g)) e
  rwa [h.act_inv, h.act_inv] at this

theorem mem_inv_act {s : σ} {g : G} (hg : h.mem s g) : h.mem (h.act g s) (h.inv g) :=
  (h.mem_act hg).2 (h.inv_mem hg)

theorem next_act {s t : σ} {g : G} (hn : M.Next s t) (hg : h.mem s g) :
    M.Next (h.act g s) (h.act g t) :=
  let ⟨_, ha⟩ := hn; ⟨_, h.step ha hg⟩

/-- A stutter or a step keeps `g ∈ G_s` (a slot leaves `absent` only). -/
theorem mem_step_or {s t : σ} {g : G} (hst : t = s ∨ M.Next s t) (hg : h.mem s g) :
    h.mem t g := by
  rcases hst with rfl | ⟨_, ha⟩
  · exact hg
  · exact h.mem_step ha hg

theorem moves_act {s : σ} {g : G} (hg : h.mem s g) : M.Moves (h.act g s) ↔ M.Moves s := by
  constructor
  · rintro ⟨u, hne, hn⟩
    have hb := h.next_act hn (h.mem_inv_act hg)
    rw [h.act_inv] at hb
    refine ⟨h.act (h.inv g) u, fun e => hne ?_, hb⟩
    rw [← e, h.act_inv']
  · rintro ⟨u, hne, hn⟩
    exact ⟨h.act g u, fun e => hne (h.act_inj e), h.next_act hn hg⟩

end Sym

/-! ## Fairness transfer: the image of a taken or enabled instance -/

section Transfer

variable {h : Sym M G} (L : SymLive h Φ β)

theorem takes_act_fwd {s s' : σ} {g : G} {f : Φ} (hg : h.mem s g)
    (ht : M.Takes L.inst f s s') :
    M.Takes L.inst (L.actF g f) (h.act g s) (h.act g s') :=
  let ⟨hne, a, hi, hs⟩ := ht
  ⟨fun e => hne (h.act_inj e), h.actL g a, L.inst_act g f a hi, h.step hs hg⟩

/-- **D20's "taken" respects the action**: for `g ∈ G_s`, the step `g(s) → g(s')` takes
`g(f)` iff `s → s'` takes `f`. -/
theorem takes_act {s s' : σ} {g : G} {f : Φ} (hg : h.mem s g) :
    M.Takes L.inst (L.actF g f) (h.act g s) (h.act g s') ↔ M.Takes L.inst f s s' := by
  refine ⟨fun ht => ?_, takes_act_fwd L hg⟩
  have := takes_act_fwd L (h.mem_inv_act hg) ht
  rwa [L.actF_inv, h.act_inv, h.act_inv] at this

/-- An instance is enabled at `s` iff its image is enabled at `g(s)`, for `g ∈ G_s`. -/
theorem enabled_act {s : σ} {g : G} {f : Φ} (hg : h.mem s g) :
    M.Enabled L.inst (L.actF g f) (h.act g s) ↔ M.Enabled L.inst f s := by
  constructor
  · rintro ⟨u, ht⟩
    rw [← h.act_inv' g u] at ht
    exact ⟨_, (takes_act L hg).1 ht⟩
  · rintro ⟨s', ht⟩
    exact ⟨_, (takes_act L hg).2 ht⟩

/-- `WF_vars(Next)`'s stop rule, `¬ Moves`, is one the group preserves. -/
theorem stop_moves_equi {s : σ} {g : G} (hg : h.mem s g) :
    ¬ M.Moves (h.act g s) ↔ ¬ M.Moves s := by
  rw [h.moves_act hg]

end Transfer

/-! ## The annotated quotient -/

/-- The sequence of images of `x` under the permutations `p 0, p 1, …` in turn: the binding
or the thread of an instance followed along an annotated run. -/
def orbitSeq {γ : Type} (act : G → γ → γ) (p : Nat → G) (x : γ) : Nat → γ
  | 0 => x
  | n + 1 => act (p n) (orbitSeq act p x n)

/-- An annotated run of the quotient: from a stored state `r n`, a stutter (`t n = r n`) or
a step to the concrete successor `t n`, stored as `r (n+1) = p n (t n)` with `p n ∈ G_{t n}`.
The first stored state is an image of an initial state. -/
structure ARun (h : Sym M G) (r t : Nat → σ) (p : Nat → G) : Prop where
  init : ∃ c g, M.init c ∧ h.mem c g ∧ r 0 = h.act g c
  step : ∀ n, t n = r n ∨ M.Next (r n) (t n)
  mem : ∀ n, h.mem (t n) (p n)
  next : ∀ n, r (n + 1) = h.act (p n) (t n)

/-- The annotated run the code explores: it stores canonical forms `can t = π(t)(t)` and
annotates each edge with `π(t)`. -/
def RunOf (h : Sym M G) (can : σ → σ) (π : σ → G) (r t : Nat → σ) (p : Nat → G) : Prop :=
  ARun h r t p ∧ (∃ c, M.init c ∧ r 0 = can c) ∧ ∀ n, r (n + 1) = can (t n) ∧ p n = π (t n)

/-- `Fair_P` on the annotated run: every thread whose instance is enabled at every stored
state from some point on is taken infinitely often, by the step `r n → t n`. -/
def AFair {h : Sym M G} (L : SymLive h Φ β) (r t : Nat → σ) (p : Nat → G) : Prop :=
  ∀ x, EvAlways (fun n => M.Enabled L.inst (orbitSeq L.actF p x n) (r n)) →
    InfOften (fun n => M.Takes L.inst (orbitSeq L.actF p x n) (r n) (t n))

/-- The annotated run stutters forever only from a stored state where `stop` holds. -/
def AStops (stop : σ → Prop) (r t : Nat → σ) : Prop :=
  ∀ N, (∀ m, N ≤ m → t m = r m) → stop (r N)

/-- The annotated run violates `□(trig ⇒ ◇goal)` for the binding `b` at its first state,
the binding moved along the run. -/
def AViolates {h : Sym M G} (L : SymLive h Φ β) (trig goal : β → σ → Prop) (r : Nat → σ)
    (p : Nat → G) (b : β) : Prop :=
  ∃ i, trig (orbitSeq L.actB p b i) (r i) ∧
    ∀ j, i ≤ j → ¬ goal (orbitSeq L.actB p b j) (r j)

/-! ## Frames: a concrete behaviour seen through accumulated permutations -/

/-- `c` is the concrete sequence of the annotated run `(r, t, p)` through the frames `Θ`:
`r n = Θ n (c n)`, `t n = Θ n (c (n+1))`, and the frames accumulate the permutations. -/
structure Framed (h : Sym M G) (r t c : Nat → σ) (p Θ : Nat → G) : Prop where
  mem : ∀ n, h.mem (c n) (Θ n)
  hr : ∀ n, r n = h.act (Θ n) (c n)
  ht : ∀ n, t n = h.act (Θ n) (c (n + 1))
  hΘ : ∀ n, Θ (n + 1) = h.mul (p n) (Θ n)

theorem orbitSeq_frame {h : Sym M G} {γ : Type} (act : G → γ → γ)
    (hmul : ∀ g k y, act (h.mul g k) y = act g (act k y)) {p Θ : Nat → G}
    (hΘ : ∀ n, Θ (n + 1) = h.mul (p n) (Θ n)) (y : γ) :
    ∀ n, orbitSeq act p (act (Θ 0) y) n = act (Θ n) y := by
  intro n
  induction n with
  | zero => rfl
  | succ n ih => simp only [orbitSeq, ih, hΘ, hmul]

section Framed

variable {h : Sym M G} (L : SymLive h Φ β) {r t c : Nat → σ} {p Θ : Nat → G}

theorem Framed.fair_iff (hf : Framed h r t c p Θ) : AFair L r t p ↔ M.FairP L.inst c := by
  have key : ∀ f n, orbitSeq L.actF p (L.actF (Θ 0) f) n = L.actF (Θ n) f :=
    fun f => orbitSeq_frame L.actF L.actF_mul hf.hΘ f
  have hen : ∀ f n, M.Enabled L.inst (orbitSeq L.actF p (L.actF (Θ 0) f) n) (r n) ↔
      M.Enabled L.inst f (c n) := by
    intro f n; rw [key, hf.hr]; exact enabled_act L (hf.mem n)
  have htk : ∀ f n, M.Takes L.inst (orbitSeq L.actF p (L.actF (Θ 0) f) n) (r n) (t n) ↔
      M.Takes L.inst f (c n) (c (n + 1)) := by
    intro f n; rw [key, hf.hr, hf.ht]; exact takes_act L (hf.mem n)
  constructor
  · intro ha f ⟨N, hN⟩ N'
    obtain ⟨m, hm, hT⟩ := ha (L.actF (Θ 0) f) ⟨N, fun m hm => (hen f m).2 (hN m hm)⟩ N'
    exact ⟨m, hm, (htk f m).1 hT⟩
  · intro hc x ⟨N, hN⟩ N'
    have hx : L.actF (Θ 0) (L.actF (h.inv (Θ 0)) x) = x := L.actF_inv' _ _
    rw [← hx] at hN ⊢
    obtain ⟨m, hm, hT⟩ := hc _ ⟨N, fun m hm => (hen _ m).1 (hN m hm)⟩ N'
    exact ⟨m, hm, (htk _ m).2 hT⟩

theorem Framed.stutter_iff (hf : Framed h r t c p Θ) (n : Nat) :
    t n = r n ↔ c (n + 1) = c n := by
  rw [hf.ht, hf.hr]
  exact ⟨h.act_inj, fun e => by rw [e]⟩

theorem Framed.stops_iff (hf : Framed h r t c p Θ) {stop : σ → Prop}
    (hstop : ∀ {s g}, h.mem s g → (stop (h.act g s) ↔ stop s)) :
    AStops stop r t ↔ StopsOnly stop c := by
  constructor
  · intro ha n q hc
    have hs := ha n fun m hm => (hf.stutter_iff m).2 (by rw [hc m hm, hc (m + 1) (by omega)])
    rw [hf.hr, hstop (hf.mem n), hc n (Nat.le_refl _)] at hs
    exact hs
  · intro hc N hst
    have hcon := const_of_stutter fun m hm => (hf.stutter_iff m).1 (hst m hm)
    rw [hf.hr, hstop (hf.mem N)]
    exact hc N (c N) hcon

theorem Framed.violates_iff (hf : Framed h r t c p Θ) {trig goal : β → σ → Prop}
    {stop : σ → Prop} {after : β → σ → Prop} (he : LiveEqui h L trig goal stop after)
    (hstep : ∀ n, c (n + 1) = c n ∨ M.Next (c n) (c (n + 1))) (b : β) :
    AViolates L trig goal r p (L.actB (Θ 0) b) ↔ Violates (trig b) (goal b) c := by
  have key : ∀ n, orbitSeq L.actB p (L.actB (Θ 0) b) n = L.actB (Θ n) b :=
    orbitSeq_frame L.actB L.actB_mul hf.hΘ b
  have hafter : ∀ i, trig b (c i) → ∀ j, i ≤ j → after b (c j) := by
    intro i hi j hj
    obtain ⟨d, rfl⟩ : ∃ d, j = i + d := ⟨j - i, by omega⟩
    clear hj
    induction d with
    | zero => exact he.trig_after b _ hi
    | succ d ih =>
      show after b (c (i + d + 1))
      rcases hstep (i + d) with e | hn
      · rw [e]; exact ih
      · exact he.after_next b _ _ ih hn
  simp only [AViolates, Violates, key, hf.hr, he.trig_act b (hf.mem _)]
  constructor
  · rintro ⟨i, hi, hg⟩
    exact ⟨i, hi, fun j hj hgj =>
      hg j hj ((he.goal_act b (hf.mem j) (hafter i hi j hj)).2 hgj)⟩
  · rintro ⟨i, hi, hg⟩
    exact ⟨i, hi, fun j hj hgj =>
      hg j hj ((he.goal_act b (hf.mem j) (hafter i hi j hj)).1 hgj)⟩

end Framed

/-! ## Lifting an annotated run, and projecting a behaviour -/

/-- The concrete states and frames of an annotated run from `(c₀, Θ₀)`: `c (n+1) = Θ n⁻¹ (t n)`
and `Θ (n+1) = p n · Θ n`, as `explore/orbit.rs::lift_path` accumulates `θ`. -/
def liftSeq (h : Sym M G) (t : Nat → σ) (p : Nat → G) (c₀ : σ) (Θ₀ : G) : Nat → σ × G
  | 0 => (c₀, Θ₀)
  | n + 1 => (h.act (h.inv (liftSeq h t p c₀ Θ₀ n).2) (t n), h.mul (p n) (liftSeq h t p c₀ Θ₀ n).2)

/-- **The lift (N12).** An annotated run lifts, from any initial state it is an image
of, to a behaviour of `M` that it frames. -/
theorem ARun.lift {h : Sym M G} {r t : Nat → σ} {p : Nat → G} (ha : ARun h r t p)
    {c₀ : σ} {Θ₀ : G} (hc₀ : M.init c₀) (hΘ₀ : h.mem c₀ Θ₀) (hr₀ : r 0 = h.act Θ₀ c₀) :
    let c := fun n => (liftSeq h t p c₀ Θ₀ n).1
    let Θ := fun n => (liftSeq h t p c₀ Θ₀ n).2
    Framed h r t c p Θ ∧ M.Behaviour c := by
  intro c Θ
  have hc1 : ∀ n, c (n + 1) = h.act (h.inv (Θ n)) (t n) := fun _ => rfl
  have hΘ1 : ∀ n, Θ (n + 1) = h.mul (p n) (Θ n) := fun _ => rfl
  have ht : ∀ n, t n = h.act (Θ n) (c (n + 1)) := fun n => by rw [hc1, h.act_inv']
  -- The invariant: `Θ n ∈ G_{c n}` and `r n = Θ n (c n)`; with it, the concrete step.
  have inv : ∀ n, (h.mem (c n) (Θ n) ∧ r n = h.act (Θ n) (c n)) ∧
      (c (n + 1) = c n ∨ M.Next (c n) (c (n + 1))) := by
    intro n
    induction n with
    | zero =>
      have hI : h.mem (c 0) (Θ 0) ∧ r 0 = h.act (Θ 0) (c 0) := ⟨hΘ₀, hr₀⟩
      refine ⟨hI, ?_⟩
      have hi := h.mem_inv_act hI.1
      rcases ha.step 0 with e | hn
      · left; rw [hc1, e, hI.2, h.act_inv]
      · right
        have := h.next_act hn (by rw [hI.2]; exact hi)
        rw [hI.2, h.act_inv] at this; rw [hc1]; exact this
    | succ n ih =>
      obtain ⟨⟨hm, hr⟩, hs⟩ := ih
      have hmt : h.mem (c (n + 1)) (Θ n) := h.mem_step_or hs hm
      have hpt : h.mem (c (n + 1)) (p n) := by
        rw [hc1]
        have hi : h.mem (t n) (h.inv (Θ n)) := by
          rw [ht n]; exact (h.mem_act hmt).2 (h.inv_mem hmt)
        exact (h.mem_act hi).2 (ha.mem n)
      have hI : h.mem (c (n + 1)) (Θ (n + 1)) ∧ r (n + 1) = h.act (Θ (n + 1)) (c (n + 1)) :=
        ⟨by rw [hΘ1]; exact h.mul_mem hpt hmt,
         by rw [ha.next, hΘ1, h.act_mul, ← ht]⟩
      refine ⟨hI, ?_⟩
      have hi := h.mem_inv_act hI.1
      rcases ha.step (n + 1) with e | hn
      · left; rw [hc1, e, hI.2, h.act_inv]
      · right
        have := h.next_act hn (by rw [hI.2]; exact hi)
        rw [hI.2, h.act_inv] at this; rw [hc1]; exact this
  exact ⟨⟨fun n => (inv n).1.1, fun n => (inv n).1.2, ht, hΘ1⟩,
    ⟨hc₀, fun n => (inv n).2⟩⟩

/-- The frames of the canonical run over a concrete sequence: `Θ 0 = π (c 0)` and
`Θ (n+1) = π(Θ n (c (n+1))) · Θ n`. -/
def projFrame (h : Sym M G) (π : σ → G) (c : Nat → σ) : Nat → G
  | 0 => π (c 0)
  | n + 1 => h.mul (π (h.act (projFrame h π c n) (c (n + 1)))) (projFrame h π c n)

section Project

variable (h : Sym M G) (can : σ → σ) (π : σ → G)
  (hcan : ∀ s, can s = h.act (π s) s) (hπ : ∀ s, h.mem s (π s))

include hcan hπ

/-- **The projection (A18's last sentence, along a behaviour).** A behaviour of `M` is
framed by an annotated run of the canonical quotient. -/
theorem project {c : Nat → σ} (hb : M.Behaviour c) :
    let Θ := projFrame h π c
    let r := fun n => h.act (Θ n) (c n)
    let t := fun n => h.act (Θ n) (c (n + 1))
    RunOf h can π r t (fun n => π (t n)) ∧ Framed h r t c (fun n => π (t n)) Θ := by
  intro Θ r t
  have hΘ1 : ∀ n, Θ (n + 1) = h.mul (π (t n)) (Θ n) := fun _ => rfl
  have hstep : ∀ n, c (n + 1) = c n ∨ M.Next (c n) (c (n + 1)) := hb.2
  have hmem : ∀ n, h.mem (c n) (Θ n) := by
    intro n
    induction n with
    | zero => exact hπ _
    | succ n ih =>
      have hmt : h.mem (c (n + 1)) (Θ n) := h.mem_step_or (hstep n) ih
      have : h.mem (c (n + 1)) (π (t n)) := (h.mem_act hmt).1 (hπ _)
      rw [hΘ1]; exact h.mul_mem this hmt
  have hnext : ∀ n, r (n + 1) = h.act (π (t n)) (t n) := by
    intro n; show h.act (Θ (n + 1)) _ = _; rw [hΘ1, h.act_mul]
  refine ⟨⟨⟨⟨c 0, Θ 0, hb.1, hmem 0, rfl⟩, fun n => ?_, fun n => hπ _, hnext⟩,
    ⟨c 0, hb.1, (hcan _).symm⟩, fun n => ⟨by rw [hnext, hcan], rfl⟩⟩,
    ⟨hmem, fun _ => rfl, fun _ => rfl, hΘ1⟩⟩
  rcases hstep n with e | hn
  · left; show h.act _ _ = h.act _ _; rw [e]
  · right; exact h.next_act hn (hmem n)

end Project

theorem RunOf.arun {h : Sym M G} {can : σ → σ} {π : σ → G} {r t : Nat → σ} {p : Nat → G}
    (hr : RunOf h can π r t p) : ARun h r t p := hr.1

/-! ## Theorem A23 on behaviours -/

section A23

variable {h : Sym M G} (L : SymLive h Φ β) {trig goal : β → σ → Prop} {stop : σ → Prop}
  {after : β → σ → Prop} (he : LiveEqui h L trig goal stop after)

include he

/-- A counterexample on the annotated quotient lifts to one of `M`, with the binding read
back through the first frame. -/
theorem a23_lift {r t : Nat → σ} {p : Nat → G} {b' : β} (ha : ARun h r t p)
    (hf : AFair L r t p) (hs : AStops stop r t) (hv : AViolates L trig goal r p b') :
    ∃ c b, M.Behaviour c ∧ M.FairP L.inst c ∧ StopsOnly stop c ∧
      Violates (trig b) (goal b) c := by
  obtain ⟨c₀, Θ₀, hc₀, hΘ₀, hr₀⟩ := ha.init
  obtain ⟨hfr, hb⟩ := ha.lift hc₀ hΘ₀ hr₀
  refine ⟨_, L.actB (h.inv Θ₀) b', hb, (hfr.fair_iff L).1 hf, (hfr.stops_iff he.stop_act).1 hs,
    (hfr.violates_iff L he hb.2 _).1 ?_⟩
  show AViolates L trig goal r p (L.actB Θ₀ (L.actB (h.inv Θ₀) b'))
  rw [L.actB_inv']; exact hv

/-- A counterexample of `M` projects to one of the canonical quotient. -/
theorem a23_project (can : σ → σ) (π : σ → G) (hcan : ∀ s, can s = h.act (π s) s)
    (hπ : ∀ s, h.mem s (π s)) {c : Nat → σ} {b : β} (hb : M.Behaviour c)
    (hf : M.FairP L.inst c) (hs : StopsOnly stop c) (hv : Violates (trig b) (goal b) c) :
    ∃ r t p b', RunOf h can π r t p ∧ AFair L r t p ∧ AStops stop r t ∧
      AViolates L trig goal r p b' := by
  obtain ⟨hrun, hfr⟩ := project h can π hcan hπ hb
  exact ⟨_, _, _, _, hrun, (hfr.fair_iff L).2 hf, (hfr.stops_iff he.stop_act).2 hs,
    (hfr.violates_iff L he hb.2 b).2 hv⟩

/-- **Theorem A23, on behaviours.** Under `Fair_P` and a stop rule the group preserves, a
liveness property holds on `M` for every binding iff no annotated run of the canonical
quotient that is thread-fair and stops only where the rule allows violates it for any
binding. -/
theorem a23_iff (can : σ → σ) (π : σ → G) (hcan : ∀ s, can s = h.act (π s) s)
    (hπ : ∀ s, h.mem s (π s)) :
    (∀ b, M.LiveHolds (fun ρ => M.FairP L.inst ρ ∧ StopsOnly stop ρ) (trig b) (goal b)) ↔
    (∀ r t p b', RunOf h can π r t p → AFair L r t p → AStops stop r t →
      ¬ AViolates L trig goal r p b') := by
  constructor
  · intro hM r t p b' hr hf hs hv
    obtain ⟨c, b, hb, hfc, hsc, hvc⟩ := a23_lift L he hr.arun hf hs hv
    exact hM b c hb ⟨hfc, hsc⟩ hvc
  · intro hQ b c hb ⟨hf, hs⟩ hv
    obtain ⟨r, t, p, b', hr, hfr, hsr, hvr⟩ := a23_project L he can π hcan hπ hb hf hs hv
    exact hQ r t p b' hr hfr hsr hvr

end A23

/-! ## Periodic sequences -/

theorem periodic_mul {γ : Type} {f : Nat → γ} {k q : Nat} (hp : ∀ n, k ≤ n → f (n + q) = f n) :
    ∀ j n, k ≤ n → f (n + j * q) = f n := by
  intro j
  induction j with
  | zero => intro n _; simp
  | succ j ih =>
    intro n hn
    have : n + (j + 1) * q = (n + j * q) + q := by rw [Nat.succ_mul]; omega
    rw [this, hp _ (by omega), ih n hn]

/-- A sequence whose next value depends on the index only through a periodic `F` repeats
from the first index whose value it meets again one period later. -/
theorem periodic_of_recurrence {γ : Type} {x : Nat → γ} {F : Nat → γ → γ} {a D : Nat}
    (hx : ∀ n, x (n + 1) = F n (x n)) (hF : ∀ n, a ≤ n → F (n + D) = F n)
    (h0 : x (a + D) = x a) : ∀ n, a ≤ n → x (n + D) = x n := by
  intro n hn
  obtain ⟨i, rfl⟩ : ∃ i, n = a + i := ⟨n - a, by omega⟩
  clear hn
  induction i with
  | zero => exact h0
  | succ i ih =>
    have e : a + (i + 1) + D = (a + i + D) + 1 := by omega
    rw [e, hx, ih, hF _ (by omega), ← hx]; rfl

/-- An annotated lasso: from index `k` on, the run repeats with period `q > 0`. -/
def ALasso (r t : Nat → σ) (p : Nat → G) : Prop :=
  ∃ k q, 0 < q ∧ ∀ n, k ≤ n → r (n + q) = r n ∧ t (n + q) = t n ∧ p (n + q) = p n

/-- Finitely many elements in each `G_s`. -/
def FiniteGroups (h : Sym M G) : Prop := ∀ s, ∃ LG : List G, ∀ g, h.mem s g → g ∈ LG

/-- The pigeonhole on a sequence of group elements of one finite `G_s`. -/
theorem repeats_in {h : Sym M G} (hG : FiniteGroups h) (s : σ) (x : Nat → G)
    (hx : ∀ j, h.mem s (x j)) : ∃ j₁ j₂, j₁ < j₂ ∧ x j₂ = x j₁ := by
  obtain ⟨LG, hLG⟩ := hG s
  obtain ⟨n₀, m, _, hnm, he, _⟩ := recurrent_segment x LG (fun j => hLG _ (hx j)) 0
  exact ⟨n₀, m, hnm, he⟩

/-- The projection of a concrete lasso is an annotated lasso. -/
theorem project_lasso (h : Sym M G) (π : σ → G) (hπ : ∀ s, h.mem s (π s)) (hG : FiniteGroups h)
    {c : Nat → σ} (hb : M.Behaviour c) (hl : Lasso c) :
    let Θ := projFrame h π c
    let r := fun n => h.act (Θ n) (c n)
    let t := fun n => h.act (Θ n) (c (n + 1))
    ALasso r t (fun n => π (t n)) := by
  intro Θ r t
  obtain ⟨k, q, hq, hc⟩ := hl
  have hmem : ∀ n, h.mem (c n) (Θ n) := by
    intro n
    induction n with
    | zero => exact hπ _
    | succ n ih =>
      have hmt : h.mem (c (n + 1)) (Θ n) := h.mem_step_or (hb.2 n) ih
      have : h.mem (c (n + 1)) (π (h.act (Θ n) (c (n + 1)))) := (h.mem_act hmt).1 (hπ _)
      exact h.mul_mem this hmt
  have hcj := periodic_mul hc
  obtain ⟨j₁, j₂, hj, he⟩ := repeats_in hG (c k) (fun j => Θ (k + j * q)) (fun j => by
    have := hmem (k + j * q); rwa [hcj j k (Nat.le_refl _)] at this)
  let a := k + j₁ * q
  let D := (j₂ - j₁) * q
  have hD : 0 < D := Nat.mul_pos (by omega) hq
  have hcD : ∀ n, k ≤ n → c (n + D) = c n := fun n hn => hcj _ n hn
  let F : Nat → G → G := fun n g => h.mul (π (h.act g (c (n + 1)))) g
  have hΘD : ∀ n, a ≤ n → Θ (n + D) = Θ n := by
    refine periodic_of_recurrence (F := F) (fun n => rfl) (fun n hn => ?_) ?_
    · funext g; simp only [F]
      rw [show n + D + 1 = (n + 1) + D by omega, hcD (n + 1) (by omega)]
    · show Θ (k + j₁ * q + (j₂ - j₁) * q) = Θ (k + j₁ * q)
      rw [show k + j₁ * q + (j₂ - j₁) * q = k + j₂ * q by
        rw [Nat.add_assoc, ← Nat.add_mul]; congr 2; omega]
      exact he
  refine ⟨a, D, hD, fun n hn => ?_⟩
  have h1 : r (n + D) = r n := by
    show h.act _ _ = h.act _ _; rw [hΘD n hn, hcD n (by omega)]
  have h2 : t (n + D) = t n := by
    show h.act _ _ = h.act _ _
    rw [hΘD n hn, show n + D + 1 = (n + 1) + D by omega, hcD (n + 1) (by omega)]
  exact ⟨h1, h2, by simp only [h2]⟩

/-! ## Theorem A23 on lassos -/

/-- **Theorem A23.** On a system whose reachable states lie in a list and whose groups
`G_s` are finite, under `Fair_P` and a stop rule the group preserves: a liveness property
holds for every binding iff no annotated *lasso* of the canonical quotient that is
thread-fair and stops only where the rule allows violates it for any binding. The
quotient's side is a statement about finitely many stored states and edges, as the run's
search reads it. -/
theorem a23 {h : Sym M G} (L : SymLive h Φ β) {trig goal : β → σ → Prop} {stop : σ → Prop}
    {after : β → σ → Prop} (he : LiveEqui h L trig goal stop after) (can : σ → σ) (π : σ → G)
    (hcan : ∀ s, can s = h.act (π s) s) (hπ : ∀ s, h.mem s (π s))
    (LM : List σ) (hLM : ∀ s, M.Reach s → s ∈ LM) (hG : FiniteGroups h) :
    (∀ b, M.LiveHolds (fun ρ => M.FairP L.inst ρ ∧ StopsOnly stop ρ) (trig b) (goal b)) ↔
    (∀ r t p b', RunOf h can π r t p → ALasso r t p → AFair L r t p → AStops stop r t →
      ¬ AViolates L trig goal r p b') := by
  constructor
  · intro hM r t p b' hr _ hf hs hv
    exact (a23_iff L he can π hcan hπ).1 hM r t p b' hr hf hs hv
  · intro hQ b c hb ⟨hf, hs⟩ hv
    obtain ⟨c', hl, hb', hf', hs', hv'⟩ :=
      lasso_of_behaviour M LM hLM L.inst stop (trig b) (goal b) c hb hf hs hv
    obtain ⟨hrun, hfr⟩ := project h can π hcan hπ hb'
    exact hQ _ _ _ _ hrun (project_lasso h π hπ hG hb' hl) ((hfr.fair_iff L).2 hf')
      ((hfr.stops_iff he.stop_act).2 hs') ((hfr.violates_iff L he hb'.2 b).2 hv')

/-! ## Lasso closure -/

/-- `f` applied `n` times. -/
def iter {γ : Type} (f : γ → γ) : Nat → γ → γ
  | 0, x => x
  | n + 1, x => f (iter f n x)

theorem iter_add {γ : Type} (f : γ → γ) (m n : Nat) (x : γ) :
    iter f (m + n) x = iter f m (iter f n x) := by
  induction m with
  | zero => simp [iter]
  | succ m ih => rw [show m + 1 + n = (m + n) + 1 by omega]; simp only [iter, ih]

theorem iter_inj {γ : Type} {f : γ → γ} (hf : ∀ y z, f y = f z → y = z) :
    ∀ n y z, iter f n y = iter f n z → y = z := by
  intro n
  induction n with
  | zero => intro y z e; exact e
  | succ n ih => intro y z e; exact ih y z (hf _ _ e)

/-- `g, g², g³, …` as group elements: `gpow g j` acts as `g` applied `j + 1` times. -/
def gpow (h : Sym M G) (g : G) : Nat → G
  | 0 => g
  | j + 1 => h.mul g (gpow h g j)

/-- The composite `p (n+i) ⋯ p (n+1) · p n` of the permutations of `i + 1` edges from `n`. -/
def cmp (h : Sym M G) (p : Nat → G) (n : Nat) : Nat → G
  | 0 => p n
  | i + 1 => h.mul (p (n + i + 1)) (cmp h p n i)

section Closure

variable {h : Sym M G}

/-- A sequence that each edge moves by its permutation is moved by the composite. -/
theorem moved_by_cmp {γ : Type} (act : G → γ → γ)
    (hmul : ∀ g k y, act (h.mul g k) y = act g (act k y)) {p : Nat → G} {y : Nat → γ}
    (hy : ∀ m, y (m + 1) = act (p m) (y m)) (n : Nat) :
    ∀ i, y (n + i + 1) = act (cmp h p n i) (y n) := by
  intro i
  induction i with
  | zero => exact hy n
  | succ i ih =>
    rw [show n + (i + 1) + 1 = (n + i + 1) + 1 by omega, hy, ih]
    simp only [cmp, hmul]

theorem cmp_periodic {p : Nat → G} {k q : Nat} (hp : ∀ n, k ≤ n → p (n + q) = p n) :
    ∀ n, k ≤ n → cmp h p (n + q) = cmp h p n := by
  intro n hn
  funext i
  induction i with
  | zero => exact hp n hn
  | succ i ih =>
    simp only [cmp, ih]
    rw [show n + q + i + 1 = (n + i + 1) + q by omega, hp _ (by omega)]

theorem gpow_act {γ : Type} (act : G → γ → γ)
    (hmul : ∀ g k y, act (h.mul g k) y = act g (act k y)) (g : G) :
    ∀ j y, act (gpow h g j) y = iter (act g) (j + 1) y := by
  intro j
  induction j with
  | zero => intro y; rfl
  | succ j ih => intro y; simp only [gpow, hmul, ih]; rfl

/-- **Finite order.** An element of a finite `G_s` has a power `d > 0` that acts as the
identity in every action of `G` in which it acts injectively: on states, on bindings and on
fair instances at once. -/
theorem order_exists (hG : FiniteGroups h) {s : σ} {g : G} (hg : h.mem s g) :
    ∃ d, 0 < d ∧ ∀ {γ : Type} (act : G → γ → γ),
      (∀ g k y, act (h.mul g k) y = act g (act k y)) →
      (∀ y z, act g y = act g z → y = z) → ∀ y, iter (act g) d y = y := by
  have hpow : ∀ j, h.mem s (gpow h g j) := by
    intro j
    induction j with
    | zero => exact hg
    | succ j ih => exact h.mul_mem hg ih
  obtain ⟨j₁, j₂, hj, he⟩ := repeats_in hG s (gpow h g) hpow
  refine ⟨j₂ - j₁, by omega, fun act hmul hinj y => ?_⟩
  have e := congrArg (fun k => act k y) he
  simp only [gpow_act act hmul] at e
  rw [show j₂ + 1 = (j₁ + 1) + (j₂ - j₁) by omega, iter_add] at e
  exact iter_inj hinj _ _ _ e

variable {r t : Nat → σ} {p : Nat → G}

theorem ARun.mem_succ (ha : ARun h r t p) {n : Nat} {g : G} (hg : h.mem (r n) g) :
    h.mem (r (n + 1)) g := by
  rw [ha.next]; exact (h.mem_act (ha.mem n)).2 (h.mem_step_or (ha.step n) hg)

theorem ARun.mem_add (ha : ARun h r t p) {n : Nat} {g : G} (hg : h.mem (r n) g) :
    ∀ d, h.mem (r (n + d)) g := by
  intro d
  induction d with
  | zero => exact hg
  | succ d ih => exact ha.mem_succ ih

/-- **The group is constant on a cycle.** From the index `k` where the stored states repeat
with period `q`, every stored state has the group `G_{r k}`, and every permutation of the
run lies in it. A step can only enlarge `G_s` (a slot leaves `absent` only), the stored
target has the group of its concrete successor (`mem_act`), and the cycle comes back. -/
theorem ARun.mem_const (ha : ARun h r t p) {k q : Nat} (hq : 0 < q)
    (hr : ∀ n, k ≤ n → r (n + q) = r n) :
    (∀ n, k ≤ n → ∀ g, h.mem (r n) g ↔ h.mem (r k) g) ∧ ∀ n, k ≤ n → h.mem (r k) (p n) := by
  have hconst : ∀ n, k ≤ n → ∀ g, h.mem (r n) g ↔ h.mem (r k) g := by
    intro n hn g
    constructor
    · intro hg
      have hback := ha.mem_add hg (k + (n - k) * q - n)
      have hle : n - k ≤ (n - k) * q := Nat.le_mul_of_pos_right _ hq
      rwa [show n + (k + (n - k) * q - n) = k + (n - k) * q by omega,
        periodic_mul hr (n - k) k (Nat.le_refl _)] at hback
    · intro hg
      have := ha.mem_add hg (n - k)
      rwa [show k + (n - k) = n by omega] at this
  refine ⟨hconst, fun n hn => ?_⟩
  have : h.mem (r (n + 1)) (p n) := by rw [ha.next]; exact (h.mem_act (ha.mem n)).2 (ha.mem n)
  exact (hconst (n + 1) (by omega) _).1 this

/-- **Lasso closure (N12).** The lift of an annotated lasso with period `q` from `k` is a
concrete lasso: the composite of one period of permutations lies in `G_{r k}` and has a
power `d > 0` that acts as the identity, and `d` copies of the cycle bring the concrete state
back. -/
theorem lasso_closure (hG : FiniteGroups h) (ha : ARun h r t p) {c : Nat → σ} {Θ : Nat → G}
    (hf : Framed h r t c p Θ) {k q : Nat} (hq : 0 < q)
    (hl : ∀ n, k ≤ n → r (n + q) = r n ∧ t (n + q) = t n ∧ p (n + q) = p n) :
    ∃ d, 0 < d ∧ h.mem (r k) (cmp h p k (q - 1)) ∧ ∀ n, k ≤ n → c (n + d * q) = c n := by
  have hr : ∀ n, k ≤ n → r (n + q) = r n := fun n hn => (hl n hn).1
  have hp : ∀ n, k ≤ n → p (n + q) = p n := fun n hn => (hl n hn).2.2
  have hmemP := (ha.mem_const hq hr).2
  let Pc := cmp h p k (q - 1)
  have hPc : ∀ i, h.mem (r k) (cmp h p k i) := by
    intro i
    induction i with
    | zero => exact hmemP k (Nat.le_refl _)
    | succ i ih => exact h.mul_mem (hmemP _ (by omega)) ih
  obtain ⟨d, hd, hord⟩ := order_exists hG (hPc (q - 1))
  refine ⟨d, hd, hPc (q - 1), ?_⟩
  -- Frames as actions: `Θ (m+1)` acts as `p m` after `Θ m`.
  have hy : ∀ x m, h.act (Θ (m + 1)) x = h.act (p m) (h.act (Θ m) x) := fun x m => by
    rw [hf.hΘ, h.act_mul]
  have hcp := periodic_mul (f := fun n => cmp h p n (q - 1)) (fun n hn => by
    show cmp h p (n + q) (q - 1) = cmp h p n (q - 1); rw [cmp_periodic hp n hn])
  have hcopy : ∀ x j, h.act (Θ (k + j * q)) x = iter (h.act Pc) j (h.act (Θ k) x) := by
    intro x j
    induction j with
    | zero => simp [iter]
    | succ j ih =>
      have e := moved_by_cmp (h := h) h.act h.act_mul (y := fun m => h.act (Θ m) x)
        (fun m => hy x m) (k + j * q) (q - 1)
      rw [show k + (j + 1) * q = k + j * q + (q - 1) + 1 by rw [Nat.succ_mul]; omega, e, ih,
        hcp j k (Nat.le_refl _)]
      rfl
  let D := d * q
  have hpD : ∀ n, k ≤ n → p (n + D) = p n := fun n hn => periodic_mul hp d n hn
  have hrD : ∀ n, k ≤ n → r (n + D) = r n := fun n hn => periodic_mul hr d n hn
  have hΘD : ∀ n, k ≤ n → ∀ x, h.act (Θ (n + D)) x = h.act (Θ n) x := by
    intro n hn
    obtain ⟨i, rfl⟩ : ∃ i, n = k + i := ⟨n - k, by omega⟩
    clear hn
    induction i with
    | zero =>
      intro x
      rw [Nat.add_zero, hcopy x d, hord h.act h.act_mul (fun _ _ => h.act_inj)]
    | succ i ih =>
      intro x
      rw [show k + (i + 1) + D = (k + i + D) + 1 by omega, hy, ih, hpD _ (by omega),
        ← Nat.add_assoc, hy]
  intro n hn
  apply h.act_inj (g := Θ (n + D))
  rw [← hf.hr, hrD n hn, hf.hr, hΘD n hn]

/-! ## Thread pieces on a cycle -/

theorem periodic_reduce {γ : Type} {f : Nat → γ} {k D : Nat} (hD : 0 < D)
    (hp : ∀ n, k ≤ n → f (n + D) = f n) :
    ∀ m, k ≤ m → ∃ n, k ≤ n ∧ n < k + D ∧ f m = f n := by
  intro m hm
  refine ⟨k + (m - k) % D, by omega, by have := Nat.mod_lt (m - k) hD; omega, ?_⟩
  have e : k + (m - k) % D + ((m - k) / D) * D = m := by
    have := Nat.mod_add_div (m - k) D; rw [Nat.mul_comm] at this; omega
  have := periodic_mul hp ((m - k) / D) (k + (m - k) % D) (by omega)
  rw [e] at this
  exact this

/-- **Thread pieces.** On an annotated lasso with finite groups, a thread starting at the
first state is periodic, with one period `D` for every thread, and its fairness is the rule
the code applies to a cycle: within one period `[k, k + D)`, the thread's piece either takes
its instance on some edge or meets a stored state where the instance is not enabled. -/
theorem threadFair_iff_pieces (L : SymLive h Φ β) (hG : FiniteGroups h) (ha : ARun h r t p)
    (hl : ALasso r t p) :
    ∃ k D, 0 < D ∧ ∀ x,
      ((EvAlways fun n => M.Enabled L.inst (orbitSeq L.actF p x n) (r n)) →
        InfOften fun n => M.Takes L.inst (orbitSeq L.actF p x n) (r n) (t n)) ↔
      ∃ n, k ≤ n ∧ n < k + D ∧ (¬ M.Enabled L.inst (orbitSeq L.actF p x n) (r n) ∨
        M.Takes L.inst (orbitSeq L.actF p x n) (r n) (t n)) := by
  obtain ⟨k, q, hq, hl⟩ := hl
  have hr : ∀ n, k ≤ n → r (n + q) = r n := fun n hn => (hl n hn).1
  have hp : ∀ n, k ≤ n → p (n + q) = p n := fun n hn => (hl n hn).2.2
  have hmemP := (ha.mem_const hq hr).2
  let Pc := cmp h p k (q - 1)
  have hPc : ∀ i, h.mem (r k) (cmp h p k i) := by
    intro i
    induction i with
    | zero => exact hmemP k (Nat.le_refl _)
    | succ i ih => exact h.mul_mem (hmemP _ (by omega)) ih
  obtain ⟨d, hd, hord⟩ := order_exists hG (hPc (q - 1))
  have hFinj : ∀ g y z, L.actF g y = L.actF g z → y = z := fun g y z e => by
    have := congrArg (L.actF (h.inv g)) e; rwa [L.actF_inv, L.actF_inv] at this
  have hcp := periodic_mul (f := fun n => cmp h p n (q - 1)) (fun n hn => by
    show cmp h p (n + q) (q - 1) = cmp h p n (q - 1); rw [cmp_periodic hp n hn])
  let D := d * q
  have hD : 0 < D := Nat.mul_pos hd hq
  refine ⟨k, D, hD, fun x => ?_⟩
  let y := orbitSeq L.actF p x
  have hcopy : ∀ j, y (k + j * q) = iter (L.actF Pc) j (y k) := by
    intro j
    induction j with
    | zero => simp [iter]
    | succ j ih =>
      have e := moved_by_cmp (h := h) L.actF L.actF_mul (y := y) (fun m => rfl) (k + j * q)
        (q - 1)
      rw [show k + (j + 1) * q = k + j * q + (q - 1) + 1 by rw [Nat.succ_mul]; omega, e, ih,
        hcp j k (Nat.le_refl _)]
      rfl
  have hyD : ∀ n, k ≤ n → y (n + D) = y n := by
    refine periodic_of_recurrence (F := fun m => L.actF (p m)) (fun m => rfl)
      (fun m hm => by show L.actF (p (m + d * q)) = _; rw [periodic_mul hp d m hm]) ?_
    rw [hcopy d, hord L.actF L.actF_mul (hFinj Pc)]
  -- The state of the thread at `n`: its instance, the stored state and the successor.
  let z := fun n => (y n, r n, t n)
  have hzD : ∀ n, k ≤ n → z (n + D) = z n := fun n hn => by
    have e1 : r (n + d * q) = r n := periodic_mul hr d n hn
    have e2 : t (n + d * q) = t n := periodic_mul (fun n hn => (hl n hn).2.1) d n hn
    show (y (n + D), r (n + d * q), t (n + d * q)) = _
    rw [hyD n hn, e1, e2]
  have hred := periodic_reduce hD hzD
  have hfar : ∀ n, k ≤ n → ∀ N, N ≤ n + N * D ∧ z (n + N * D) = z n := fun n hn N =>
    ⟨by have := Nat.le_mul_of_pos_right N hD; omega, periodic_mul hzD N n hn⟩
  have en_of : ∀ m n, z m = z n →
      (M.Enabled L.inst (y m) (r m) ↔ M.Enabled L.inst (y n) (r n)) := by
    intro m n e; simp only [z, Prod.mk.injEq] at e; rw [e.1, e.2.1]
  have tk_of : ∀ m n, z m = z n →
      (M.Takes L.inst (y m) (r m) (t m) ↔ M.Takes L.inst (y n) (r n) (t n)) := by
    intro m n e; simp only [z, Prod.mk.injEq] at e; rw [e.1, e.2.1, e.2.2]
  constructor
  · intro hfair
    refine Classical.byContradiction fun hno => ?_
    have hall : ∀ n, k ≤ n → n < k + D →
        M.Enabled L.inst (y n) (r n) ∧ ¬ M.Takes L.inst (y n) (r n) (t n) := by
      intro n h1 h2
      refine ⟨Classical.byContradiction fun hc => hno ⟨n, h1, h2, .inl hc⟩,
        fun hc => hno ⟨n, h1, h2, .inr hc⟩⟩
    obtain ⟨m, hm, hT⟩ := hfair ⟨k, fun m hm => by
      obtain ⟨n, h1, h2, e⟩ := hred m hm
      exact (en_of m n e).2 (hall n h1 h2).1⟩ k
    obtain ⟨n, h1, h2, e⟩ := hred m hm
    exact (hall n h1 h2).2 ((tk_of m n e).1 hT)
  · rintro ⟨n, h1, h2, hc⟩ ⟨N, hN⟩ N'
    rcases hc with hdis | htk
    · exfalso
      obtain ⟨hle, e⟩ := hfar n h1 N
      exact hdis ((en_of _ n e).1 (hN _ hle))
    · obtain ⟨hle, e⟩ := hfar n h1 N'
      exact ⟨_, hle, (tk_of _ n e).2 htk⟩

end Closure

/-! ## Commutation on an invariant instead of on every state

Theorem A18 states the commutation for reachable states; `Sym` assumes it on every state.
Reachable states alone are not enough for the quotient: a stored state is an image `g(s)`
of a reachable `s`, which need not be reachable when `g ∈ G_s` was not in the group of an
earlier state of the path to `s` (`ExampleReachOnly.reach_only_counterexample`). What the
proofs use is commutation on a set `W` that holds on the initial states and is closed under
steps and under every `G_s`; the least such set is the images of reachable states. `SymOn`
assumes that, and `SymOn.toSym` is a `Sym` of `M` restricted to `W`, whose reachable states,
behaviours, fairness and verdicts are `M`'s. -/

/-- The steps of `M` from the states of `W`. -/
def LTS.restrict (M : LTS σ α) (W : σ → Prop) : LTS σ α where
  init := M.init
  step s a t := W s ∧ M.step s a t

/-- A state-dependent symmetry whose commutation holds on the states of `W`. -/
structure SymOn (M : LTS σ α) (G : Type) (W : σ → Prop) where
  mul : G → G → G
  inv : G → G
  act : G → σ → σ
  actL : G → α → α
  mem : σ → G → Prop
  act_mul : ∀ g h s, act (mul g h) s = act g (act h s)
  act_inv : ∀ g s, act (inv g) (act g s) = s
  act_inv' : ∀ g s, act g (act (inv g) s) = s
  mul_mem : ∀ {s g h}, mem s g → mem s h → mem s (mul g h)
  inv_mem : ∀ {s g}, mem s g → mem s (inv g)
  mem_act : ∀ {s g h}, mem s g → (mem (act g s) h ↔ mem s h)
  step : ∀ {s a t g}, W s → M.step s a t → mem s g → M.step (act g s) (actL g a) (act g t)
  mem_step : ∀ {s a t g}, W s → M.step s a t → mem s g → mem t g
  init : ∀ {s g}, M.init s → mem s g → M.init (act g s)
  W_init : ∀ {s}, M.init s → W s
  W_step : ∀ {s a t}, W s → M.step s a t → W t
  W_act : ∀ {s g}, W s → mem s g → W (act g s)

/-- `SymOn` gives a `Sym` of the restriction to `W`. -/
def SymOn.toSym {W : σ → Prop} (h : SymOn M G W) : Sym (M.restrict W) G where
  mul := h.mul
  inv := h.inv
  act := h.act
  actL := h.actL
  mem := h.mem
  act_mul := h.act_mul
  act_inv := h.act_inv
  act_inv' := h.act_inv'
  mul_mem := h.mul_mem
  inv_mem := h.inv_mem
  mem_act := h.mem_act
  step := fun ⟨hW, hs⟩ hg => ⟨h.W_act hW hg, h.step hW hs hg⟩
  mem_step := fun ⟨hW, hs⟩ hg => h.mem_step hW hs hg
  init := h.init

section Restrict

variable {W : σ → Prop} (hWi : ∀ {s}, M.init s → W s)
  (hWs : ∀ {s a t}, W s → M.step s a t → W t)

include hWi hWs

theorem reach_W : ∀ s, M.Reach s → W s := by
  intro s hs
  induction hs with
  | init h => exact hWi h
  | step _ hst ih => exact hWs ih hst

theorem restrict_reach_iff (s : σ) : (M.restrict W).Reach s ↔ M.Reach s := by
  constructor
  · intro hs
    induction hs with
    | init h => exact Reach.init h
    | step _ hst ih => exact Reach.step ih hst.2
  · intro hs
    induction hs with
    | init h => exact Reach.init h
    | @step s a t hs hst ih => exact Reach.step ih ⟨reach_W hWi hWs s hs, hst⟩

theorem restrict_behaviour_iff (ρ : Nat → σ) : (M.restrict W).Behaviour ρ ↔ M.Behaviour ρ := by
  constructor
  · intro ⟨h0, hn⟩
    exact ⟨h0, fun n => (hn n).imp id fun ⟨a, _, hs⟩ => ⟨a, hs⟩⟩
  · intro hb
    refine ⟨hb.1, fun n => (hb.2 n).imp id fun ⟨a, hs⟩ => ⟨a, ?_, hs⟩⟩
    exact reach_W hWi hWs _ (hb.reach n)

omit hWi hWs in
theorem restrict_takes_iff {inst : Φ → α → Prop} {f : Φ} {s s' : σ} (hs : W s) :
    (M.restrict W).Takes inst f s s' ↔ M.Takes inst f s s' :=
  ⟨fun ⟨hne, a, hi, _, hst⟩ => ⟨hne, a, hi, hst⟩, fun ⟨hne, a, hi, hst⟩ => ⟨hne, a, hi, hs, hst⟩⟩

omit hWi hWs in
theorem restrict_enabled_iff {inst : Φ → α → Prop} {f : Φ} {s : σ} (hs : W s) :
    (M.restrict W).Enabled inst f s ↔ M.Enabled inst f s :=
  ⟨fun ⟨s', ht⟩ => ⟨s', (restrict_takes_iff hs).1 ht⟩,
   fun ⟨s', ht⟩ => ⟨s', (restrict_takes_iff hs).2 ht⟩⟩

omit hWi hWs in
theorem restrict_moves_iff {s : σ} (hs : W s) : (M.restrict W).Moves s ↔ M.Moves s :=
  ⟨fun ⟨s', hne, a, _, hst⟩ => ⟨s', hne, a, hst⟩, fun ⟨s', hne, a, hst⟩ => ⟨s', hne, a, hs, hst⟩⟩

/-- The liveness verdict of `M` is the one of its restriction to `W`, for stop rules that
agree on `W` (such as `¬ Moves` of each). -/
theorem restrict_liveHolds_iff (inst : Φ → α → Prop) (stop stop' : σ → Prop)
    (hstop : ∀ s, W s → (stop' s ↔ stop s)) (trig goal : σ → Prop) :
    (M.restrict W).LiveHolds (fun ρ => (M.restrict W).FairP inst ρ ∧ StopsOnly stop' ρ)
      trig goal ↔
    M.LiveHolds (fun ρ => M.FairP inst ρ ∧ StopsOnly stop ρ) trig goal := by
  have hW : ∀ ρ, M.Behaviour ρ → ∀ n, W (ρ n) := fun ρ hb n => reach_W hWi hWs _ (hb.reach n)
  have hfair : ∀ ρ, M.Behaviour ρ → ((M.restrict W).FairP inst ρ ↔ M.FairP inst ρ) := by
    intro ρ hb
    simp only [FairP, EvAlways, InfOften, restrict_enabled_iff (hW ρ hb _),
      restrict_takes_iff (hW ρ hb _)]
  have hst : ∀ ρ, M.Behaviour ρ → (StopsOnly stop' ρ ↔ StopsOnly stop ρ) := by
    intro ρ hb
    constructor
    · intro hs n q hc
      have hq : W q := by rw [← hc n (Nat.le_refl _)]; exact hW ρ hb n
      exact (hstop q hq).1 (hs n q hc)
    · intro hs n q hc
      have hq : W q := by rw [← hc n (Nat.le_refl _)]; exact hW ρ hb n
      exact (hstop q hq).2 (hs n q hc)
  constructor
  · intro hR ρ hb ⟨hf, hs⟩
    exact hR ρ ((restrict_behaviour_iff hWi hWs ρ).2 hb)
      ⟨(hfair ρ hb).2 hf, (hst ρ hb).2 hs⟩
  · intro hM ρ hb ⟨hf, hs⟩
    have hb' := (restrict_behaviour_iff hWi hWs ρ).1 hb
    exact hM ρ hb' ⟨(hfair ρ hb').1 hf, (hst ρ hb').1 hs⟩

end Restrict

/-- **Theorem A23 with commutation on an invariant.** `a23` for a symmetry whose commutation
holds only on a set `W` that contains the initial states and is closed under steps and
`G_s`: the plain verdict of `M` equals the verdict on annotated lassos of the canonical
quotient of `M` restricted to `W`. -/
theorem a23_on {W : σ → Prop} (h : SymOn M G W) (L : SymLive h.toSym Φ β)
    {trig goal : β → σ → Prop} {stop stop' : σ → Prop} {after : β → σ → Prop}
    (he : LiveEqui h.toSym L trig goal stop' after) (hstop : ∀ s, W s → (stop' s ↔ stop s))
    (can : σ → σ) (π : σ → G) (hcan : ∀ s, can s = h.act (π s) s) (hπ : ∀ s, h.mem s (π s))
    (LM : List σ) (hLM : ∀ s, M.Reach s → s ∈ LM) (hG : FiniteGroups h.toSym) :
    (∀ b, M.LiveHolds (fun ρ => M.FairP L.inst ρ ∧ StopsOnly stop ρ) (trig b) (goal b)) ↔
    (∀ r t p b', RunOf h.toSym can π r t p → ALasso r t p → AFair L r t p →
      AStops stop' r t → ¬ AViolates L trig goal r p b') := by
  have hLM' : ∀ s, (M.restrict W).Reach s → s ∈ LM :=
    fun s hs => hLM s ((restrict_reach_iff h.W_init h.W_step s).1 hs)
  rw [← a23 L he can π hcan hπ LM hLM' hG]
  exact forall_congr' fun b =>
    (restrict_liveHolds_iff h.W_init h.W_step L.inst stop stop' hstop (trig b) (goal b)).symm

end RdraIsh
