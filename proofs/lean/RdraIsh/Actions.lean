import RdraIsh.LTS

/-!
# An action-level model of a cone system (cones.md, Definition 3's setting)

The cone system `C` of `TransitionSystem::cone_system`, one level below the abstract
transition systems of `Reduction.lean`:

- a finite set of variables `V` (`vars` lists every one). A variable belongs to an entity
  whose existence variable is `own v` (`own_existence`; `none` for an existence or the
  clock), and `existences v` lists the existence variables `v` means nothing without (its
  entity's, and for an owner variable its parent's too);
- a state is a valuation `V → Val`. A variable here is one of the code's `VarId`s, so `Val`
  holds the values of every slot of it;
- a finite set of actions `A` (`acts`), each with its parameters (an entity and a role per
  parameter), a list of guard conjuncts, a read set `rd`, a write set `wr`, and an effect.
  A step binds every parameter to a slot and picks a value for every choice (`Binding`);
- the step relation: `σ —(a, b)→ eff a b σ` when every guard conjunct of `a` holds of `σ`
  under `b`.

`rd a` is the code's read set of `split_with`: `action_reads` (the plain reads and the mark
reads) together with `copy_reads`. `wr a` is `action_writes`. This file does not compute them
from a syntax; that they bound what the guard and the effect look at is the hypothesis `Local`
below. `SymActModel.lean` reads a model off the slot syntax of `SymActions.lean`, with `rd`
and `wr` computed from the guards and effects as the code computes them, and proves `Local`
of it (`SymAct.Sys.local_`).

The parameters and roles are carried for Lemma 6 (`restrict_to` drops a parameter with its
guards; `bind_stub_params` re-binds one on a stub). Lemma 5 does not look at them.
-/

namespace RdraIsh.Act

open LTS

/-- The role of a parameter (`ParamRole`): which rows a slot may be bound to. `existing`,
`parent` and `triggered` need a present row, `new` an absent one, `earlier` a row an earlier
step of the run created. -/
inductive Role where
  | existing
  | parent
  | triggered
  | new
  | earlier (step : Nat)
  deriving DecidableEq

/-- The roles whose binding needs a present row (`binds_exists() == PRESENT`). -/
def Role.needsPresent : Role → Bool
  | .existing | .parent | .triggered => true
  | _ => false

/-- The roles whose binding reads the entity's existence (all but `earlier`). -/
def Role.readsRow : Role → Bool
  | .earlier _ => false
  | _ => true

theorem Role.readsRow_of_needsPresent {r : Role} (h : r.needsPresent = true) :
    r.readsRow = true := by
  cases r <;> simp_all [Role.needsPresent, Role.readsRow]

section Rows

variable {Val : Type}

/-- How an existence value holds its rows: which slots are present, and which slot is the
lowest absent one. The semantics of `Role` is `Rows.cand`: Lemma 6's model reads it through
an `Rows` of its valuations, and the slot model of `SymActions.lean` through `SymAct.rows`. -/
structure Rows (Val : Type) where
  present : Val → Nat → Prop
  lowestAbsent : Val → Nat → Prop

/-- `candidate_slots`: the slots a parameter of a role may bind, given its entity's existence. -/
def Rows.cand (R : Rows Val) : Role → Val → Nat → Prop
  | .existing, v, c => R.present v c
  | .parent, v, c => R.present v c
  | .triggered, v, c => R.present v c
  | .new, v, c => R.lowestAbsent v c
  | .earlier _, _, _ => True

theorem Rows.cand_present (R : Rows Val) {r : Role} (h : r.needsPresent = true) (v : Val)
    (c : Nat) : R.cand r v c ↔ R.present v c := by
  cases r <;> simp_all [Role.needsPresent, Rows.cand]

end Rows

/-- A parameter: its entity and its role. -/
structure Param (E : Type) where
  entity : E
  role : Role

