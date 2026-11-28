import RdraIsh.SymActions

/-!
# Commutation fails outside `W` (cones.md, Theorem A18)

Shops and orders, two slots each, both classes moving. An order has an optional owner, a
shop, whose base is `null`; the one action creates an order and does not write its owner,
which (G4) allows. Every hypothesis of `step_comm` holds (`wf`, `built`, `hyp`). The state
`s` has both shops present and order slot 0 absent with its owner on shop 0, so it is not in
`W`. The swap of the shops lies in `G_s`, and the step that creates order 0 from `s` has no
image step: the step from `g(s)` keeps owner `shop 0` on the new order, `g(t)` names shop 1.
-/

namespace RdraIsh.ExampleOffW

open SymAct

/-- Classes and entities: `false` the shops, `true` the orders. -/
def S : Sys Bool Bool Empty Unit Unit where
  n _ := 2
  moves _ := true
  cls e := e
  pre e := !e
  cent c := nomatch c
  cbase c := nomatch c
  creset c := nomatch c
  cany c := nomatch c
  cdom c := nomatch c
  child _ := true
  par _ := false
  opt _ := true
  obase _ := none
  ostart _ := .base
  params _ := [⟨true, .new⟩]
  guards _ := []
  effs _ := [.create 0]

/-- The swap of slots 0 and 1. -/
def sw (k : Nat) : Nat := if k = 0 then 1 else if k = 1 then 0 else k

theorem sw_sw (k : Nat) : sw (sw k) = k := by
  unfold sw; split <;> split <;> (try split) <;> omega

/-- The swap of the shops. -/
def g : Perm S.n where
  f κ k := if κ then k else sw k
  finv κ k := if κ then k else sw k
  left κ k := by cases κ <;> simp [sw_sw]
  right κ k := by cases κ <;> simp [sw_sw]
  fix κ k hk := by
    cases κ
    · show sw k = k
      have : (2 : Nat) ≤ k := hk
      unfold sw; split <;> (try split) <;> omega
    · rfl

/-- Both shops present, order slot 0 absent with its owner on shop 0. -/
def s : St Bool Empty Unit where
  ex e k := if !e && k < 2 then .present else .absent
  col c _ := nomatch c
  own _ _ := some 0
  now := 0

/-- The binding of order 0. -/
def b : Act.Binding := ⟨fun _ => 0, fun _ => none⟩

theorem wf : S.WF where
  guards _ φ h := by simp [S] at h
  effs _ x h := by
    simp only [S, List.mem_singleton] at h
    subst h
    simp [Sys.ewf, pent, S]

theorem built : S.Built where
  new_create a p q hq _ := by
    have : p = 0 := by
      cases p with
      | zero => rfl
      | succ p => simp [S] at hq
    subst this; simp [S]
  earlier_mark a p q _ hq hr := by
    cases p with
    | zero =>
      simp only [S, List.getElem?_cons_zero, Option.some.injEq] at hq
      subst hq; cases hr
    | succ p => simp [S] at hq

theorem hyp : S.Hyp where
  g1 _ φ h := by simp [S] at h
  g2 _ x h := by
    simp only [S, List.mem_singleton] at h
    subst h; trivial
  g3 := by
    unfold Sys.G3
    intro o
    refine ⟨fun h => ?_, fun _ h => ?_⟩ <;> simp [S] at h
  g4 _ _ _ _ _ := .inl ⟨rfl, rfl⟩
  g6 _ φ h := by simp [S] at h

theorem not_W : ¬ S.BaseAbsent s := by
  intro h
  have := h.2 () 0 rfl
  cases this

theorem mem : S.mem s g := by
  unfold Sys.mem
  refine And.intro (fun κ _ h => by simp [S] at h) (fun e k h => ?_)
  cases e
  · show sw k = k
    have : ¬ k < 2 := by
      intro hk
      have : s.ex false k = .present := by simp [s, hk]
      rw [h] at this; cases this
    unfold sw; split <;> (try split) <;> omega
  · rfl

theorem step : S.lts.step s ((), b) (S.post () b s) := by
  refine ⟨fun p q hq => ?_, fun i x hx => ?_, fun φ h => by simp [S] at h, rfl⟩
  · cases p with
    | zero =>
      simp only [S, List.getElem?_cons_zero, Option.some.injEq] at hq
      subst hq
      refine ⟨by decide, rfl, fun j hj => absurd hj (Nat.not_lt_zero j)⟩
    | succ p => simp [S] at hq
  · cases i with
    | zero =>
      simp only [S, List.getElem?_cons_zero, Option.some.injEq] at hx
      subst hx; trivial
    | succ i => simp [S] at hx

/-- **Commutation fails outside `W`**: under every hypothesis of `step_comm`, a step from a
state outside `W` has no image under a `g ∈ G_s`. -/
theorem off_W_counterexample :
    S.WF ∧ S.Built ∧ S.Hyp ∧ ¬ S.BaseAbsent s ∧ S.mem s g ∧
      S.lts.step s ((), b) (S.post () b s) ∧
      ¬ S.lts.step (S.act g s) (S.actL g ((), b)) (S.act g (S.post () b s)) := by
  refine ⟨wf, built, hyp, not_W, mem, step, ?_⟩
  intro h
  have := congrArg (fun st => st.own () 0) h.2.2.2
  revert this
  decide

end RdraIsh.ExampleOffW
