import RdraIsh.Fairness

/-!
# Monotone variables (cones.md, Lemma 5 (d) and Theorem A15's (iii))

`reduce/split.rs::TransitionSystem::monotone_vars` marks a variable monotone when it is not a
counter, not a column or owner of an entity some action deletes, and its write graph, the
pairs of values the system's effects can move it between, has no cycle other than
self-loops (`acyclic`). This file proves what the reduction uses of that mark.

1. **Values along a sequence** (`Chain`, `Acyclic`). A sequence of values each of whose steps
   stays or follows an edge of an acyclic graph never comes back to a value it left
   (`chain_of_change`), and over finitely many values it changes finitely often
   (`finite_changes`).
2. **Along a behaviour** (`Covers`). If every step from a reachable state keeps a cell of the
   state or moves it along an edge, the cell changes finitely often along every behaviour
   (`finite_changes_of_behaviour`); if every cell of a state is so, with finitely many cells
   and values, every behaviour is constant from some point on (`settles`).
3. **Case 4 of Definition 5** (`no_silentCycle`, `a15_monotone`). When every cell of `S`
   (the non-input variables of `X`, slot by slot) is monotone along the silent edges, no
   block of any partition has a silent cycle between different states, so `hloop` holds with
   no class marked: that is `partition.rs::s_cycles` false, and `a15` holds with
   `canLoop := fun _ => False`.
4. **The write graph of the code** (`Sys`, `Edge`, `Monotone`, `succ_covers`). A model of the
   effects (`Effect` in `types.rs`), of the edges `monotone_vars` adds for each, and of
   `each_successor`, the successors of one bound action. A successor moves each cell of a
   variable that is not cyclic along an edge of that variable's graph or keeps it, given the
   facts of the pre-state named in `Pre`: a created row is absent and its cells at base, a
   deleted row is present, every value lies in its domain, a parameter's slot is one of its
   entity's, and the clock is below `int_max` when it ticks. So `monotone_vars` is sound for
   the steps of the system it reads, under `Pre`.