/-- A binding of a step: the slot of each parameter, by position, and the value each choice
effect picks, by position (`none` is `null`, which an owner choice may pick). Which bindings a
step may take is part of the guard. -/
structure Binding where
  slot : Nat → Nat
  choice : Nat → Option Nat

/-- A guard conjunct: the variables it reads and the condition. -/
structure Conj (V Val : Type) where
  rd : List V
  holds : Binding → (V → Val) → Prop

/-- A cone system at the level of its actions. -/
structure Model (V E A Val : Type) where
  vars : List V
  acts : List A
  vars_all : ∀ v, v ∈ vars
  acts_all : ∀ a, a ∈ acts
  /-- `own_existence`: the existence variable of a variable's entity. -/
  own : V → Option V
  /-- `existences`: the existence variables a variable means nothing without. -/
  existences : V → List V
  /-- `is_counted`: the counter of a counted edge. -/
  counted : V → Bool
  params : A → List (Param E)
  guards : A → List (Conj V Val)
  rd : A → List V
  wr : A → List V
  eff : A → Binding → (V → Val) → (V → Val)
  init : (V → Val) → Prop

variable {V E A Val : Type}

namespace Model

variable (M : Model V E A Val)

/-- The guard of a step: every conjunct holds. -/
def guard (a : A) (b : Binding) (σ : V → Val) : Prop := ∀ g ∈ M.guards a, g.holds b σ

/-- The cone system's step relation over valuations, labelled by the action and its
binding. -/
def lts : LTS (V → Val) (A × Binding) where
  init := M.init
  step σ l τ := M.guard l.1 l.2 σ ∧ τ = M.eff l.1 l.2 σ

end Model

/-- Two valuations agree on a list of variables. -/
def Agree (l : List V) (σ τ : V → Val) : Prop := ∀ v ∈ l, σ v = τ v

/-- Locality of the code's read and write sets, the hypothesis Lemma 5 turns into the
`Split` of `Reduction.lean`:

- every guard conjunct reads only variables of `rd a ++ wr a`, and its value depends only on
  the variables it reads;
- the effect leaves every variable outside `wr a` unchanged;
- the effect's value on `wr a` depends only on `rd a ++ wr a`.

The footprint is `rd ∪ wr`, not `rd`, because the code does not count two things as reads:
taking a `new` slot looks at the existence it then writes (`action_reads`), and a write to
one slot of a variable keeps the other slots, so the variable's new value depends on its old
one. The weaker form is all Lemma 5 needs, since an action's writes lie on its own side. -/
structure Local (M : Model V E A Val) : Prop where
  conj_rd : ∀ {a g}, g ∈ M.guards a → ∀ v ∈ g.rd, v ∈ M.rd a ++ M.wr a
  conj_local : ∀ {a g}, g ∈ M.guards a → ∀ b σ τ, Agree g.rd σ τ → g.holds b σ → g.holds b τ
  eff_frame : ∀ a b σ v, v ∉ M.wr a → M.eff a b σ v = σ v
  eff_local : ∀ a b σ τ, Agree (M.rd a ++ M.wr a) σ τ → ∀ v ∈ M.wr a,
    M.eff a b σ v = M.eff a b τ v

variable {M : Model V E A Val}

/-- The guard depends only on `rd a ++ wr a`. -/
theorem Local.guard_local (hL : Local M) {a b σ τ} (h : Agree (M.rd a ++ M.wr a) σ τ)
    (hg : M.guard a b σ) : M.guard a b τ := by
  intro g hgm
  exact hL.conj_local hgm b σ τ (fun v hv => h v (hL.conj_rd hgm v hv)) (hg g hgm)

/-- An action that writes nothing stutters. -/
theorem Local.idle (hL : Local M) {a b σ τ} (hw : M.wr a = []) (h : M.lts.step σ (a, b) τ) :
    τ = σ := by
  funext v
  rw [h.2]
  exact hL.eff_frame a b σ v (by rw [hw]; exact List.not_mem_nil)

end RdraIsh.Act
