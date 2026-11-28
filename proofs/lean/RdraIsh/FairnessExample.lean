import RdraIsh.Fairness

/-!
# A concrete instance of Theorem A15's hypotheses, with a silent cycle

The shape of the corpus case `cone_reduced_loop_fair_enabled`. `T = Bool` is a flag and
`S = Bool` a sub-lifecycle variable. The `S_a` action toggles `s` forever; the fair `T_a`
action sets the flag, reading nothing of `S` (`obs` is constant). There are no inputs and no
rises. Both states of `X` fall in one block, so the toggle is a silent cycle inside it and the
block can loop (`silent_cycle`): under `StopRule::Looping` the reduced state with the flag
down may stop unless a fair instance is enabled there, and one is.

`C` has a behaviour that toggles forever and never sets the flag. It satisfies
`WF_vars(Next)` but not `Fair_P`. `eventually_flag` derives from `a15` that every behaviour
of `C` under `Fair_P ∧ WF_vars(Next)` sets the flag, from the run's verdict on the two-state
`R` (`run_ok`).
-/

namespace RdraIsh.ExampleFair

open LTS

def C : LTS (Bool × Bool) (Unit ⊕ Unit) where
  init p := p.1 = false ∧ p.2 = false
  step p l q := match l with
    | .inl () => p.1 = false ∧ q.1 = true ∧ q.2 = p.2
    | .inr () => q.1 = p.1 ∧ q.2 = !p.2

def X : LTS (Bool × Unit) (Unit ⊕ Empty) where
  init x := x.1 = false
  step x l y := match l with
    | .inl () => y.1 = !x.1
    | .inr r => nomatch r

def obs : Bool → Unit := fun _ => ()
def inp : Bool → Unit := fun _ => ()
def B : Bool × Unit → Unit := fun _ => ()
def rep : Unit → Bool × Unit := fun _ => (false, ())

/-- The one fair instance, "set the flag", and its one label. -/
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
  s_step _ h := ⟨(), h.2⟩
  rise _ _ := .refl _
  s_step_back _ h := ⟨(), rfl, h⟩

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

/-- The toggle is a silent cycle inside the one block. -/
theorem silent_cycle : SilentCycle X B () :=
  ⟨(false, ()), (true, ()), Reach.init rfl, rfl, rfl, by decide, ⟨(), rfl⟩,
    .cons (y := (false, ())) rfl rfl (by decide) ⟨(), rfl⟩ (.refl _)⟩

/-- `can_loop` read as Definition 5, case 1. -/
def canLoop : Unit → Prop := SilentCycle X B

/-- The run on `R` finds no counterexample: a fair lasso of `R` that keeps the flag down from
some point on keeps "set the flag" enabled and so must take it. -/
theorem run_ok : ReducedRunOk R inst canLoop (fun q => trig (obsR obs rep q))
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

/-- Theorem A15 on this cone: every behaviour of `C` under `Fair_P ∧ WF_vars(Next)` sets the
flag, though `C` can toggle `s` forever. -/
theorem eventually_flag :
    C.LiveHolds (fun σ => C.FairP (onT inst) σ ∧ C.WFNext σ)
      (fun p => trig (obsC obs p)) (fun p => goal (obsC obs p)) :=
  a15 split lemma6 extended stable LX (fun x _ => mem_LX x) LX (fun q _ => mem_LX q)
    canLoop (fun _ h => h) inst trig goal run_ok

end RdraIsh.ExampleFair
