import RdraIsh.Lemma6Syntax

/-!
# Lemma 6's hypotheses from the syntax, on an instance (cones.md, Lemma 6)

Desks, shops and orders, one slot each. An order has an owner, its shop. `T_v` holds the
existences of desks and shops and the shops' flag `tf`; `S_v` the orders, their status `st`,
the owner and the clock. The desks' existence is an input and the shops are a stub: their
existence is in `T_v`, not an input, and the parent of the owner, which `S_v` holds.

- `note` (`S_a`) binds an `Existing` order and an `Existing` shop, needs the shop's `tf = 1`
  and sets the order's `st`. `fold_of` folds it: the shop is its one `T` parameter, the guard
  reads `T_v` alone, no effect names the shop, so the key is the order (`note_key`).
- `mark` (`S_a`) binds an `Existing` order and needs some desk to exist. It has no `T`
  parameter, so it does not fold, and its read of the desks' existence makes that an input.
- `open_` (`T_a`, a rise) creates a desk and binds a `Parent` shop, a parameter on the stub,
  which `X` rebinds to `Earlier { step: 0 }` (`open_shop`).
- `setf` (`T_a`) sets a shop's `tf`.

`hyp` is `Lemma6Hyp` of the data `ext` reads off the syntax, from `Built`, Lemma 5 (a)(b),
the code's inputs and `exact_holdsB` alone; `lemma6` is Lemma 6 with the code's stages.
-/

namespace RdraIsh.ExampleLemma6Syntax

open SymAct

inductive Ent | desk | shop | order
  deriving DecidableEq

inductive Col | tf | st
  deriving DecidableEq

inductive Act' | note | mark | open_ | setf
  deriving DecidableEq

def S : Sys Ent Ent Col Unit Act' where
  n _ := 1
  moves _ := false
  cls e := e
  pre
    | .desk => false
    | _ => true
  cent
    | .tf => .shop
    | .st => .order
  cbase _ := 0
  creset _ := true
  cany _ := false
  cdom _ := 3
  child _ := .order
  par _ := .shop
  opt _ := true
  obase _ := none
  ostart _ := .base
  params
    | .note => [⟨.order, .existing⟩, ⟨.shop, .existing⟩]
    | .mark => [⟨.order, .existing⟩]
    | .open_ => [⟨.desk, .new⟩, ⟨.shop, .parent⟩]
    | .setf => [⟨.shop, .existing⟩]
  guards
    | .note => [.cmp (.col .tf (.param 1)) .eq (.lit 1)]
    | .mark => [.any .desk .tt]
    | _ => []
  effs
    | .note => [.setCol .st 0 1]
    | .mark => [.setCol .st 0 2]
    | .open_ => [.create 0]
    | .setf => [.setCol .tf 0 1]

def LE : List Ent := [.desk, .shop, .order]
def LC : List Col := [.tf, .st]
def LO : List Unit := [()]
def LA : List Act' := [.note, .mark, .open_, .setf]

theorem hE : ∀ e, e ∈ LE := by intro e; cases e <;> simp [LE]
theorem hC : ∀ c, c ∈ LC := by intro c; cases c <;> simp [LC]
theorem hO : ∀ o, o ∈ LO := fun _ => List.mem_singleton_self _
theorem hA : ∀ a, a ∈ LA := by intro a; cases a <;> simp [LA]

/-- The split: `T_v` the desks, the shops and `tf`; the desks' existence the one input. -/
def tv : Var Ent Col Unit → Bool
  | .ex .desk => true
  | .ex .shop => true
  | .col .tf => true
  | _ => false

def inputs : Var Ent Col Unit → Bool
  | .ex .desk => true
  | _ => false

def X : Cut Ent Col Unit Act' where
  tv := tv
  ta
    | .open_ => true
    | .setf => true
    | _ => false
  inputs := inputs
  inputs_t v h := by
    cases v <;> (try (rename_i e; cases e)) <;> first | rfl | simp [inputs] at h
  sel _ := true

abbrev M := S.model LE LC LO LA hE hC hO hA

/-- The data `Extended::build` starts from, read off the syntax, with the bits of
`holdsB`. -/
noncomputable abbrev D :=
  S.ext LE LC LO LA hE hC hO hA X (Sys.keysB LA 1) (Sys.keysB_all hA 1) (Sys.bitKeyB 1)
    (S.holdsB X LC LO 1 (fun _ => .now 0)) (fun _ => .now 0)

def obs : Var Ent Col Unit → Prop := fun _ => False

theorem built : S.Built where
  new_create a p q hq hr := by
    cases a <;> rcases p with _ | _ | p <;> simp [S] at hq <;> subst hq <;> simp_all [S]
  earlier_mark a p q st hq hr := by
    cases a <;> rcases p with _ | _ | p <;> simp [S] at hq <;> subst hq <;> cases hr

theorem lemma5 : Act.Lemma5ab M X.tv X.ta obs where
  a a ha := by
    cases a <;> simp [Act.Sa, X] at ha <;> decide
  b_wr a ha := by
    cases a <;> simp [X] at ha <;> decide
  b_rd a ha v hv hs := by
    have : ∀ v ∈ M.rd a, X.tv v = true := by cases a <;> simp [X] at ha <;> decide
    rw [this v hv] at hs
    cases hs
  obs_s _ h := h.elim

/-- `note` folds, keyed by its order. -/
theorem note_key : S.keyOf X LC LO .note = some 0 := by decide

/-- `mark` does not fold. -/
theorem mark_unfolded : S.keyOf X LC LO .mark = none := by decide

theorem inputs_ok : S.Inputs X LC LO (S.wr LC LO) where
  col_side c h := by cases c <;> simp_all [X, tv, S]
  inputs_col c h := by cases c <;> simp [X, inputs] at h
  raw a h1 h2 := by
    cases a <;> simp [X] at h1 <;> decide

/-- **`Lemma6Hyp` from the syntax**, with nothing about the folds, the kept conjuncts or the
effects assumed. -/
theorem hyp : Act.Lemma6Hyp D obs :=
  Sys.lemma6Hyp built lemma5 inputs_ok (Sys.exact_holdsB fun _ => Nat.le_refl 1)

/-- **Lemma 6 with the code's stages** for this cone and the `X` read off its syntax. -/
theorem lemma6 :
    Lemma6Staged (Act.coneW M X.tv X.ta) D.ext D.inp D.stages :=
  Sys.lemma6_of_syntax built lemma5 inputs_ok (Sys.exact_holdsB fun _ => Nat.le_refl 1)

/-- The instance has a fold, a rise and a stub parameter: `note` is folded, `open_` is a rise,
and its `Parent` shop is on the stub, which `X` rebinds to `Earlier { step: 0 }`. -/
theorem open_shop :
    (D.fold .note).isSome = true ∧ D.riseAct .open_ ∧ D.side .shop = .stub ∧
      D.xrole .open_ 1 ⟨.shop, .parent⟩ = some (.earlier 0) := by
  have hn : D.fold .note = some _ := Sys.fold_some note_key
  have ho : D.fold .open_ = none := Sys.fold_none (by decide)
  have hs : D.side .shop = .stub := by decide
  refine ⟨by rw [hn]; rfl, ⟨rfl, .ex .desk, by decide, rfl⟩, hs, ?_⟩
  unfold Act.Ext.xrole Act.Ext.isKey
  rw [hs, ho]
  rfl

end RdraIsh.ExampleLemma6Syntax
