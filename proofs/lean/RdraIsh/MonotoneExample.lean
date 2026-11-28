import RdraIsh.Monotone

/-!
# Concrete instances of the monotone-variable theorems

**`ExampleMono`, A15 with no class marked.** The cone of `ExampleFair` with the toggle
replaced by a raise: `T = Bool` is a flag that a fair `T_a` action sets, and `S = Bool` a
sub-lifecycle variable that an `S_a` action takes from `false` to `true` once. `S`'s one cell
follows the write graph `false → true`, which is acyclic, so `no_silentCycle` leaves no block
that can loop, and `a15_monotone` derives `eventually flag` under `Fair_P ∧ WF_vars(Next)`
from a run whose only stops are states no move leaves (`canLoop := fun _ => False`, as
`s_cycles` false gives).

**`ExampleMonoSys`, the code's write graph.** One entity with an existence and a column
`level`; one action creates the row and sets `level` to `0`, another moves `level` through
`0 → 1 → 2`. `monotone_vars`' edges for both variables are acyclic and neither is cyclic, so
both are `Monotone`; the existence by `existence_monotone`.
-/

namespace RdraIsh.ExampleMono

open LTS Mono

def C : LTS (Bool × Bool) (Unit ⊕ Unit) where
  init p := p.1 = false ∧ p.2 = false
  step p l q := match l with
    | .inl () => p.1 = false ∧ q.1 = true ∧ q.2 = p.2
    | .inr () => q.1 = p.1 ∧ p.2 = false ∧ q.2 = true

def X : LTS (Bool × Unit) (Unit ⊕ Empty) where
  init x := x.1 = false
  step x l y := match l with
    | .inl () => x.1 = false ∧ y.1 = true
    | .inr r => nomatch r

def obs : Bool → Unit := fun _ => ()
def inp : Bool → Unit := fun _ => ()
def B : Bool × Unit → Unit := fun _ => ()
def rep : Unit → Bool × Unit := fun _ => (false, ())

def inst : Unit → Unit → Prop := fun _ _ => True
def trig : Bool × Unit → Prop := fun _ => True
def goal : Bool × Unit → Prop := fun o => o.1 = true

def R : LTS (Bool × Unit) (Unit ⊕ Unit) := reduced C X inp B rep

theorem split : Split C obs where
  s_keeps_t h := h.1
  t_keeps_s h := h.2.2
  t_reads_obs _ h := ⟨h.1, h.2.1, rfl⟩

theorem lemma6 : Lemma6 C X inp where
  init h := h.2
  s_step _ h := ⟨(), h.2.1, h.2.2⟩
  rise _ _ := .refl _
  s_step_back _ h := ⟨(), rfl, h.1, h.2⟩

theorem extended : Extended X where
  silent_keeps_inp _ := rfl
  rise_keeps_s {_ _ r} := nomatch r

theorem stable : Stable X obs B rep where
  b1 _ _ _ := ⟨rfl, rfl⟩
  b2 _ _ _ hx := absurd rfl hx.1
  b3 {_ _ r} := nomatch r
  rep_block _ := rfl
  rep_reach _ := Reach.init rfl

def LX : List (Bool × Unit) := [(false, ()), (true, ())]

theorem mem_LX (x : Bool × Unit) : x ∈ LX := by
  obtain ⟨b, ⟨⟩⟩ := x
  cases b <;> simp [LX]

/-- `S`'s one cell and its write graph, `false → true`. -/
def cell : Unit → Bool → Bool := fun _ s => s
def E : Unit → Bool → Bool → Prop := fun _ a b => a = false ∧ b = true

theorem acyclic : ∀ k, Acyclic (E k) := fun _ =>
  acyclic_of_rank Bool.toNat fun a b h _ => by rw [h.1, h.2]; decide

theorem covers : ∀ k, SilentCovers X (cell k) (E k) := fun _ _ _ _ ⟨(), h⟩ => .inr h

/-- No block holds a silent cycle. -/
theorem no_cycle : ¬ SilentCycle X B () :=
  no_silentCycle extended cell E (fun _ _ h => h ()) covers acyclic B ()

theorem run_ok : ReducedRunOk R inst (fun _ => False) (fun q => trig (obsR obs rep q))
    (fun q => goal (obsR obs rep q)) := by
  intro ρ _ _ hf _ ⟨i, _, hg⟩
  have hdown : ∀ j, i ≤ j → (ρ j).1 = false := fun j hj => by
    have := hg j hj
    simp only [goal, obsR] at this
    cases h : (ρ j).1
    · rfl
    · exact absurd h this
  have hen : EvAlways fun n => R.Enabled (onT inst) () (ρ n) := by
    refine ⟨i, fun m hm => ⟨(true, ()), ?_, .inl (), trivial, ?_, rep (ρ m).2, .refl _, rfl, rfl⟩⟩
    · intro he
      have := congrArg Prod.fst he
      rw [hdown m hm] at this
      exact Bool.noConfusion this
    · exact ⟨hdown m hm, rfl, rfl⟩
  obtain ⟨m, hm, _, l, hl, hs⟩ := hf () hen i
  cases l with
  | inr _ => exact hl.elim
  | inl _ =>
    have h1 : (ρ (m + 1)).1 = true := hs.1.2.1
    rw [hdown (m + 1) (by omega)] at h1
    exact Bool.noConfusion h1

