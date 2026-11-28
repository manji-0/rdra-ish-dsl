import RdraIsh.Reduction
import RdraIsh.Symmetry

/-!
# A concrete instance of Theorem A8's hypotheses

A guard against vacuity: a cone whose every hypothesis (Lemma 5 (a)(b), Lemma 6, the shape of
`X`, (B1)–(B3)) is discharged, so `a8_holds_iff` applies to something.

`T = Bool` is a flag and `S = Bool` a sub-lifecycle variable. The `S_a` action sets `s` from
`false` to `true`; the `T_a` action sets the flag once `s` is `true`, reading `s` through
`obs = id`. There are no inputs (`I = Unit`) and so no rises. `X` is the `S` half, and the
partition is the identity on `S`.
-/

namespace RdraIsh.Example

open LTS

def C : LTS (Bool × Bool) (Unit ⊕ Unit) where
  init p := p.1 = false ∧ p.2 = false
  step p l q := match l with
    | .inr () => q.1 = p.1 ∧ p.2 = false ∧ q.2 = true
    | .inl () => p.1 = false ∧ p.2 = true ∧ q.1 = true ∧ q.2 = p.2

def X : LTS (Bool × Unit) (Unit ⊕ Empty) where
  init x := x.1 = false
  step x l y := match l with
    | .inl () => x.1 = false ∧ y.1 = true
    | .inr r => nomatch r

def obs : Bool → Bool := id
def inp : Bool → Unit := fun _ => ()
def B : Bool × Unit → Bool := fun x => x.1
def rep : Bool → Bool × Unit := fun b => (b, ())

theorem split : Split C obs where
  s_keeps_t h := h.1
  t_keeps_s h := h.2.2.2
  t_reads_obs ho h := by
    simp only [obs, id] at ho
    subst ho
    exact h

theorem lemma6 : Lemma6 C X inp where
  init h := h.2
  s_step _ h := ⟨(), h.2.1, h.2.2⟩
  rise _ _ := .refl _
  s_step_back _ h := ⟨(), rfl, h.1, h.2⟩

theorem extended : Extended X where
  silent_keeps_inp _ := rfl
  rise_keeps_s {_ _ r} := nomatch r

theorem pair_eq {x y : Bool × Unit} (h : B x = B y) : x = y := by
  obtain ⟨x1, ⟨⟩⟩ := x
  obtain ⟨y1, ⟨⟩⟩ := y
  simp only [B] at h
  rw [h]

theorem stable : Stable X obs B rep where
  b1 _ _ h := ⟨h, rfl⟩
  b2 _ _ h hx := by
    rw [← pair_eq h]
    exact hx
  b3 {_ _ r} := nomatch r
  rep_block _ := rfl
  rep_reach {x} hx := by
    obtain ⟨x1, ⟨⟩⟩ := x
    exact hx

/-- Theorem A8, consequence 1, on this cone: the invariant "the flag is set only once `s` is"
holds on `C` iff it holds on `R`. -/
theorem flag_after_s :
    C.Holds (fun p => (obsC obs p).1 = true → (obsC obs p).2 = true) ↔
      (reduced C X inp B rep).Holds (fun q => (obsR obs rep q).1 = true →
        (obsR obs rep q).2 = true) :=
  a8_holds_iff split lemma6 extended stable (fun o => o.1 = true → o.2 = true)

end RdraIsh.Example

/-!
# A concrete instance of `Sym`

Two slots, each created once (`false → true`); the group is `{id, swap}` (`g = true` swaps),
allowed on every state. The invariant "not both slots are created" is symmetric (and fails on
both sides); "slot 0 is created whenever slot 1 is" is not symmetric, and the theorem says
nothing of it.
-/

namespace RdraIsh.ExampleSym

open LTS

def M : LTS (Bool × Bool) Bool where
  init s := s = (false, false)
  step s a t := (a = false ∧ s.1 = false ∧ t = (true, s.2)) ∨
    (a = true ∧ s.2 = false ∧ t = (s.1, true))

def swap (g : Bool) (s : Bool × Bool) : Bool × Bool := if g then (s.2, s.1) else s

def sym : Sym M Bool where
  mul g h := xor g h
  inv g := g
  act := swap
  actL g a := xor g a
  mem _ _ := True
  act_mul g h s := by cases g <;> cases h <;> rfl
  act_inv g s := by cases g <;> rfl
  act_inv' g s := by cases g <;> rfl
  mul_mem _ _ := trivial
  inv_mem _ := trivial
  mem_act _ := Iff.rfl
  step {s a t g} hst _ := by
    obtain ⟨s1, s2⟩ := s
    cases g <;> cases a <;> simp_all [M, swap]
  mem_step _ _ := trivial
  init {s g} hi _ := by
    simp only [M] at hi ⊢
    subst hi
    cases g <;> rfl

/-- The quotient by "store the state with its slots sorted" gives the plain verdict of the
symmetric invariant. -/
theorem sorted_quotient :
    M.Holds (fun s => s ≠ (true, true)) ↔
      (quotient M (fun s => swap (s.2 && !s.1) s)).Holds (fun s => s ≠ (true, true)) :=
  quotient_holds_iff sym _ (fun s => s.2 && !s.1) (fun _ => rfl) (fun _ => trivial) _
    (fun s g _ => by obtain ⟨s1, s2⟩ := s; cases g <;> cases s1 <;> cases s2 <;> decide)

end RdraIsh.ExampleSym
