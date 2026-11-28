import RdraIsh.Fairness

/-!
# The liveness search by fair components (native.md, N9)

`ComponentRun::avoid` (`explore/liveness.rs`) decides `□(trig ⇒ ◇goal)` at one binding on a
complete explored graph. This file states what it accepts and proves that it accepts exactly
when a behaviour that satisfies `Fair_P`, stops only where the stop rule allows, and violates
the property exists.

The search, read abstractly on a system `M` whose reachable states lie in a list:

- `N` is the goal-free states, and `E_N` (`NEdge`) the steps between two different states of
  `N`. A *component* is mutual reachability by `E_N` (`SameComp`); Tarjan's algorithm, which
  computes it, is not modelled.
- A component is *fair* (`FairComp`) when it has a second state and every fair instance
  enabled at every one of its states is taken by a step between two of its states.
  Enabledness reads every step of `M`, so a step that leaves `N` still enables its instance.
- A *stop* (`IsStop`) is a state of `N` where no fair instance is enabled and the stop rule
  `stop` holds: `True` under `Plain`, `¬ Moves` under `WfNext` (`isStop_moves_iff`: the code
  tests `¬ Moves` alone), `LoopingStop` on a reduced cone (`StopRule::Looping`).
- The search *accepts* (`Accepts`) when a reachable state where the trigger holds and the goal
  fails reaches a stop or a state of a fair component by a path of `E_N`.

`accepts_iff`: on a system whose reachable states lie in a list, the search accepts iff some
behaviour satisfies `Fair_P`, stays forever only where `stop` holds, and violates the
property; `accepts_iff_lasso` says the same of lassos, and `liveHolds_iff_not_accepts` reads
it as the verdict of D21. `a15_search` is Theorem A15 with the run's verdict read as "the
search on `R` under `LoopingStop` does not accept" in place of `ReducedRunOk`.

Finite walks are functions `w : Nat → γ` with a length (`IsWalk`), joined by `catW`; the
counterexample is closed into a lasso by `foldSeq` of Behaviour.lean.
-/

namespace RdraIsh

universe u v w

/-! ## Paths and finite walks -/

section Walks

variable {γ : Type u}

/-- The reflexive-transitive closure of `E`. -/
inductive Star (E : γ → γ → Prop) : γ → γ → Prop
  | refl (x : γ) : Star E x x
  | tail {x y z : γ} : Star E x y → E y z → Star E x z

namespace Star

variable {E : γ → γ → Prop}

theorem single {x y : γ} (h : E x y) : Star E x y := .tail (.refl x) h

theorem trans {x y z : γ} (h₁ : Star E x y) (h₂ : Star E y z) : Star E x z := by
  induction h₂ with
  | refl => exact h₁
  | tail _ he ih => exact .tail ih he

theorem head {x y z : γ} (he : E x y) (h : Star E y z) : Star E x z := (single he).trans h

/-- A path is empty or starts with a step. -/
theorem cases_head {x y : γ} (h : Star E x y) : x = y ∨ ∃ z, E x z ∧ Star E z y := by
  induction h with
  | refl => exact .inl rfl
  | @tail b c _ he ih =>
    rcases ih with rfl | ⟨z, hz, hp⟩
    · exact .inr ⟨c, he, .refl c⟩
    · exact .inr ⟨z, hz, .tail hp he⟩

/-- A property every step establishes holds at the end of a path from where it holds. -/
theorem keep {P : γ → Prop} (hP : ∀ a b, E a b → P b) {x y : γ} (h : Star E x y)
    (hx : P x) : P y := by
  induction h with
  | refl => exact hx
  | tail _ he _ => exact hP _ _ he

end Star

/-- `w 0, w 1, …, w n` is a walk of `E`. -/
def IsWalk (E : γ → γ → Prop) (w : Nat → γ) (n : Nat) : Prop :=
  ∀ j, j < n → E (w j) (w (j + 1))

/-- The walk `w₁` up to `n₁`, then `w₂`. -/
def catW (w₁ : Nat → γ) (n₁ : Nat) (w₂ : Nat → γ) (k : Nat) : γ :=
  if k ≤ n₁ then w₁ k else w₂ (k - n₁)

theorem catW_le {w₁ w₂ : Nat → γ} {n₁ k : Nat} (h : k ≤ n₁) : catW w₁ n₁ w₂ k = w₁ k :=
  ite_eq_left h

theorem catW_add {w₁ w₂ : Nat → γ} {n₁ : Nat} (hj : w₁ n₁ = w₂ 0) (j : Nat) :
    catW w₁ n₁ w₂ (n₁ + j) = w₂ j := by
  unfold catW
  split
  · next h =>
    have : j = 0 := by omega
    subst this
    simpa using hj
  · congr 1; omega

theorem IsWalk.mono {E E' : γ → γ → Prop} (hE : ∀ a b, E a b → E' a b) {w : Nat → γ} {n : Nat}
    (h : IsWalk E w n) : IsWalk E' w n := fun j hj => hE _ _ (h j hj)