/-- Theorem A15 with no class marked: every behaviour of `C` under `Fair_P ∧ WF_vars(Next)`
sets the flag. -/
theorem eventually_flag :
    C.LiveHolds (fun σ => C.FairP (onT inst) σ ∧ C.WFNext σ)
      (fun p => trig (obsC obs p)) (fun p => goal (obsC obs p)) :=
  a15_monotone split lemma6 extended stable LX (fun x _ => mem_LX x) LX (fun q _ => mem_LX q)
    cell E (fun _ _ h => h ()) covers acyclic inst trig goal run_ok

end RdraIsh.ExampleMono

namespace RdraIsh.ExampleMonoSys

open Mono

inductive V where
  | ex
  | level
  deriving DecidableEq

def create : SAct V Unit := ⟨fun _ => (), [.create 0, .assign .level 0 (.lit 0)]⟩
def raise : SAct V Unit := ⟨fun _ => (), [.transition .level 0 [(0, 1), (1, 2)]]⟩

def sys : Sys V Unit where
  acts := [create, raise]
  ex := fun _ => .ex
  columns := fun _ => [.level]
  owners := fun _ => []
  base := fun _ => 0
  size := fun _ => 3
  slots := fun _ => 1
  ofSlot := fun _ c => c
  counted := fun _ => False
  now := none
  intMax := 0

theorem not_cyclic (v : V) : ¬ sys.Cyclic v := by
  rintro (h | ⟨a, ha, p, hd, _⟩)
  · exact h
  · simp only [sys, List.mem_cons, List.not_mem_nil, or_false] at ha
    rcases ha with rfl | rfl <;> simp [create, raise] at hd

/-- Every edge of `level` that changes it raises it. -/
theorem level_rises {f t : Nat} (h : sys.Edge .level f t) (hne : f ≠ t) : f < t := by
  obtain ⟨a, ha, e, he, hx⟩ := h
  simp only [sys, List.mem_cons, List.not_mem_nil, or_false] at ha
  rcases ha with rfl | rfl
  · simp only [create, List.mem_cons, List.not_mem_nil, or_false] at he
    rcases he with rfl | rfl
    · exact absurd hx.1 (by decide)
    · obtain ⟨_, hfrom, rfl⟩ := hx
      have : f = 0 := hfrom.1 (List.mem_cons_self ..)
      exact absurd this hne
  · simp only [raise, List.mem_cons, List.not_mem_nil, or_false] at he
    subst he
    obtain ⟨_, hm⟩ := hx
    simp only [List.mem_cons, Prod.mk.injEq, List.not_mem_nil, or_false] at hm
    rcases hm with ⟨rfl, rfl⟩ | ⟨rfl, rfl⟩ <;> decide

theorem level_monotone : sys.Monotone .level :=
  ⟨not_cyclic _, acyclic_of_rank id fun _ _ h hne => level_rises h hne⟩

theorem ex_monotone : sys.Monotone .ex := by
  refine sys.existence_monotone (not_cyclic _) fun a ha e he f t hx => ?_
  simp only [sys, List.mem_cons, List.not_mem_nil, or_false] at ha
  rcases ha with rfl | rfl
  · simp only [create, List.mem_cons, List.not_mem_nil, or_false] at he
    rcases he with rfl | rfl
    · exact .inl ⟨hx.2.1, hx.2.2⟩
    · exact absurd hx.1 (by decide)
  · simp only [raise, List.mem_cons, List.not_mem_nil, or_false] at he
    subst he
    exact absurd hx.1 (by decide)

/-- The empty state: no row, `level` at base. -/
def σ₀ : St V := fun _ _ => 0
/-- After the create: the row present, `level` at `0`. -/
def σ₁ : St V := setv (setv σ₀ .ex 0 1) .level 0 0
/-- After one raise: `level` at `1`. -/
def σ₂ : St V := setv σ₁ .level 0 1

def b₀ : Nat → Nat := fun _ => 0

theorem create_pre : sys.Pre create b₀ σ₀ where
  created_absent _ _ := rfl
  deleted_present p h := by simp [create] at h
  created_base _ _ _ _ := rfl
  in_domain v _ _ := by cases v <;> exact Nat.zero_lt_succ _
  slot_lt _ _ _ h := by simp [create] at h
  tick_below h := by simp [create] at h

theorem create_step : sys.Succ create b₀ σ₀ σ₁ := .nil

theorem raise_pre : sys.Pre raise b₀ σ₁ where
  created_absent p h := by simp [raise] at h
  deleted_present p h := by simp [raise] at h
  created_base _ p h := by simp [raise] at h
  in_domain _ _ h := by simp [Sys.WritesAt, raise] at h
  slot_lt _ _ _ h := by simp [raise] at h
  tick_below h := by simp [raise] at h

theorem raise_step : sys.Succ raise b₀ σ₁ σ₂ := .nil

/-- `succ_covers` applies to both steps: `Pre` and `Succ` are jointly satisfiable. -/
theorem steps_covered :
    (∀ v k, ¬ sys.Cyclic v → StepOrStay (sys.Edge v) (σ₀ v k) (σ₁ v k)) ∧
      ∀ v k, ¬ sys.Cyclic v → StepOrStay (sys.Edge v) (σ₁ v k) (σ₂ v k) :=
  ⟨sys.succ_covers (by simp [sys]) create_pre create_step,
   sys.succ_covers (by simp [sys]) raise_pre raise_step⟩

end RdraIsh.ExampleMonoSys