What is read from the code and not derived: that the depth-first search of `acyclic`
decides `Acyclic`; that `Sys` holds the effects of the system's actions and `succ` is
`each_successor`; that the guards give `Pre` on every reachable state (the roles of the
create and delete parameters, the clock's guard, and the base values of an absent row).
-/

namespace RdraIsh.Mono

open LTS

universe u v w x

variable {W : Type u}

/-! ## Values along a sequence -/

/-- A nonempty walk of edges of `E` between different values. -/
inductive Chain (E : W → W → Prop) : W → W → Prop
  | one {a b} : E a b → a ≠ b → Chain E a b
  | cons {a b c} : E a b → a ≠ b → Chain E b c → Chain E a c

theorem Chain.snoc {E : W → W → Prop} {a b c : W} (h : Chain E a b) (hbc : E b c)
    (hne : b ≠ c) : Chain E a c := by
  induction h with
  | one hab hab' => exact .cons hab hab' (.one hbc hne)
  | cons hab hab' _ ih => exact .cons hab hab' (ih hbc hne)

theorem Chain.trans {E : W → W → Prop} {a b c : W} (h₁ : Chain E a b) (h₂ : Chain E b c) :
    Chain E a c := by
  induction h₁ with
  | one hab hab' => exact .cons hab hab' h₂
  | cons hab hab' _ ih => exact .cons hab hab' (ih h₂)

/-- The write graph has no cycle other than self-loops (`acyclic` skips an edge `(a, a)`). -/
def Acyclic (E : W → W → Prop) : Prop := ∀ a, ¬ Chain E a a

/-- A graph whose edges between different values raise a rank is acyclic. -/
theorem acyclic_of_rank {E : W → W → Prop} (rank : W → Nat)
    (h : ∀ a b, E a b → a ≠ b → rank a < rank b) : Acyclic E := by
  have key : ∀ {a b}, Chain E a b → rank a < rank b := by
    intro a b hc
    induction hc with
    | one hab hne => exact h _ _ hab hne
    | cons hab hne _ ih => exact Nat.lt_trans (h _ _ hab hne) ih
  intro a hc
  exact Nat.lt_irrefl _ (key hc)

/-- A step of a cell: it keeps its value or moves along an edge. -/
def StepOrStay (E : W → W → Prop) (a b : W) : Prop := b = a ∨ E a b

section Seq

variable {E : W → W → Prop} {ω : Nat → W}

/-- Along a sequence of such steps the value at `i + d` is the value at `i` or lies at the
end of a chain from it. -/
theorem stay_or_chain (hw : ∀ k, StepOrStay E (ω k) (ω (k + 1))) (i d : Nat) :
    ω (i + d) = ω i ∨ Chain E (ω i) (ω (i + d)) := by
  induction d with
  | zero => exact .inl rfl
  | succ d ih =>
    rw [show i + (d + 1) = i + d + 1 by omega]
    by_cases hst : ω (i + d + 1) = ω (i + d)
    · rw [hst]; exact ih
    · rcases hw (i + d) with h | h
      · exact absurd h hst
      · rcases ih with h' | h'
        · rw [← h']; exact .inr (.one h (fun e => hst e.symm))
        · exact .inr (h'.snoc h (fun e => hst e.symm))

/-- A sequence that changes at some `k` in `[i, m)` goes from `ω i` to `ω m` by a chain. -/
theorem chain_of_change (hw : ∀ k, StepOrStay E (ω k) (ω (k + 1))) {i k m : Nat}
    (hik : i ≤ k) (hkm : k < m) (hne : ω (k + 1) ≠ ω k) : Chain E (ω i) (ω m) := by
  have e : E (ω k) (ω (k + 1)) := by
    rcases hw k with h | h
    · exact absurd h hne
    · exact h
  have hmid : Chain E (ω k) (ω (k + 1)) := .one e (fun h => hne h.symm)
  have h₁ := stay_or_chain hw i (k - i)
  have h₂ := stay_or_chain hw (k + 1) (m - (k + 1))
  rw [show i + (k - i) = k by omega] at h₁
  rw [show k + 1 + (m - (k + 1)) = m by omega] at h₂
  have hleft : Chain E (ω i) (ω (k + 1)) := by
    rcases h₁ with h | h
    · rw [← h]; exact hmid
    · exact h.trans hmid
  rcases h₂ with h | h
  · rw [h]; exact hleft
  · exact hleft.trans h

/-- **An acyclic cell changes finitely often.** A sequence of such steps over the values of a
list, for an acyclic graph, changes value at only finitely many indices. -/
theorem finite_changes (hA : Acyclic E) (hw : ∀ k, StepOrStay E (ω k) (ω (k + 1)))
    (L : List W) (hL : ∀ n, ω n ∈ L) : ¬ InfOften (fun n => ω (n + 1) ≠ ω n) := by
  intro hinf
  obtain ⟨n₀, m, _, hlt, hret, hseg⟩ := recurrent_segment ω L hL 0
  obtain ⟨k, hk, hch⟩ := hinf n₀
  obtain ⟨j, hj, hjm, hjk, hjk'⟩ := hseg k hk
  have hne : ω (j + 1) ≠ ω j := by rw [hjk, hjk']; exact hch
  have := chain_of_change hw hj hjm hne
  rw [hret] at this
  exact hA _ this

end Seq

/-! ## Along a behaviour -/

section Behaviour

variable {σ : Type v} {α : Type w}

/-- `val` reads one cell of a state, `E` is its variable's write graph: every step from a
reachable state keeps the cell or moves it along an edge. -/
def Covers (M : LTS σ α) (val : σ → W) (E : W → W → Prop) : Prop :=
  ∀ {s a t}, M.Reach s → M.step s a t → StepOrStay E (val s) (val t)

theorem Covers.seq {M : LTS σ α} {val : σ → W} {E : W → W → Prop} (hc : Covers M val E)
    {ρ : Nat → σ} (hb : M.Behaviour ρ) : ∀ k, StepOrStay E (val (ρ k)) (val (ρ (k + 1))) := by
  intro k
  rcases hb.2 k with h | ⟨a, ha⟩
  · exact .inl (by rw [h])
  · exact hc (hb.reach k) ha

/-- **A15's (iii), one cell.** A cell whose steps follow an acyclic write graph, over
finitely many values, changes finitely often along every behaviour. -/
theorem finite_changes_of_behaviour {M : LTS σ α} {val : σ → W} {E : W → W → Prop}
    (hc : Covers M val E) (hA : Acyclic E) (L : List W) (hL : ∀ s, M.Reach s → val s ∈ L)
    {ρ : Nat → σ} (hb : M.Behaviour ρ) :
    ¬ InfOften (fun n => val (ρ (n + 1)) ≠ val (ρ n)) :=
  finite_changes hA (hc.seq hb) L (fun n => hL _ (hb.reach n))

/-- **Every cell monotone, the state settles.** With finitely many cells (`LK`), each
following an acyclic graph, finitely many values, and the cells fixing the state, every
behaviour is constant from some index on. -/
theorem settles {K : Type x} {M : LTS σ α} (cell : K → σ → W) (E : K → W → W → Prop)
    (LK : List K) (hK : ∀ k, k ∈ LK) (hinj : ∀ s t, (∀ k, cell k s = cell k t) → s = t)
    (hc : ∀ k, Covers M (cell k) (E k)) (hA : ∀ k, Acyclic (E k)) (L : List W)
    (hL : ∀ k s, M.Reach s → cell k s ∈ L) {ρ : Nat → σ} (hb : M.Behaviour ρ) :
    ∃ N, ∀ m, N ≤ m → ρ m = ρ N := by
  have each : ∀ k, ∃ N, ∀ m, N ≤ m → cell k (ρ (m + 1)) = cell k (ρ m) := by
    intro k
    obtain ⟨N, hN⟩ := not_infOften (finite_changes_of_behaviour (hc k) (hA k) L (hL k) hb)
    exact ⟨N, fun m hm => Classical.byContradiction (hN m hm)⟩
  have all : ∀ l : List K, ∃ N, ∀ k ∈ l, ∀ m, N ≤ m → cell k (ρ (m + 1)) = cell k (ρ m) := by
    intro l
    induction l with
    | nil => exact ⟨0, fun _ h => absurd h List.not_mem_nil⟩
    | cons k l ih =>
      obtain ⟨N₁, h₁⟩ := each k
      obtain ⟨N₂, h₂⟩ := ih
      refine ⟨max N₁ N₂, fun k' hk' m hm => ?_⟩
      rcases List.mem_cons.mp hk' with he | hl
      · rw [he]; exact h₁ m (by omega)
      · exact h₂ k' hl m (by omega)
  obtain ⟨N, hN⟩ := all LK
  exact ⟨N, const_of_stutter fun m hm => hinj _ _ fun k => hN k (hK k) m hm⟩

end Behaviour

/-! ## Case 4 of Definition 5 -/

section Case4

variable {T S I O Blk Ta Sa XSa Ra Φ K : Type}

/-- A cell of `S` follows `E` along every silent edge of `X` from a reachable state. -/
def SilentCovers (X : LTS (S × I) (XSa ⊕ Ra)) (cell : S → W) (E : W → W → Prop) : Prop :=
  ∀ {x y}, X.Reach x → (silent X).Next x y → StepOrStay E (cell x.1) (cell y.1)

theorem walk_stay_or_chain {X : LTS (S × I) (XSa ⊕ Ra)} {B : S × I → Blk} {c : Blk}
    {cell : S → W} {E : W → W → Prop} (hc : SilentCovers X cell E) {y z : S × I}
    (hw : SilentWalk X B c y z) (hy : X.Reach y) :
    cell z.1 = cell y.1 ∨ Chain E (cell y.1) (cell z.1) := by
  induction hw with
  | refl => exact .inl rfl
  | @cons a b z _ _ _ hn _ ih =>
    obtain ⟨l, hl⟩ := hn
    have hb : X.Reach b := Reach.step hy hl
    have hab := hc hy ⟨l, hl⟩
    rcases ih hb with h | h
    · rw [h]
      by_cases he : cell b.1 = cell a.1
      · exact .inl he
      · rcases hab with h' | h'
        · exact absurd h' he
        · exact .inr (.one h' (fun e => he e.symm))
    · by_cases he : cell b.1 = cell a.1
      · rw [← he]; exact .inr h
      · rcases hab with h' | h'
        · exact absurd h' he
        · exact .inr (.cons h' (fun e => he e.symm) h)

/-- **No silent cycle when every cell of `S` is monotone.** If the cells of `S` determine it
and each follows an acyclic write graph along the silent edges, no block of any partition
holds a silent cycle between different states. Silent edges keep the input bytes
(`Extended`), so the inputs need no such condition: `s_cycles` reads only the variables of
`X` that are not inputs. -/
theorem no_silentCycle {X : LTS (S × I) (XSa ⊕ Ra)} (hE : Extended X) (cell : K → S → W)
    (E : K → W → W → Prop) (hinj : ∀ s s', (∀ k, cell k s = cell k s') → s = s')
    (hc : ∀ k, SilentCovers X (cell k) (E k)) (hA : ∀ k, Acyclic (E k))
    (B : S × I → Blk) (c : Blk) : ¬ SilentCycle X B c := by
  rintro ⟨x, y, hx, _, _, hne, hn, hw⟩
  obtain ⟨xs, xi⟩ := x
  obtain ⟨ys, yi⟩ := y
  obtain ⟨l, hl⟩ := hn
  have hi : yi = xi := hE.silent_keeps_inp hl
  subst hi
  have hs : ys ≠ xs := fun h => hne (by rw [h])
  have : ∃ k, cell k ys ≠ cell k xs := Classical.byContradiction fun h =>
    hs (hinj _ _ fun k => Classical.byContradiction fun hk => h ⟨k, hk⟩)
  obtain ⟨k, hk⟩ := this
  have hxy : E k (cell k xs) (cell k ys) := by
    rcases hc k hx ⟨l, hl⟩ with h | h
    · exact absurd h hk
    · exact h
  have hfirst : Chain (E k) (cell k xs) (cell k ys) := .one hxy (fun e => hk e.symm)
  have hy : X.Reach (ys, yi) := Reach.step hx hl
  rcases walk_stay_or_chain (hc k) hw hy with h | h
  · exact hA k _ (by have := hfirst; simp only at h; rwa [← h] at this)
  · exact hA k _ (hfirst.trans h)

variable {C : LTS (T × S) (Ta ⊕ Sa)} {X : LTS (S × I) (XSa ⊕ Ra)} {obs : S → O}
  {inp : T → I} {B : S × I → Blk} {rep : Blk → S × I}

/-- **Theorem A15, no class marked.** Under `a15`'s hypotheses, with the cells of `S`
monotone along the silent edges (`s_cycles` false), `hloop` holds with no class that can
loop: a run whose stops are only states no move leaves decides `C ⊨ P` under `WfNext`. -/
theorem a15_monotone (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (LX : List (S × I)) (hLX : ∀ x, X.Reach x → x ∈ LX)
    (LR : List (T × Blk)) (hLR : ∀ q, (reduced C X inp B rep).Reach q → q ∈ LR)
    (cell : K → S → W) (E : K → W → W → Prop)
    (hinj : ∀ s s', (∀ k, cell k s = cell k s') → s = s')
    (hc : ∀ k, SilentCovers X (cell k) (E k)) (hA : ∀ k, Acyclic (E k))
    (inst : Φ → Ta → Prop) (trig goal : T × O → Prop)
    (hrun : ReducedRunOk (reduced C X inp B rep) inst (fun _ => False)
      (fun q => trig (obsR obs rep q)) (fun q => goal (obsR obs rep q))) :
    C.LiveHolds (fun σ => C.FairP (onT inst) σ ∧ C.WFNext σ)
      (fun p => trig (obsC obs p)) (fun p => goal (obsC obs p)) :=
  a15 h5 h6 hE hX LX hLX LR hLR (fun _ => False)
    (fun c h => no_silentCycle hE cell E hinj hc hA B c h) inst trig goal hrun

end Case4

/-! ## The write graph of the code -/

section Effects

/-- `Rhs`: a literal value, or the value that names the slot a parameter is bound to. -/
inductive Rhs where
  | lit (c : Nat)
  | param (q : Nat)

/-- `Effect` (`types.rs`), with values as their codes: `0` absent, `1` present, `2` deleted
for an existence. -/
inductive Eff (V : Type) where
  | create (p : Nat)
  | delete (p : Nat)
  | assign (var : V) (p : Nat) (rhs : Rhs)
  | transition (var : V) (p : Nat) (arms : List (Nat × Nat))
  | choose (var : V) (p : Nat) (values : List Nat)
  | tick

/-- An action: the entity of each parameter, and its effects in order. -/
structure SAct (V Ent : Type) where
  ent : Nat → Ent
  effects : List (Eff V)

/-- The parts of a `TransitionSystem` that `monotone_vars` and `each_successor` read. -/
structure Sys (V Ent : Type) where
  acts : List (SAct V Ent)
  /-- An entity's existence variable. -/
  ex : Ent → V
  columns : Ent → List V
  owners : Ent → List V
  base : V → Nat
  /-- `domain.size`. -/
  size : V → Nat
  slots : Ent → Nat
  /-- `domain.of_slot`: the value that names a slot. -/
  ofSlot : V → Nat → Nat
  /-- `is_counted`. -/
  counted : V → Prop
  now : Option V
  intMax : Nat

variable {V Ent : Type}

namespace Sys

variable (sys : Sys V Ent)

/-- `froms` of `monotone_vars`: the base value in the action that creates the row, any value
of the domain elsewhere. -/
def From (a : SAct V Ent) (var : V) (p f : Nat) : Prop :=
  (Eff.create p ∈ a.effects → f = sys.base var) ∧ (Eff.create p ∉ a.effects → f < sys.size var)

/-- The edges `monotone_vars` adds to `v`'s graph for one effect of `a`. -/
def EdgeOf (a : SAct V Ent) : Eff V → V → Nat → Nat → Prop
  | .create p, v, f, t => v = sys.ex (a.ent p) ∧ f = 0 ∧ t = 1
  | .delete p, v, f, t => v = sys.ex (a.ent p) ∧ f = 1 ∧ t = 2
  | .assign var p (.lit c), v, f, t => v = var ∧ sys.From a var p f ∧ t = c
  | .assign var p (.param q), v, f, t =>
      v = var ∧ sys.From a var p f ∧ ∃ c, c < sys.slots (a.ent q) ∧ t = sys.ofSlot var c
  | .transition var _ arms, v, f, t => v = var ∧ (f, t) ∈ arms
  | .choose var p values, v, f, t => v = var ∧ sys.From a var p f ∧ t ∈ values
  | .tick, v, f, t => sys.now = some v ∧ f < sys.intMax ∧ t = f + 1

/-- `edges[v]` of `monotone_vars`, over every effect of every action. -/
def Edge (v : V) (f t : Nat) : Prop := ∃ a ∈ sys.acts, ∃ e ∈ a.effects, sys.EdgeOf a e v f t

/-- `cyclic[v]` of `monotone_vars`: a counter, or a column or owner of an entity that some
action deletes. -/
def Cyclic (v : V) : Prop :=
  sys.counted v ∨ ∃ a ∈ sys.acts, ∃ p, Eff.delete p ∈ a.effects ∧
    (v ∈ sys.columns (a.ent p) ∨ v ∈ sys.owners (a.ent p))

/-- `monotone_vars`' verdict: not cyclic, and the write graph has no cycle but self-loops. -/
def Monotone (v : V) : Prop := ¬ sys.Cyclic v ∧ Acyclic (sys.Edge v)

end Sys

/-- A state: the value of each slot of each variable. -/
abbrev St (V : Type) := V → Nat → Nat

set_option linter.unusedSectionVars false

variable [DecidableEq V]

/-- `StateSlice::set`. -/
def setv (σ : St V) (v : V) (k c : Nat) : St V :=
  fun v' k' => if v' = v ∧ k' = k then c else σ v' k'

/-- The arms of a transition whose `from` the pre-state holds. -/
def armsAt (pre : St V) (b : Nat → Nat) (var : V) (p : Nat) (arms : List (Nat × Nat)) :
    List (Nat × Nat) :=
  arms.filter (fun arm => arm.1 == pre var (b p))

/-- The deleted parameters, in order. -/
def deletes : List (Eff V) → List Nat
  | [] => []
  | .delete p :: es => p :: deletes es
  | _ :: es => deletes es

/-- The choices left open: each `choose`, and each transition with two or more arms that
apply. -/
def choices (b : Nat → Nat) (pre : St V) : List (Eff V) → List (V × Nat × List Nat)
  | [] => []
  | .choose var p values :: es => (var, p, values) :: choices b pre es
  | .transition var p arms :: es =>
      match armsAt pre b var p arms with
      | _ :: _ :: _ => (var, p, (armsAt pre b var p arms).map Prod.snd) :: choices b pre es
      | _ => choices b pre es
  | _ :: es => choices b pre es

namespace Sys

variable (sys : Sys V Ent)

/-- One pass of `each_successor` over an effect: a create, an assignment, a transition with
one arm that applies, and a tick write at once; a delete and a choice wait. Transitions and
ticks read the pre-state. -/
def det (a : SAct V Ent) (b : Nat → Nat) (pre : St V) (cur : St V) : Eff V → St V
  | .create p => setv cur (sys.ex (a.ent p)) (b p) 1
  | .assign var p (.lit c) => setv cur var (b p) c
  | .assign var p (.param q) => setv cur var (b p) (sys.ofSlot var (b q))
  | .transition var p arms =>
      match armsAt pre b var p arms with
      | [(_, t)] => setv cur var (b p) t
      | _ => cur
  | .tick =>
      match sys.now with
      | some v => setv cur v 0 (pre v 0 + 1)
      | none => cur
  | .delete _ => cur
  | .choose _ _ _ => cur

/-- The deletes, after every other effect: the columns back to base, the existence to
deleted. -/
def applyDeletes (a : SAct V Ent) (b : Nat → Nat) : List Nat → St V → St V
  | [], cur => cur
  | p :: ps, cur =>
      applyDeletes a b ps
        (setv ((sys.columns (a.ent p)).foldl (fun c v => setv c v (b p) (sys.base v)) cur)
          (sys.ex (a.ent p)) (b p) 2)

end Sys

/-- The choices, in order, each fixing one of its values; a choice of a deleted parameter is
dropped. -/
inductive Pick (b : Nat → Nat) (dl : List Nat) : List (V × Nat × List Nat) → St V → St V → Prop
  | nil {cur} : Pick b dl [] cur cur
  | skip {var p vals rest cur τ} : p ∈ dl → Pick b dl rest cur τ →
      Pick b dl ((var, p, vals) :: rest) cur τ
  | pick {var p vals rest cur τ c} : p ∉ dl → c ∈ vals → Pick b dl rest (setv cur var (b p) c) τ →
      Pick b dl ((var, p, vals) :: rest) cur τ

namespace Sys

variable (sys : Sys V Ent)

/-- `each_successor`: `τ` is a successor of `σ` under `a` bound to `b`. -/
def Succ (a : SAct V Ent) (b : Nat → Nat) (σ τ : St V) : Prop :=
  Pick b (deletes a.effects) (choices b σ a.effects)
    (sys.applyDeletes a b (deletes a.effects) (a.effects.foldl (sys.det a b σ) σ)) τ

/-- `a` assigns or chooses `var` at parameter `p`. -/
def WritesAt (a : SAct V Ent) (var : V) (p : Nat) : Prop :=
  (∃ rhs, Eff.assign var p rhs ∈ a.effects) ∨ ∃ vals, Eff.choose var p vals ∈ a.effects

/-- What the guards give of the pre-state of a step of `a` bound to `b`: a created row is
absent, and the cells an assignment or a choice writes on it are at base; a deleted row is
present; a cell an assignment or a choice writes holds a value of its domain; a parameter
named by an assignment is bound to one of its entity's slots; and the clock ticks only below
`int_max`. -/
structure Pre (a : SAct V Ent) (b : Nat → Nat) (σ : St V) : Prop where
  created_absent : ∀ p, Eff.create p ∈ a.effects → σ (sys.ex (a.ent p)) (b p) = 0
  deleted_present : ∀ p, Eff.delete p ∈ a.effects → σ (sys.ex (a.ent p)) (b p) = 1
  created_base : ∀ var p, Eff.create p ∈ a.effects → WritesAt a var p →
    σ var (b p) = sys.base var
  in_domain : ∀ var p, WritesAt a var p → σ var (b p) < sys.size var
  slot_lt : ∀ var p q, Eff.assign var p (.param q) ∈ a.effects → b q < sys.slots (a.ent q)
  tick_below : Eff.tick ∈ a.effects → ∀ v, sys.now = some v → σ v 0 < sys.intMax

/-- Every cell of a variable that is not cyclic keeps its pre-state value or moves along an
edge of its graph. -/
def Inv (pre cur : St V) : Prop :=
  ∀ v k, ¬ sys.Cyclic v → StepOrStay (sys.Edge v) (pre v k) (cur v k)

theorem inv_refl (pre : St V) : sys.Inv pre pre := fun _ _ _ => .inl rfl

theorem inv_set {pre cur : St V} (h : sys.Inv pre cur) {var : V} {k c : Nat}
    (he : ¬ sys.Cyclic var → sys.Edge var (pre var k) c) : sys.Inv pre (setv cur var k c) := by
  intro v k' hv
  show StepOrStay _ _ (if v = var ∧ k' = k then c else cur v k')
  split
  · rename_i hvk
    obtain ⟨rfl, rfl⟩ := hvk
    exact .inr (he hv)
  · exact h v k' hv

theorem from_pre {a : SAct V Ent} {b : Nat → Nat} {σ : St V} (hp : sys.Pre a b σ)
    {var : V} {p : Nat} (hw : WritesAt a var p) : sys.From a var p (σ var (b p)) :=
  ⟨fun hc => hp.created_base var p hc hw, fun _ => hp.in_domain var p hw⟩

theorem inv_det {a : SAct V Ent} (ha : a ∈ sys.acts) {b : Nat → Nat} {σ : St V}
    (hp : sys.Pre a b σ) :
    ∀ (es : List (Eff V)) (cur : St V), (∀ e ∈ es, e ∈ a.effects) → sys.Inv σ cur →
      sys.Inv σ (es.foldl (sys.det a b σ) cur) := by
  intro es
  induction es with
  | nil => intro cur _ h; exact h
  | cons e es ih =>
    intro cur hes h
    have he : e ∈ a.effects := hes e (List.mem_cons_self ..)
    refine ih _ (fun e' h' => hes e' (List.mem_cons_of_mem _ h')) ?_
    have edge : ∀ {v f t}, sys.EdgeOf a e v f t → sys.Edge v f t :=
      fun hx => ⟨a, ha, e, he, hx⟩
    cases e with
    | create p =>
      exact sys.inv_set h fun _ => edge ⟨rfl, hp.created_absent p he, rfl⟩
    | delete p => exact h
    | assign var p rhs =>
      cases rhs with
      | lit c => exact sys.inv_set h fun _ => edge ⟨rfl, sys.from_pre hp (.inl ⟨_, he⟩), rfl⟩
      | param q =>
        exact sys.inv_set h fun _ =>
          edge ⟨rfl, sys.from_pre hp (.inl ⟨_, he⟩), _, hp.slot_lt var p q he, rfl⟩
    | transition var p arms =>
      show sys.Inv σ (match armsAt σ b var p arms with
        | [(_, t)] => setv cur var (b p) t
        | _ => cur)
      split
      · rename_i f t heq
        have hm : (f, t) ∈ armsAt σ b var p arms := by
          rw [heq]; exact List.mem_singleton_self _
        unfold armsAt at hm
        obtain ⟨hma, hmf⟩ := List.mem_filter.mp hm
        have hf : f = σ var (b p) := by simpa using hmf
        exact sys.inv_set h fun _ => edge ⟨rfl, by rw [← hf]; exact hma⟩
      · exact h
    | choose var p values => exact h
    | tick =>
      show sys.Inv σ (match sys.now with
        | some v => setv cur v 0 (σ v 0 + 1)
        | none => cur)
      split
      · rename_i v hv
        exact sys.inv_set h fun _ => edge ⟨hv, hp.tick_below he v hv, rfl⟩
      · exact h

theorem mem_deletes {p : Nat} : ∀ {es : List (Eff V)}, p ∈ deletes es → Eff.delete p ∈ es
  | [], h => absurd h List.not_mem_nil
  | .delete q :: es, h => by
    rcases List.mem_cons.mp h with h | h
    · rw [h]; exact List.mem_cons_self ..
    · exact List.mem_cons_of_mem _ (mem_deletes h)
  | .create _ :: _, h => List.mem_cons_of_mem _ (mem_deletes h)
  | .assign _ _ _ :: _, h => List.mem_cons_of_mem _ (mem_deletes h)
  | .transition _ _ _ :: _, h => List.mem_cons_of_mem _ (mem_deletes h)
  | .choose _ _ _ :: _, h => List.mem_cons_of_mem _ (mem_deletes h)
  | .tick :: _, h => List.mem_cons_of_mem _ (mem_deletes h)

theorem inv_deletes {a : SAct V Ent} (ha : a ∈ sys.acts) {b : Nat → Nat} {σ : St V}
    (hp : sys.Pre a b σ) :
    ∀ (ps : List Nat) (cur : St V), (∀ p ∈ ps, Eff.delete p ∈ a.effects) → sys.Inv σ cur →
      sys.Inv σ (sys.applyDeletes a b ps cur) := by
  intro ps
  induction ps with
  | nil => intro cur _ h; exact h
  | cons p ps ih =>
    intro cur hps h
    have hd : Eff.delete p ∈ a.effects := hps p (List.mem_cons_self ..)
    refine ih _ (fun q hq => hps q (List.mem_cons_of_mem _ hq)) ?_
    have hcols : ∀ (l : List V) (c : St V), (∀ v ∈ l, v ∈ sys.columns (a.ent p)) →
        sys.Inv σ c → sys.Inv σ (l.foldl (fun c v => setv c v (b p) (sys.base v)) c) := by
      intro l
      induction l with
      | nil => intro c _ hc; exact hc
      | cons v l ihl =>
        intro c hl hc
        refine ihl _ (fun u hu => hl u (List.mem_cons_of_mem _ hu)) ?_
        refine sys.inv_set hc fun hcyc => absurd ?_ hcyc
        exact .inr ⟨a, ha, p, hd, .inl (hl v (List.mem_cons_self ..))⟩
    exact sys.inv_set (hcols _ cur (fun _ h => h) h) fun _ =>
      ⟨a, ha, _, hd, rfl, hp.deleted_present p hd, rfl⟩

theorem choice_edge {a : SAct V Ent} (ha : a ∈ sys.acts) {b : Nat → Nat} {σ : St V}
    (hp : sys.Pre a b σ) :
    ∀ {es : List (Eff V)}, (∀ e ∈ es, e ∈ a.effects) → ∀ {var p vals},
      (var, p, vals) ∈ choices b σ es → ∀ c ∈ vals, sys.Edge var (σ var (b p)) c := by
  intro es
  induction es with
  | nil => intro _ _ _ _ h; exact absurd h List.not_mem_nil
  | cons e es ih =>
    intro hes var p vals hm c hc
    have he : e ∈ a.effects := hes e (List.mem_cons_self ..)
    have hes' : ∀ e ∈ es, e ∈ a.effects := fun e' h' => hes e' (List.mem_cons_of_mem _ h')
    cases e with
    | choose var' p' values =>
      rcases List.mem_cons.mp hm with h | h
      · simp only [Prod.mk.injEq] at h
        obtain ⟨rfl, rfl, rfl⟩ := h
        exact ⟨a, ha, _, he, rfl, sys.from_pre hp (.inr ⟨_, he⟩), hc⟩
      · exact ih hes' h c hc
    | transition var' p' arms =>
      simp only [choices] at hm
      split at hm
      · rcases List.mem_cons.mp hm with h | h
        · simp only [Prod.mk.injEq] at h
          obtain ⟨rfl, rfl, rfl⟩ := h
          obtain ⟨arm, harm, rfl⟩ := List.mem_map.mp hc
          unfold armsAt at harm
          obtain ⟨hma, hmf⟩ := List.mem_filter.mp harm
          have hf : arm.1 = σ var (b p) := by simpa using hmf
          exact ⟨a, ha, _, he, rfl, by rw [← hf]; exact hma⟩
        · exact ih hes' h c hc
      · exact ih hes' hm c hc
    | create _ => exact ih hes' hm c hc
    | delete _ => exact ih hes' hm c hc
    | assign _ _ _ => exact ih hes' hm c hc
    | tick => exact ih hes' hm c hc

theorem inv_pick {b : Nat → Nat} {dl : List Nat} {σ : St V} {cs : List (V × Nat × List Nat)}
    (hcs : ∀ {var p vals}, (var, p, vals) ∈ cs → ∀ c ∈ vals, sys.Edge var (σ var (b p)) c)
    {cur τ : St V} (h : Pick b dl cs cur τ) (hi : sys.Inv σ cur) : sys.Inv σ τ := by
  induction h with
  | nil => exact hi
  | skip _ _ ih => exact ih (fun hm c hc => hcs (List.mem_cons_of_mem _ hm) c hc) hi
  | pick _ hc _ ih =>
    exact ih (fun hm c hc => hcs (List.mem_cons_of_mem _ hm) c hc)
      (sys.inv_set hi fun _ => hcs (List.mem_cons_self ..) _ hc)

/-- **`monotone_vars` is sound for a step.** A successor of a pre-state that meets `Pre`
moves each cell of a variable that is not cyclic along an edge of its write graph, or keeps
it. -/
theorem succ_covers {a : SAct V Ent} (ha : a ∈ sys.acts) {b : Nat → Nat} {σ τ : St V}
    (hp : sys.Pre a b σ) (hs : sys.Succ a b σ τ) :
    ∀ v k, ¬ sys.Cyclic v → StepOrStay (sys.Edge v) (σ v k) (τ v k) := by
  have h₁ := sys.inv_det ha hp a.effects σ (fun _ h => h) (sys.inv_refl σ)
  have h₂ := sys.inv_deletes ha hp _ _ (fun _ h => mem_deletes h) h₁
  exact sys.inv_pick (fun hm c hc => sys.choice_edge ha hp (fun _ h => h) hm c hc) hs h₂

/-- The system's steps: an action with a binding whose guard holds (`guard`) and one of its
successors. -/
def lts (init : St V → Prop) (guard : SAct V Ent → (Nat → Nat) → St V → Prop) :
    LTS (St V) (SAct V Ent × (Nat → Nat)) where
  init := init
  step σ l τ := l.1 ∈ sys.acts ∧ guard l.1 l.2 σ ∧ sys.Succ l.1 l.2 σ τ

/-- **A monotone variable changes finitely often, slot by slot**, along every behaviour of
the system, when the guards give `Pre` on every reachable state and its values lie in a
list. -/
theorem monotone_finite_changes {init : St V → Prop}
    {guard : SAct V Ent → (Nat → Nat) → St V → Prop}
    (hpre : ∀ {σ a b}, (sys.lts init guard).Reach σ → guard a b σ → sys.Pre a b σ)
    {v : V} (hm : sys.Monotone v) (k : Nat) (L : List Nat)
    (hL : ∀ σ, (sys.lts init guard).Reach σ → σ v k ∈ L) {ρ : Nat → St V}
    (hb : (sys.lts init guard).Behaviour ρ) :
    ¬ InfOften (fun n => ρ (n + 1) v k ≠ ρ n v k) :=
  have hc : Covers (sys.lts init guard) (fun σ => σ v k) (sys.Edge v) := by
    intro σ l τ hr hs
    exact sys.succ_covers hs.1 (hpre hr hs.2.1) hs.2.2 v k hm.1
  finite_changes_of_behaviour hc hm.2 L hL hb

/-- An existence that only creates and deletes write is monotone when it is not cyclic: its
only edges are absent to present and present to deleted. -/
theorem existence_monotone {v : V} (hc : ¬ sys.Cyclic v)
    (honly : ∀ a ∈ sys.acts, ∀ e ∈ a.effects, ∀ f t, sys.EdgeOf a e v f t →
      (f = 0 ∧ t = 1) ∨ (f = 1 ∧ t = 2)) : sys.Monotone v := by
  refine ⟨hc, acyclic_of_rank id fun f t ⟨a, ha, e, he, hx⟩ _ => ?_⟩
  rcases honly a ha e he f t hx with ⟨rfl, rfl⟩ | ⟨rfl, rfl⟩ <;> decide

end Sys

end Effects

end RdraIsh.Mono