theorem IsWalk.cat {E : γ → γ → Prop} {w₁ w₂ : Nat → γ} {n₁ n₂ : Nat} (h₁ : IsWalk E w₁ n₁)
    (h₂ : IsWalk E w₂ n₂) (hj : w₁ n₁ = w₂ 0) : IsWalk E (catW w₁ n₁ w₂) (n₁ + n₂) := by
  intro j hj'
  by_cases hlt : j < n₁
  · rw [catW_le (by omega), catW_le (by omega)]; exact h₁ j hlt
  · obtain ⟨i, rfl⟩ : ∃ i, j = n₁ + i := ⟨j - n₁, by omega⟩
    rw [catW_add hj, show n₁ + i + 1 = n₁ + (i + 1) by omega, catW_add hj]
    exact h₂ i (by omega)

/-- The walk `w` up to `n`, then `z`. -/
def snocW (w : Nat → γ) (n : Nat) (z : γ) (k : Nat) : γ := if k ≤ n then w k else z

theorem snocW_le {w : Nat → γ} {n k : Nat} {z : γ} (h : k ≤ n) : snocW w n z k = w k :=
  ite_eq_left h

theorem snocW_succ {w : Nat → γ} {n : Nat} {z : γ} : snocW w n z (n + 1) = z :=
  ite_eq_right (by omega)

/-- A walk extended by one step. -/
theorem IsWalk.snoc {E : γ → γ → Prop} {w : Nat → γ} {n : Nat} (h : IsWalk E w n) {z : γ}
    (he : E (w n) z) : IsWalk E (snocW w n z) (n + 1) := by
  intro j hj
  by_cases hjn : j < n
  · rw [snocW_le (by omega), snocW_le (by omega)]; exact h j hjn
  · have : j = n := by omega
    subst this
    rw [snocW_le (Nat.le_refl _), snocW_succ]; exact he

theorem IsWalk.star {E : γ → γ → Prop} {w : Nat → γ} :
    ∀ {n : Nat}, IsWalk E w n → Star E (w 0) (w n)
  | 0, _ => .refl _
  | n + 1, h => .tail (IsWalk.star fun j hj => h j (by omega)) (h n (by omega))

/-- A property every step keeps holds along a walk. -/
theorem IsWalk.keep {E : γ → γ → Prop} {P : γ → Prop} (hP : ∀ a b, E a b → P a → P b)
    {w : Nat → γ} {n : Nat} (h : IsWalk E w n) (h0 : P (w 0)) : ∀ k, k ≤ n → P (w k) := by
  intro k
  induction k with
  | zero => intro; exact h0
  | succ k ih => intro hk; exact hP _ _ (h k (by omega)) (ih (by omega))

theorem Star.walk {E : γ → γ → Prop} {x y : γ} (h : Star E x y) :
    ∃ w n, w 0 = x ∧ w n = y ∧ IsWalk E w n := by
  induction h with
  | refl => exact ⟨fun _ => x, 0, rfl, rfl, fun j hj => absurd hj (Nat.not_lt_zero _)⟩
  | @tail b c _ he ih =>
    obtain ⟨w, n, h0, hn, hw⟩ := ih
    exact ⟨snocW w n c, n + 1, by rw [snocW_le (Nat.zero_le _), h0], snocW_succ,
      hw.snoc (by rw [hn]; exact he)⟩

/-- A walk from `a` through the step `b → c` to `d`. -/
theorem walk_through {E : γ → γ → Prop} {a b c d : γ} (h₁ : Star E a b) (he : E b c)
    (h₂ : Star E c d) :
    ∃ w n, w 0 = a ∧ w n = d ∧ IsWalk E w n ∧ ∃ j, j < n ∧ w j = b ∧ w (j + 1) = c := by
  obtain ⟨w₁, n₁, h10, h1n, hw₁⟩ := h₁.walk
  have hs := hw₁.snoc (z := c) (by rw [h1n]; exact he)
  obtain ⟨w₂, n₂, h20, h2n, hw₂⟩ := h₂.walk
  have hj : snocW w₁ n₁ c (n₁ + 1) = w₂ 0 := by rw [snocW_succ, h20]
  refine ⟨_, _, ?_, ?_, hs.cat hw₂ hj, n₁, by omega, ?_, ?_⟩
  · rw [catW_le (Nat.zero_le _), snocW_le (Nat.zero_le _), h10]
  · rw [catW_add hj, h2n]
  · rw [catW_le (by omega), snocW_le (Nat.le_refl _), h1n]
  · rw [catW_le (Nat.le_refl _), snocW_succ]

end Walks

/-! ## Folding a prefix and a cycle into a lasso -/

section FoldFacts

variable {σ : Type u} (ρ : Nat → σ) {n₀ m : Nat}

/-- Each step of the fold is a step of `ρ` before `m`, at the same index before `n₀`. -/
theorem fold_step (hnm : n₀ < m) (hmo : ρ m = ρ n₀) (n : Nat) :
    ∃ j, j < m ∧ foldSeq ρ (n₀ := n₀) (m := m) n = ρ j ∧
      foldSeq ρ (n₀ := n₀) (m := m) (n + 1) = ρ (j + 1) ∧ (n < n₀ → j = n) ∧
      (n₀ ≤ n → n₀ ≤ j) :=
  ⟨foldIdx n₀ m n, foldIdx_lt hnm n, rfl, foldSeq_succ ρ hmo n,
    fun hn => foldIdx_eq n (by omega), foldIdx_ge hnm n⟩

/-- Each step of `ρ` in `[n₀, m)` recurs in the fold after any time. -/
theorem fold_recurs (hnm : n₀ < m) (hmo : ρ m = ρ n₀) {j : Nat} (hj1 : n₀ ≤ j) (hj2 : j < m)
    (N : Nat) : ∃ n, N ≤ n ∧ foldSeq ρ (n₀ := n₀) (m := m) n = ρ j ∧
      foldSeq ρ (n₀ := n₀) (m := m) (n + 1) = ρ (j + 1) := by
  obtain ⟨d, hd, _⟩ := foldIdx_reaches hnm (max N n₀) (by omega) j hj1 hj2
  refine ⟨max N n₀ + d, by omega, ?_, ?_⟩
  · show ρ (foldIdx n₀ m _) = _; rw [hd]
  · rw [foldSeq_succ ρ hmo, hd]

end FoldFacts

namespace LTS

variable {σ : Type u} {α : Type v} {Φ : Type w} (M : LTS σ α)

theorem Reach.walk {M : LTS σ α} {s : σ} (h : M.Reach s) :
    ∃ w n, M.init (w 0) ∧ w n = s ∧ IsWalk M.Next w n := by
  induction h with
  | @init s hi => exact ⟨fun _ => s, 0, hi, rfl, fun j hj => absurd hj (Nat.not_lt_zero _)⟩
  | @step s a t _ hst ih =>
    obtain ⟨w, n, h0, hn, hw⟩ := ih
    exact ⟨snocW w n t, n + 1, by rw [snocW_le (Nat.zero_le _)]; exact h0, snocW_succ,
      hw.snoc (by rw [hn]; exact ⟨a, hst⟩)⟩

/-! ## The search -/

/-- An edge of `E_N`: a step between two different goal-free states. -/
def NEdge (goal : σ → Prop) (s s' : σ) : Prop := ¬ goal s ∧ ¬ goal s' ∧ s' ≠ s ∧ M.Next s s'

/-- `s` and `u` lie in one strongly connected component of `(N, E_N)`. -/
def SameComp (goal : σ → Prop) (s u : σ) : Prop :=
  Star (M.NEdge goal) s u ∧ Star (M.NEdge goal) u s

/-- The component of `e` is fair (native.md, "The check"; `avoid`'s `fair_cycle`): `e` is a
reachable goal-free state, the component has a second state (`size ≥ 2`), and every fair
instance enabled at every state of the component (`always`) is taken by a step between two of
its states (`taken`). -/
def FairComp (inst : Φ → α → Prop) (goal : σ → Prop) (e : σ) : Prop :=
  M.Reach e ∧ ¬ goal e ∧ (∃ u, u ≠ e ∧ M.SameComp goal e u) ∧
    ∀ f, (∀ u, M.SameComp goal e u → M.Enabled inst f u) →
      ∃ u v, M.SameComp goal e u ∧ M.SameComp goal e v ∧ M.Takes inst f u v

/-- A stop (`avoid`'s `stops`): a goal-free state where no fair instance is enabled and the
stop rule holds. -/
def IsStop (inst : Φ → α → Prop) (stop goal : σ → Prop) (s : σ) : Prop :=
  ¬ goal s ∧ (∀ f, ¬ M.Enabled inst f s) ∧ stop s

/-- The search accepts (`liveness_search` finds a start): a reachable state where the
trigger holds and the goal fails reaches, by a path of `E_N`, a stop or a state of a fair
component. -/
def Accepts (inst : Φ → α → Prop) (stop trig goal : σ → Prop) : Prop :=
  ∃ s e, M.Reach s ∧ trig s ∧ ¬ goal s ∧ Star (M.NEdge goal) s e ∧
    (M.IsStop inst stop goal e ∨ M.FairComp inst goal e)

variable {M}

/-- An enabled instance gives a move. -/
theorem Enabled.moves {inst : Φ → α → Prop} {f : Φ} {s : σ} (h : M.Enabled inst f s) :
    M.Moves s :=
  let ⟨s', hne, a, _, hs⟩ := h; ⟨s', hne, a, hs⟩

/-- Under `WfNext` the code's stop test is `¬ Moves` alone: no move means no enabled
instance. -/
theorem isStop_moves_iff (inst : Φ → α → Prop) (goal : σ → Prop) (s : σ) :
    M.IsStop inst (fun q => ¬ M.Moves q) goal s ↔ ¬ goal s ∧ ¬ M.Moves s :=
  ⟨fun ⟨hg, _, hm⟩ => ⟨hg, hm⟩, fun ⟨hg, hm⟩ => ⟨hg, fun _ he => hm he.moves, hm⟩⟩

/-- Under `Plain` the stop rule is `True`: a stop is a goal-free state with no enabled
instance. -/
theorem isStop_true_iff (inst : Φ → α → Prop) (goal : σ → Prop) (s : σ) :
    M.IsStop inst (fun _ => True) goal s ↔ ¬ goal s ∧ ∀ f, ¬ M.Enabled inst f s :=
  ⟨fun ⟨hg, he, _⟩ => ⟨hg, he⟩, fun ⟨hg, he⟩ => ⟨hg, he, trivial⟩⟩

section Search

variable {inst : Φ → α → Prop} {stop trig goal : σ → Prop}

theorem star_goalFree {s e : σ} (h : Star (M.NEdge goal) s e) (hs : ¬ goal s) : ¬ goal e :=
  h.keep (P := fun x => ¬ goal x) (fun _ _ he => he.2.1) hs

theorem SameComp.goalFree {e u : σ} (h : M.SameComp goal e u) (he : ¬ goal e) : ¬ goal u :=
  star_goalFree h.1 he

theorem star_reach {s e : σ} (h : Star (M.NEdge goal) s e) (hs : M.Reach s) : M.Reach e := by
  induction h with
  | refl => exact hs
  | tail _ hn ih => obtain ⟨a, ha⟩ := hn.2.2.2; exact Reach.step ih ha

theorem SameComp.reach {e u : σ} (h : M.SameComp goal e u) (he : M.Reach e) : M.Reach u :=
  star_reach h.1 he

/-- Every state of a fair component has an edge to a state of the component. -/
theorem FairComp.out {e u : σ} (hfc : M.FairComp inst goal e) (hu : M.SameComp goal e u) :
    ∃ v, M.SameComp goal e v ∧ M.NEdge goal u v := by
  obtain ⟨_, _, ⟨u', hne, hu'⟩, _⟩ := hfc
  -- A path that leaves `u` gives the edge: it ends in the component and starts at `u`.
  have step_of : ∀ y, Star (M.NEdge goal) u y → u ≠ y → Star (M.NEdge goal) y e →
      ∃ v, M.SameComp goal e v ∧ M.NEdge goal u v := by
    intro y hp hne' hback
    rcases hp.cases_head with h | ⟨z, hz, hzy⟩
    · exact absurd h hne'
    · exact ⟨z, ⟨hu.1.tail hz, hzy.trans hback⟩, hz⟩
  by_cases hue : u = e
  · subst hue
    exact step_of u' hu'.1 (Ne.symm hne) hu'.2
  · exact step_of e hu.2 hue (.refl e)

/-- **A fair component has a covering cycle.** On finitely many reachable states, a fair
component holds a closed walk of `E_N` from `e`, with at least one step, that takes every
edge of `E_N` between two states of the component. -/
theorem FairComp.cycle (L : List σ) (hL : ∀ s, M.Reach s → s ∈ L) {e : σ}
    (hfc : M.FairComp inst goal e) :
    ∃ w n, 0 < n ∧ w 0 = e ∧ w n = e ∧ IsWalk (M.NEdge goal) w n ∧
      ∀ u v, M.SameComp goal e u → M.SameComp goal e v → M.NEdge goal u v →
        ∃ j, j < n ∧ w j = u ∧ w (j + 1) = v := by
  let E := M.NEdge goal
  let Good : (Nat → σ) → Nat → σ × σ → Prop := fun w n uv =>
    M.SameComp goal e uv.1 → M.SameComp goal e uv.2 → E uv.1 uv.2 →
      ∃ j, j < n ∧ w j = uv.1 ∧ w (j + 1) = uv.2
  -- A closed walk through each edge of the component.
  have through : ∀ u v, M.SameComp goal e u → M.SameComp goal e v → E u v →
      ∃ w n, w 0 = e ∧ w n = e ∧ IsWalk E w n ∧ ∃ j, j < n ∧ w j = u ∧ w (j + 1) = v :=
    fun u v hu hv he => walk_through hu.1 he hv.2
  obtain ⟨v₀, hv₀, he₀⟩ := hfc.out ⟨.refl e, .refl e⟩
  obtain ⟨w₀, n₀, h00, h0n, hw₀, j₀, hj₀, _, _⟩ :=
    through e v₀ ⟨.refl e, .refl e⟩ hv₀ he₀
  have main : ∀ T : List (σ × σ), ∃ w n, 0 < n ∧ w 0 = e ∧ w n = e ∧ IsWalk E w n ∧
      ∀ uv, uv ∈ T → Good w n uv := by
    intro T
    induction T with
    | nil => exact ⟨w₀, n₀, by omega, h00, h0n, hw₀, fun _ h => nomatch h⟩
    | cons uv T ih =>
      obtain ⟨w, n, hn, h0, hnn, hw, hT⟩ := ih
      -- The walk for `uv`: through its edge when it is one of the component, else `w₀`.
      obtain ⟨w', n', h0', hn', hw', hgood⟩ : ∃ w' n', w' 0 = e ∧ w' n' = e ∧
          IsWalk E w' n' ∧ Good w' n' uv := by
        by_cases hc : M.SameComp goal e uv.1 ∧ M.SameComp goal e uv.2 ∧ E uv.1 uv.2
        · obtain ⟨w', n', a, b, c, d⟩ := through _ _ hc.1 hc.2.1 hc.2.2
          exact ⟨w', n', a, b, c, fun _ _ _ => d⟩
        · exact ⟨w₀, n₀, h00, h0n, hw₀, fun h1 h2 h3 => absurd ⟨h1, h2, h3⟩ hc⟩
      have hj : w n = w' 0 := by rw [hnn, h0']
      refine ⟨catW w n w', n + n', by omega, by rw [catW_le (Nat.zero_le _), h0], ?_,
        hw.cat hw' hj, fun x hx => ?_⟩
      · rw [catW_add hj, hn']
      · rcases List.mem_cons.mp hx with rfl | hx
        · intro h1 h2 h3
          obtain ⟨j, hj1, hj2, hj3⟩ := hgood h1 h2 h3
          refine ⟨n + j, by omega, by rw [catW_add hj, hj2], ?_⟩
          rw [show n + j + 1 = n + (j + 1) by omega, catW_add hj, hj3]
        · intro h1 h2 h3
          obtain ⟨j, hj1, hj2, hj3⟩ := hT x hx h1 h2 h3
          exact ⟨j, by omega, by rw [catW_le (by omega), hj2],
            by rw [catW_le (by omega), hj3]⟩
  obtain ⟨w, n, hn, h0, hnn, hw, hT⟩ := main (pairs L)
  refine ⟨w, n, hn, h0, hnn, hw, fun u v hu hv he => ?_⟩
  have hre := hfc.1
  exact hT (u, v) (mem_pairs (hL _ (hu.reach hre)) (hL _ (hv.reach hre))) hu hv he

/-- **The lasso of a prefix, a path and a cycle.** From an initial state to `s` by steps of
`M`, from `s` to `e` by edges of `E_N`, then round a closed walk at `e` of stutters and
edges of `E_N` with at least one step: the fold is a lasso and a behaviour that violates the
property when the trigger holds at `s`; from some time on each of its steps is a step of the
cycle, and each step of the cycle recurs after any time. -/
theorem lasso_of_cycle {s e : σ} {w₁ w₂ w₃ : Nat → σ} {n₁ n₂ n₃ : Nat}
    (h10 : M.init (w₁ 0)) (h1n : w₁ n₁ = s) (hw₁ : IsWalk M.Next w₁ n₁)
    (h20 : w₂ 0 = s) (h2n : w₂ n₂ = e) (hw₂ : IsWalk (M.NEdge goal) w₂ n₂)
    (hn₃ : 0 < n₃) (h30 : w₃ 0 = e) (h3n : w₃ n₃ = e)
    (hw₃ : IsWalk (fun x y => y = x ∨ M.NEdge goal x y) w₃ n₃)
    (hts : trig s) (hgs : ¬ goal s) :
    ∃ ρ, Lasso ρ ∧ M.Behaviour ρ ∧ Violates trig goal ρ ∧
      (∃ N₀, ∀ n, N₀ ≤ n → ∃ j, j < n₃ ∧ ρ n = w₃ j ∧ ρ (n + 1) = w₃ (j + 1)) ∧
      ∀ j, j < n₃ → ∀ N, ∃ n, N ≤ n ∧ ρ n = w₃ j ∧ ρ (n + 1) = w₃ (j + 1) := by
  let S : σ → σ → Prop := fun x y => y = x ∨ M.NEdge goal x y
  let B : σ → σ → Prop := fun x y => y = x ∨ M.Next x y
  have hj23 : w₂ n₂ = w₃ 0 := by rw [h2n, h30]
  let τ := catW w₂ n₂ w₃
  have hτ : IsWalk S τ (n₂ + n₃) := (hw₂.mono fun _ _ h => .inr h).cat hw₃ hj23
  have hτ0 : τ 0 = s := by show catW w₂ n₂ w₃ 0 = s; rw [catW_le (Nat.zero_le _), h20]
  have hj1 : w₁ n₁ = τ 0 := by rw [h1n, hτ0]
  let ρ₀ := catW w₁ n₁ τ
  have hρ₀ : IsWalk B ρ₀ (n₁ + (n₂ + n₃)) :=
    (hw₁.mono fun _ _ h => .inr h).cat (hτ.mono fun _ _ h => h.imp id fun h => h.2.2.2) hj1
  have hcyc : ∀ j, ρ₀ (n₁ + n₂ + j) = w₃ j := by
    intro j
    show catW w₁ n₁ τ (n₁ + n₂ + j) = w₃ j
    rw [Nat.add_assoc, catW_add hj1]
    show catW w₂ n₂ w₃ (n₂ + j) = w₃ j
    rw [catW_add hj23]
  have hfree : ∀ k, n₁ ≤ k → k ≤ n₁ + (n₂ + n₃) → ¬ goal (ρ₀ k) := by
    intro k hk1 hk2
    obtain ⟨i, rfl⟩ : ∃ i, k = n₁ + i := ⟨k - n₁, by omega⟩
    show ¬ goal (catW w₁ n₁ τ (n₁ + i))
    rw [catW_add hj1]
    refine hτ.keep (P := fun x => ¬ goal x) (fun _ _ h hx => ?_) (by rw [hτ0]; exact hgs) i
      (by omega)
    rcases h with rfl | h
    · exact hx
    · exact h.2.1
  let n₀ := n₁ + n₂
  let m := n₀ + n₃
  have hnm : n₀ < m := by omega
  have hmo : ρ₀ m = ρ₀ n₀ := by
    show ρ₀ (n₁ + n₂ + n₃) = ρ₀ (n₁ + n₂ + 0); rw [hcyc, hcyc, h3n, h30]
  have hm : m = n₁ + (n₂ + n₃) := by omega
  refine ⟨foldSeq ρ₀ (n₀ := n₀) (m := m), foldSeq_lasso ρ₀ hnm, ⟨?_, fun n => ?_⟩, ⟨n₁, ?_, fun j hj => ?_⟩, ⟨n₀, ?_⟩, ?_⟩
  · show M.init (ρ₀ (foldIdx n₀ m 0))
    show M.init (catW w₁ n₁ τ 0)
    rw [catW_le (Nat.zero_le _)]; exact h10
  · obtain ⟨j, hj, h1, h2, _⟩ := fold_step ρ₀ hnm hmo n
    rw [h1, h2]; exact hρ₀ j (by omega)
  · show trig (ρ₀ (foldIdx n₀ m n₁))
    rw [foldIdx_eq n₁ (by omega)]
    show trig (catW w₁ n₁ τ n₁)
    rw [catW_le (Nat.le_refl _), h1n]; exact hts
  · obtain ⟨j', hj', h1, _, h3, h4⟩ := fold_step ρ₀ hnm hmo j
    rw [h1]
    refine hfree j' ?_ (by omega)
    by_cases hjn : j < n₀
    · rw [h3 hjn]; exact hj
    · have := h4 (by omega); omega
  · intro n hn
    obtain ⟨j', hj', h1, h2, _, h4⟩ := fold_step ρ₀ hnm hmo n
    have hj0 := h4 hn
    obtain ⟨i, rfl⟩ : ∃ i, j' = n₀ + i := ⟨j' - n₀, by omega⟩
    refine ⟨i, by omega, by rw [h1]; exact hcyc i, ?_⟩
    rw [h2, show n₀ + i + 1 = n₁ + n₂ + (i + 1) by omega, hcyc]
  · intro j hj N
    obtain ⟨n, hn, h1, h2⟩ := fold_recurs ρ₀ hnm hmo (j := n₀ + j) (by omega) (by omega) N
    refine ⟨n, hn, by rw [h1]; exact hcyc j, ?_⟩
    rw [h2, show n₀ + j + 1 = n₁ + n₂ + (j + 1) by omega, hcyc]

/-- **N9, (⇐).** If the search accepts, some lasso is a behaviour that satisfies `Fair_P`,
stays forever only where `stop` holds, and violates the property. -/
theorem lasso_of_accepts (L : List σ) (hL : ∀ s, M.Reach s → s ∈ L)
    (h : M.Accepts inst stop trig goal) :
    ∃ ρ, Lasso ρ ∧ M.Behaviour ρ ∧ M.FairP inst ρ ∧ StopsOnly stop ρ ∧
      Violates trig goal ρ := by
  obtain ⟨s, e, hrs, hts, hgs, hpath, hend⟩ := h
  obtain ⟨w₁, n₁, h10, h1n, hw₁⟩ := hrs.walk
  obtain ⟨w₂, n₂, h20, h2n, hw₂⟩ := hpath.walk
  rcases hend with ⟨_, hnone, hstop⟩ | hfc
  · -- Stutter at the stop forever.
    obtain ⟨ρ, hl, hb, hv, ⟨N₀, hT1⟩, _⟩ :=
      lasso_of_cycle (w₃ := fun _ => e) (n₃ := 1) h10 h1n hw₁ h20 h2n hw₂ (by omega) rfl rfl
        (fun _ _ => .inl rfl) hts hgs
    have hconst : ∀ n, N₀ ≤ n → ρ n = e := fun n hn =>
      let ⟨_, _, h1, _⟩ := hT1 n hn; h1
    refine ⟨ρ, hl, hb, fun f ⟨N, hN⟩ => absurd ?_ (hnone f), fun N q hq => ?_, hv⟩
    · have : M.Enabled inst f (ρ (max N N₀)) := hN _ (by omega)
      rwa [hconst _ (by omega)] at this
    · rw [← hq (max N N₀) (by omega), hconst _ (by omega)]; exact hstop
  · obtain ⟨w₃, n₃, hn₃, h30, h3n, hw₃, hcov⟩ := hfc.cycle L hL
    obtain ⟨ρ, hl, hb, hv, _, hT2⟩ :=
      lasso_of_cycle h10 h1n hw₁ h20 h2n hw₂ hn₃ h30 h3n (hw₃.mono fun _ _ h => .inr h) hts hgs
    have hgoal_e : ¬ goal e := star_goalFree hpath hgs
    -- Every state of the component is a state of the cycle.
    have hvisit : ∀ u, M.SameComp goal e u → ∃ j, j < n₃ ∧ w₃ j = u := by
      intro u hu
      obtain ⟨v, hv', huv⟩ := hfc.out hu
      obtain ⟨j, hj, hju, _⟩ := hcov u v hu hv' huv
      exact ⟨j, hj, hju⟩
    refine ⟨ρ, hl, hb, fun f ⟨N, hN⟩ N' => ?_, fun N q hq => ?_, hv⟩
    · have hall : ∀ u, M.SameComp goal e u → M.Enabled inst f u := by
        intro u hu
        obtain ⟨j, hj, hju⟩ := hvisit u hu
        obtain ⟨n, hn, h1, _⟩ := hT2 j hj N
        rw [← hju, ← h1]; exact hN n hn
      obtain ⟨u, v, hu, hv', htk⟩ := hfc.2.2.2 f hall
      have hedge : M.NEdge goal u v :=
        ⟨hu.goalFree hgoal_e, hv'.goalFree hgoal_e, htk.1, let ⟨a, _, ha⟩ := htk.2; ⟨a, ha⟩⟩
      obtain ⟨j, hj, hju, hjv⟩ := hcov u v hu hv' hedge
      obtain ⟨n, hn, h1, h2⟩ := hT2 j hj N'
      exact ⟨n, hn, by show M.Takes inst f (ρ n) (ρ (n + 1)); rw [h1, h2, hju, hjv]; exact htk⟩
    · exfalso
      obtain ⟨u, hne, hu⟩ := hfc.2.2.1
      obtain ⟨j, hj, hju⟩ := hvisit u hu
      obtain ⟨n, hn, h1, _⟩ := hT2 j hj N
      obtain ⟨n', hn', h1', _⟩ := hT2 0 hn₃ N
      apply hne
      rw [← hju, ← h1, hq n hn, ← hq n' hn', h1', h30]

/-- **N9, (⇒).** On finitely many reachable states, a behaviour that satisfies `Fair_P`,
stays forever only where `stop` holds, and violates the property makes the search accept. -/
theorem accepts_of_behaviour (L : List σ) (hL : ∀ s, M.Reach s → s ∈ L) {ρ : Nat → σ}
    (hb : M.Behaviour ρ) (hf : M.FairP inst ρ) (hs : StopsOnly stop ρ)
    (hv : Violates trig goal ρ) : M.Accepts inst stop trig goal := by
  obtain ⟨i, hti, hgi⟩ := hv
  obtain ⟨n₀, m, hin, hnm, hmo, hcov⟩ := recurrent_segment ρ L (fun n => hL _ (hb.reach n)) i
  have hpath : ∀ a, i ≤ a → ∀ d, Star (M.NEdge goal) (ρ a) (ρ (a + d)) := by
    intro a ha d
    induction d with
    | zero => exact .refl _
    | succ d ih =>
      rw [show a + (d + 1) = a + d + 1 by omega]
      by_cases he : ρ (a + d + 1) = ρ (a + d)
      · rw [he]; exact ih
      · rcases hb.2 (a + d) with h | h
        · exact absurd h he
        · exact .tail ih ⟨hgi _ (by omega), hgi _ (by omega), he, h⟩
  have hto : ∀ a b, i ≤ a → a ≤ b → Star (M.NEdge goal) (ρ a) (ρ b) := by
    intro a b ha hab
    have := hpath a ha (b - a); rwa [show a + (b - a) = b by omega] at this
  have hcomp : ∀ k, n₀ ≤ k → M.SameComp goal (ρ n₀) (ρ k) := by
    intro k hk
    refine ⟨hto n₀ k (by omega) hk, ?_⟩
    obtain ⟨j, hj1, hj2, hj3, _⟩ := hcov k hk
    rw [← hj3, ← hmo]
    exact hto j m (by omega) (by omega)
  refine ⟨ρ i, ρ n₀, hb.reach i, hti, hgi i (Nat.le_refl _), hto i n₀ (Nat.le_refl _) hin, ?_⟩
  by_cases hex : ∃ k, n₀ ≤ k ∧ ρ k ≠ ρ n₀
  · obtain ⟨k, hk, hne⟩ := hex
    refine .inr ⟨hb.reach n₀, hgi n₀ hin, ⟨ρ k, hne, hcomp k hk⟩, fun f hall => ?_⟩
    obtain ⟨k', hk', htk⟩ := hf f ⟨n₀, fun k hk => hall _ (hcomp k hk)⟩ n₀
    exact ⟨ρ k', ρ (k' + 1), hcomp k' hk', hcomp (k' + 1) (by omega), htk⟩
  · have hconst : ∀ k, n₀ ≤ k → ρ k = ρ n₀ := fun k hk =>
      Classical.byContradiction fun hne => hex ⟨k, hk, hne⟩
    refine .inl ⟨hgi n₀ hin, fun f hen => ?_, hs n₀ (ρ n₀) hconst⟩
    obtain ⟨k, hk, htk⟩ :=
      hf f ⟨n₀, fun k hk => by show M.Enabled inst f (ρ k); rw [hconst k hk]; exact hen⟩ n₀
    exact htk.1 (by rw [hconst k hk, hconst (k + 1) (by omega)])

/-- **N9.** On a system whose reachable states lie in a list, the search accepts iff some
behaviour satisfies `Fair_P`, stays forever only where `stop` holds, and violates
`□(trig ⇒ ◇goal)`. -/
theorem accepts_iff (L : List σ) (hL : ∀ s, M.Reach s → s ∈ L) :
    M.Accepts inst stop trig goal ↔
      ∃ ρ, M.Behaviour ρ ∧ M.FairP inst ρ ∧ StopsOnly stop ρ ∧ Violates trig goal ρ := by
  constructor
  · intro h
    obtain ⟨ρ, _, hb, hf, hs, hv⟩ := lasso_of_accepts L hL h
    exact ⟨ρ, hb, hf, hs, hv⟩
  · rintro ⟨ρ, hb, hf, hs, hv⟩
    exact accepts_of_behaviour L hL hb hf hs hv

/-- **N9, on lassos.** The same with the behaviour a lasso. -/
theorem accepts_iff_lasso (L : List σ) (hL : ∀ s, M.Reach s → s ∈ L) :
    M.Accepts inst stop trig goal ↔
      ∃ ρ, Lasso ρ ∧ M.Behaviour ρ ∧ M.FairP inst ρ ∧ StopsOnly stop ρ ∧
        Violates trig goal ρ := by
  constructor
  · exact lasso_of_accepts L hL
  · rintro ⟨ρ, _, hb, hf, hs, hv⟩
    exact accepts_of_behaviour L hL hb hf hs hv

/-- **N9, the verdict.** The property holds under `Fair_P` and the stop rule (D21) iff the
search does not accept. -/
theorem liveHolds_iff_not_accepts (L : List σ) (hL : ∀ s, M.Reach s → s ∈ L) :
    M.LiveHolds (fun ρ => M.FairP inst ρ ∧ StopsOnly stop ρ) trig goal ↔
      ¬ M.Accepts inst stop trig goal := by
  rw [accepts_iff L hL]
  constructor
  · rintro hH ⟨ρ, hb, hf, hs, hv⟩
    exact hH ρ hb ⟨hf, hs⟩ hv
  · intro hN ρ hb ⟨hf, hs⟩ hv
    exact hN ⟨ρ, hb, hf, hs, hv⟩

/-- **N9 under `Plain`** (`Fair_P` alone): the stop rule is `True`. -/
theorem liveHolds_plain_iff (L : List σ) (hL : ∀ s, M.Reach s → s ∈ L) :
    M.LiveHolds (fun ρ => M.FairP inst ρ) trig goal ↔
      ¬ M.Accepts inst (fun _ => True) trig goal := by
  rw [← liveHolds_iff_not_accepts L hL]
  exact ⟨fun h ρ hb hf => h ρ hb hf.1, fun h ρ hb hf => h ρ hb ⟨hf, fun _ _ _ => trivial⟩⟩

/-- **N9 under `WfNext`** (`Fair_P ∧ WF_vars(Next)`): the stop rule is `¬ Moves`. -/
theorem liveHolds_wfNext_iff (L : List σ) (hL : ∀ s, M.Reach s → s ∈ L) :
    M.LiveHolds (fun ρ => M.FairP inst ρ ∧ M.WFNext ρ) trig goal ↔
      ¬ M.Accepts inst (fun q => ¬ M.Moves q) trig goal := by
  rw [← liveHolds_iff_not_accepts L hL]
  exact ⟨fun h ρ hb hf => h ρ hb ⟨hf.1, (M.wfNext_iff ρ).2 hf.2⟩,
    fun h ρ hb hf => h ρ hb ⟨hf.1, (M.wfNext_iff ρ).1 hf.2⟩⟩

end Search

end LTS

/-! ## Theorem A15 with the search -/

section A15

open LTS

variable {T S I O Blk Ta Sa XSa Ra Φ : Type}
  {C : LTS (T × S) (Ta ⊕ Sa)} {X : LTS (S × I) (XSa ⊕ Ra)} {obs : S → O}
  {inp : T → I} {B : S × I → Blk} {rep : Blk → S × I}

/-- **Theorem A15, with the run's search.** Under the hypotheses of `a15`: if the search on
`R` under `StopRule::Looping` (stops at a `LoopingStop`) does not accept, then `C ⊨ P` under
`WfNext`. The hypothesis `ReducedRunOk` of `a15` is derived from N9 on `R`. -/
theorem a15_search (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (LX : List (S × I)) (hLX : ∀ x, X.Reach x → x ∈ LX)
    (LR : List (T × Blk)) (hLR : ∀ q, (reduced C X inp B rep).Reach q → q ∈ LR)
    (canLoop : Blk → Prop) (hloop : ∀ c, SilentCycle X B c → canLoop c)
    (inst : Φ → Ta → Prop) (trig goal : T × O → Prop)
    (hrun : ¬ (reduced C X inp B rep).Accepts (onT inst)
      (LoopingStop (reduced C X inp B rep) canLoop)
      (fun q => trig (obsR obs rep q)) (fun q => goal (obsR obs rep q))) :
    C.LiveHolds (fun σ => C.FairP (onT inst) σ ∧ C.WFNext σ)
      (fun p => trig (obsC obs p)) (fun p => goal (obsC obs p)) :=
  a15 h5 h6 hE hX LX hLX LR hLR canLoop hloop inst trig goal fun _ _ hb hf hs hv =>
    hrun (accepts_of_behaviour LR hLR hb hf hs hv)

end A15

end RdraIsh
