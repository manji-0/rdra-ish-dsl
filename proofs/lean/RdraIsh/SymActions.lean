import RdraIsh.SymmetryLiveness
import RdraIsh.Actions

/-!
# Theorem A18 from (G1)–(G6) (cones.md, "Instance symmetry")

A slot-level model of a cone system, its group, and the hypotheses (G1)–(G6) as conditions
on the syntax of its actions, from which the commutation of Theorem A18 is derived:

- entities `E` in classes `K` (`cls`; a class with `Start::Identity` owners couples a child
  and its parent, `Group::of`); every entity of a class has the class's `n κ` slots, and the
  scan leaves a class out (`moves κ = false`);
- a state gives each slot an existence (`Ex`), each data column `C` and each owner `O` a
  value per slot, and the clock. An owner's value is `some k`, the parent's slot `k`, or
  `none` (`null`); the code's optional encoding `k + 1` is not modelled;
- a permutation `g` permutes the slots of each class and fixes the slots past `n κ`; `G_s`
  (`mem`) is the permutations that fix every slot absent in `s` and every slot of a class
  that does not move. `act g s` is `Group::apply`: a slot's columns, existence and owners
  move with the slot, and an owner on a row that is not absent is relabelled by its
  parent's permutation (`Group::relabel`);
- terms, predicates and effects follow `lifecycle/types.rs` (`Term`, `Pred`, `Effect`):
  a term reads a column or an owner at a parameter (`At::Param`), at a parameter's slot in
  the successor (`At::Next`) or at a quantifier's slot (`At::Bound`), names a row
  (`Term::Index`), or is a literal, `null` or the clock; quantifiers range over the present
  slots of the pre-state (`expr.rs`). Effects are read pointwise: a location takes the value
  of the last effect in the list that writes it, and its old value when none does;
- a step binds each parameter by its role (`candidate_slots`): `Existing`, `Parent` and
  `Triggered` a present slot, `New` the lowest absent slot, `Earlier` any slot; every choice
  effect picks a value (`Binding.choice`, by the effect's position). Parameters, roles and
  bindings are those of `Actions.lean`, and a role binds what `Rows.cand` says, through
  `rows` on a slot's existence (`roleOk`).

`W` (`BaseAbsent`) is the set of states in which every absent row holds the base value of
each of its columns and owners. It holds the initial states, is closed under steps and under
every `G_s`, and the commutation is proved on it (`step_comm`), which gives the `SymOn` of
`SymmetryLiveness.lean` (`symOn`). On a state outside `W` the commutation can fail: an absent
row whose optional owner holds a row, created by a step that does not write that owner,
keeps the value in the step from `g(s)` and is relabelled in `g(t)`.

The hypotheses are three structures:

- `WF`: the terms and effects are well typed (a column or owner is read and written on a row
  of its own entity, every parameter named exists).
- `Built`: what the builder gives and the scan does not check. A `New` parameter is created
  by its action (`build/actions.rs`); an `Earlier` parameter carries a run-mark conjunct
  `mark = k` whose base is not `k` (`run_marks`). The `Earlier { step: 0 }` parameters of the
  extended sub-lifecycle (`reduce/extended.rs`) do not satisfy the second; they bind
  entities `H` leaves out.
- `Hyp`: (G1)–(G4) and (G6) as the scan states them (`explore/symmetry/breaks.rs`). (G5) is
  part of the semantics: a delete resets a row's columns that it resets (`creset`, every
  column but the run marks) and keeps its owners.

Simplifications against `each_successor`: the effects are read in list order (the code
applies the deletes after the other effects and the choices after the deletes, and drops a
choice on a deleted row); a `Transition` is a choice among its arms from the pre-state
value; owner `Transition`s and data columns written from a parameter's slot are in the
syntax only as far as (G2) needs them; `Domain` sizes of data columns are not checked.
-/

namespace RdraIsh.SymAct

open LTS
open Act (Role Param Binding Rows)

/-- The existence of a slot (`Value::ABSENT`, `PRESENT`, `DELETED`). -/
inductive Ex where
  | absent
  | present
  | deleted
  deriving DecidableEq

/-- How a variable of a pre-existing row starts (`Start`). -/
inductive Start where
  | base
  | any
  | identity
  deriving DecidableEq

/-- Where a term reads (`At`). -/
inductive Ref where
  | param (p : Nat)
  | next (p : Nat)
  | bound (i : Nat)

/-- A term (`Term`). `index e r` is the slot of the row `r`, a row of `e`. -/
inductive Term (E C O : Type) where
  | col (c : C) (r : Ref)
  | own (o : O) (r : Ref)
  | index (e : E) (r : Ref)
  | lit (v : Nat)
  | null
  | now

/-- A comparison (`CmpOp`); `Gt` and `Ge` are `Lt` and `Le` with the operands swapped. -/
inductive Op where
  | eq
  | ne
  | lt
  | le

/-- A predicate (`Pred`), with binary `And` and `Or`. -/
inductive Pred (E C O : Type) where
  | tt
  | ff
  | cmp (x : Term E C O) (op : Op) (y : Term E C O)
  | inn (x : Term E C O) (ys : List (Term E C O))
  | not (p : Pred E C O)
  | and (p q : Pred E C O)
  | or (p q : Pred E C O)
  | all (e : E) (p : Pred E C O)
  | any (e : E) (p : Pred E C O)
  | count (e : E) (p : Pred E C O) (op : Op) (m : Nat)

/-- An effect (`Effect`), on the row of parameter `p`. `copyCol c p q` writes a data column
from the slot of `q` and `setOwnLit` an owner from a literal; (G2) says when the scan lets
the classes they name move. -/
inductive Eff (C O : Type) where
  | create (p : Nat)
  | delete (p : Nat)
  | setCol (c : C) (p : Nat) (v : Nat)
  | copyCol (c : C) (p q : Nat)
  | setOwn (o : O) (p q : Nat)
  | setOwnLit (o : O) (p : Nat) (v : Option Nat)
  | chooseCol (c : C) (p : Nat) (vs : List Nat)
  | chooseOwn (o : O) (p : Nat) (vs : List (Option Nat))
  | trans (c : C) (p : Nat) (arms : List (Nat × Nat))
  | tick

/-- A cone system at the level of slots. -/
structure Sys (K E C O A : Type) where
  n : K → Nat
  moves : K → Bool
  cls : E → K
  pre : E → Bool
  cent : C → E
  cbase : C → Nat
  creset : C → Bool
  cany : C → Bool
  cdom : C → Nat
  child : O → E
  par : O → E
  opt : O → Bool
  obase : O → Option Nat
  ostart : O → Start
  params : A → List (Param E)
  guards : A → List (Pred E C O)
  effs : A → List (Eff C O)

/-- A state: an existence, a value per column and per owner for every slot, and the clock. -/
structure St (E C O : Type) where
  ex : E → Nat → Ex
  col : C → Nat → Nat
  own : O → Nat → Option Nat
  now : Nat

theorem St.ext' {E C O : Type} {s t : St E C O} (h1 : s.ex = t.ex) (h2 : s.col = t.col)
    (h3 : s.own = t.own) (h4 : s.now = t.now) : s = t := by
  cases s; cases t; cases h1; cases h2; cases h3; cases h4; rfl

variable {K E C O A : Type}

/-- A permutation of the slots of every class that fixes the slots past `n κ`. -/
structure Perm (n : K → Nat) where
  f : K → Nat → Nat
  finv : K → Nat → Nat
  left : ∀ κ k, finv κ (f κ k) = k
  right : ∀ κ k, f κ (finv κ k) = k
  fix : ∀ κ k, n κ ≤ k → f κ k = k

namespace Perm

variable {n : K → Nat} (g : Perm n)

theorem f_inj {κ k k'} (h : g.f κ k = g.f κ k') : k = k' := by
  have := congrArg (g.finv κ) h
  rwa [g.left, g.left] at this

theorem finv_inj {κ k k'} (h : g.finv κ k = g.finv κ k') : k = k' := by
  have := congrArg (g.f κ) h
  rwa [g.right, g.right] at this

theorem f_lt {κ k} (h : k < n κ) : g.f κ k < n κ := by
  apply Nat.lt_of_not_le
  intro hge
  have := g.fix κ _ hge
  have := g.f_inj this
  omega

theorem finv_fix {κ k} (h : n κ ≤ k) : g.finv κ k = k := by
  have := g.left κ k
  rwa [g.fix κ k h] at this

theorem finv_lt {κ k} (h : k < n κ) : g.finv κ k < n κ := by
  apply Nat.lt_of_not_le
  intro hge
  have := g.right κ k
  rw [g.fix κ _ hge] at this
  omega

theorem f_eq_iff {κ k j} : g.f κ k = j ↔ k = g.finv κ j :=
  ⟨fun h => by rw [← h, g.left], fun h => by rw [h, g.right]⟩

theorem finv_of_fix {κ k} (h : g.f κ k = k) : g.finv κ k = k := by
  have := g.left κ k
  rwa [h] at this

theorem finv_lt_iff {κ k} : g.finv κ k < n κ ↔ k < n κ :=
  ⟨fun h => Nat.lt_of_not_le fun hk => by rw [g.finv_fix hk] at h; omega, g.finv_lt⟩

/-- `g` after `h`. -/
def mul (g h : Perm n) : Perm n where
  f κ k := g.f κ (h.f κ k)
  finv κ k := h.finv κ (g.finv κ k)
  left κ k := by simp only [g.left, h.left]
  right κ k := by simp only [g.right, h.right]
  fix κ k hk := by rw [h.fix κ k hk, g.fix κ k hk]

/-- The inverse. -/
def inv : Perm n where
  f := g.finv
  finv := g.f
  left := g.right
  right := g.left
  fix _ _ hk := g.finv_fix hk

end Perm

namespace Sys

variable (S : Sys K E C O A)

/-- The group of a state, `G_s` (Definition 6): `g` fixes every slot absent in `s` and every
slot of a class the scan leaves out. -/
def mem (s : St E C O) (g : Perm S.n) : Prop :=
  (∀ κ k, S.moves κ = false → g.f κ k = k) ∧ (∀ e k, s.ex e k = .absent → g.f (S.cls e) k = k)

/-- `Group::relabel`: an owner value of `o` under `g`. -/
def rel (g : Perm S.n) (o : O) (v : Option Nat) : Option Nat := v.map (g.f (S.cls (S.par o)))

/-- `Group::apply`: the slot `k` of `s` goes to `g(k)` with its columns, its existence and its
owners, and an owner on a row that is not absent is relabelled. -/
def act (g : Perm S.n) (s : St E C O) : St E C O where
  ex e j := s.ex e (g.finv (S.cls e) j)
  col c j := s.col c (g.finv (S.cls (S.cent c)) j)
  own o j :=
    if s.ex (S.child o) (g.finv (S.cls (S.child o)) j) = .absent
    then s.own o (g.finv (S.cls (S.child o)) j)
    else S.rel g o (s.own o (g.finv (S.cls (S.child o)) j))
  now := s.now

theorem act_ex (g : Perm S.n) (s : St E C O) (e k) :
    (S.act g s).ex e (g.f (S.cls e) k) = s.ex e k := by
  simp [act, g.left]

theorem act_col (g : Perm S.n) (s : St E C O) (c k) :
    (S.act g s).col c (g.f (S.cls (S.cent c)) k) = s.col c k := by
  simp [act, g.left]

theorem act_own (g : Perm S.n) (s : St E C O) (o k) :
    (S.act g s).own o (g.f (S.cls (S.child o)) k) =
      if s.ex (S.child o) k = .absent then s.own o k else S.rel g o (s.own o k) := by
  simp only [act, g.left]

theorem act_mul (g h : Perm S.n) (s : St E C O) :
    S.act (g.mul h) s = S.act g (S.act h s) := by
  apply St.ext'
  · funext e j; rfl
  · funext c j; rfl
  · funext o j
    dsimp only [act, Perm.mul, rel]
    by_cases hx : s.ex (S.child o) (h.finv (S.cls (S.child o)) (g.finv (S.cls (S.child o)) j)) = .absent
    · simp [hx]
    · simp [hx, Option.map_map, Function.comp_def]
  · rfl

theorem act_inv (g : Perm S.n) (s : St E C O) : S.act g.inv (S.act g s) = s := by
  apply St.ext'
  · funext e j; simp [act, Perm.inv, g.left]
  · funext c j; simp [act, Perm.inv, g.left]
  · funext o j
    simp only [act, Perm.inv, rel, g.left]
    split <;> simp_all [Option.map_map, Function.comp_def, g.left]
  · rfl

theorem act_inv' (g : Perm S.n) (s : St E C O) : S.act g (S.act g.inv s) = s := by
  apply St.ext'
  · funext e j; simp [act, Perm.inv, g.right]
  · funext c j; simp [act, Perm.inv, g.right]
  · funext o j
    simp only [act, Perm.inv, rel, g.right]
    split <;> simp_all [Option.map_map, Function.comp_def, g.right]
  · rfl

theorem mul_mem {s : St E C O} {g h : Perm S.n} (hg : S.mem s g) (hh : S.mem s h) :
    S.mem s (g.mul h) := by
  refine ⟨fun κ k hκ => ?_, fun e k he => ?_⟩
  · simp only [Perm.mul]; rw [hh.1 κ k hκ, hg.1 κ k hκ]
  · simp only [Perm.mul]; rw [hh.2 e k he, hg.2 e k he]

theorem inv_mem {s : St E C O} {g : Perm S.n} (hg : S.mem s g) : S.mem s g.inv :=
  ⟨fun κ k hκ => g.finv_of_fix (hg.1 κ k hκ), fun e k he => g.finv_of_fix (hg.2 e k he)⟩

/-- `g ∈ G_s` keeps the absent set: `g` fixes every absent slot. -/
theorem act_absent_iff {s : St E C O} {g : Perm S.n} (hg : S.mem s g) (e k) :
    (S.act g s).ex e k = .absent ↔ s.ex e k = .absent := by
  simp only [act]
  constructor
  · intro h
    have hk := hg.2 e _ h
    rw [g.right] at hk
    rwa [← hk] at h
  · intro h
    rw [g.finv_of_fix (hg.2 e k h)]
    exact h

theorem mem_act {s : St E C O} {g h : Perm S.n} (hg : S.mem s g) :
    S.mem (S.act g s) h ↔ S.mem s h := by
  simp only [mem, S.act_absent_iff hg]

end Sys

/-! ## The semantics: terms, predicates, effects, steps -/

/-- The entity of a parameter. -/
def pent (ps : List (Param E)) (p : Nat) : Option E := (ps[p]?).map Param.entity

/-- The entity of the row a reference names, under the parameters and the quantifiers'
entities (innermost first). -/
def refEnt (ps : List (Param E)) (ctx : List E) : Ref → Option E
  | .param p => pent ps p
  | .next p => pent ps p
  | .bound i => ctx[i]?

/-- The slot a reference names. -/
def slotOf (b : Nat → Nat) (env : List Nat) : Ref → Nat
  | .param p => b p
  | .next p => b p
  | .bound i => (env[i]?).getD 0

/-- The state a reference reads: the successor for `At::Next`, the pre-state otherwise. -/
def stOf (s t : St E C O) : Ref → St E C O
  | .next _ => t
  | .param _ => s
  | .bound _ => s

/-- A term's value: a column's value, an owner's row or `null`, a row's slot. -/
def tval (s t : St E C O) (b : Nat → Nat) (env : List Nat) : Term E C O → Option Nat
  | .col c r => some ((stOf s t r).col c (slotOf b env r))
  | .own o r => (stOf s t r).own o (slotOf b env r)
  | .index _ r => some (slotOf b env r)
  | .lit v => some v
  | .null => none
  | .now => some s.now

/-- A comparison (`compare`): `null` equals only `null` and is never ordered. -/
def Op.ev : Op → Option Nat → Option Nat → Bool
  | .eq, x, y => decide (x = y)
  | .ne, x, y => !decide (x = y)
  | .lt, some x, some y => decide (x < y)
  | .le, some x, some y => decide (x ≤ y)
  | _, _, _ => false

/-- Whether a slot is present. -/
def isPresent (s : St E C O) (e : E) (k : Nat) : Bool :=
  decide (s.ex e k = .present)

/-- The arms of a transition from a value. -/
def tos (arms : List (Nat × Nat)) (v : Nat) : List Nat :=
  (arms.filter (fun a => decide (a.1 = v))).map Prod.snd

/-- The value of the last effect that writes, from the position `i` on. -/
def lastW {β : Type} (f : Nat → Eff C O → Option β) : Nat → List (Eff C O) → Option β
  | _, [] => none
  | i, x :: xs => (lastW f (i + 1) xs).or (f i x)

/-- The existence an effect writes at `(e, k)`. -/
def wex [DecidableEq E] (ps : List (Param E)) (b : Nat → Nat) (e : E) (k : Nat) :
    Eff C O → Option Ex
  | .create p => if pent ps p = some e ∧ b p = k then some .present else none
  | .delete p => if pent ps p = some e ∧ b p = k then some .deleted else none
  | _ => none

/-- The clock an effect writes. -/
def wnow (s : St E C O) : Eff C O → Option Nat
  | .tick => some (s.now + 1)
  | _ => none

/-- Whether a choice's pick is one of its values. -/
def pickOk (s : St E C O) (b : Binding) (i : Nat) : Eff C O → Prop
  | .chooseCol _ _ vs => ∃ v, b.choice i = some v ∧ v ∈ vs
  | .chooseOwn _ _ vs => b.choice i ∈ vs
  | .trans c p arms => (tos arms (s.col c (b.slot p))).isEmpty = false →
      ∃ v, b.choice i = some v ∧ v ∈ tos arms (s.col c (b.slot p))
  | _ => True

/-- Every choice picks one of its values. -/
def PicksOk (xs : List (Eff C O)) (s : St E C O) (b : Binding) : Prop :=
  ∀ i x, xs[i]? = some x → pickOk s b i x

/-- How a slot's existence holds its rows: the present slots, and the lowest absent one. -/
def rows : Rows (Nat → Ex) where
  present f k := f k = .present
  lowestAbsent f k := f k = .absent ∧ ∀ j < k, f j ≠ .absent

/-- What a role binds (`ParamRole::binds_exists`, `candidate_slots`): `Rows.cand`, the
semantics Lemma 6's model gives `Role`, on the existence of the parameter's entity. -/
def roleOk (s : St E C O) (e : E) (k : Nat) (r : Role) : Prop := rows.cand r (s.ex e) k

namespace Sys

variable (S : Sys K E C O A)

/-- A predicate's value on the pre-state `s` and the successor `t`. Quantifiers range over
the present slots of `s` (`expr.rs`, `quantify` and `count`). -/
def eval (s t : St E C O) (b : Nat → Nat) : List Nat → Pred E C O → Bool
  | _, .tt => true
  | _, .ff => false
  | env, .cmp x op y => op.ev (tval s t b env x) (tval s t b env y)
  | env, .inn x ys => ys.any (fun y => decide (tval s t b env x = tval s t b env y))
  | env, .not p => !(eval s t b env p)
  | env, .and p q => eval s t b env p && eval s t b env q
  | env, .or p q => eval s t b env p || eval s t b env q
  | env, .all e p => (List.range (S.n (S.cls e))).all
      (fun k => !isPresent s e k || eval s t b (k :: env) p)
  | env, .any e p => (List.range (S.n (S.cls e))).any
      (fun k => isPresent s e k && eval s t b (k :: env) p)
  | env, .count e p op m => op.ev (some ((List.range (S.n (S.cls e))).countP
      (fun k => isPresent s e k && eval s t b (k :: env) p))) (some m)

variable [DecidableEq E] [DecidableEq C] [DecidableEq O]

/-- The column value an effect writes at `(c, k)`. A delete resets the columns `creset`
names (every column but the run marks) and keeps the owners, (G5). -/
def wcol (ps : List (Param E)) (b : Binding) (s : St E C O) (i : Nat) (c : C) (k : Nat) :
    Eff C O → Option Nat
  | .delete p =>
      if S.creset c = true ∧ pent ps p = some (S.cent c) ∧ b.slot p = k then some (S.cbase c)
      else none
  | .setCol c' p v =>
      if c' = c ∧ pent ps p = some (S.cent c) ∧ b.slot p = k then some v else none
  | .copyCol c' p q =>
      if c' = c ∧ pent ps p = some (S.cent c) ∧ b.slot p = k then some (b.slot q) else none
  | .chooseCol c' p _ =>
      if c' = c ∧ pent ps p = some (S.cent c) ∧ b.slot p = k then b.choice i else none
  | .trans c' p arms =>
      if c' = c ∧ pent ps p = some (S.cent c) ∧ b.slot p = k then
        (if (tos arms (s.col c (b.slot p))).isEmpty then none else b.choice i)
      else none
  | _ => none

/-- The owner value an effect writes at `(o, k)`. -/
def wown (ps : List (Param E)) (b : Binding) (i : Nat) (o : O) (k : Nat) :
    Eff C O → Option (Option Nat)
  | .setOwn o' p q =>
      if o' = o ∧ pent ps p = some (S.child o) ∧ b.slot p = k then some (some (b.slot q))
      else none
  | .setOwnLit o' p v =>
      if o' = o ∧ pent ps p = some (S.child o) ∧ b.slot p = k then some v else none
  | .chooseOwn o' p _ =>
      if o' = o ∧ pent ps p = some (S.child o) ∧ b.slot p = k then some (b.choice i) else none
  | _ => none

/-- The successor of `s` under the action `a` with the binding `b`. -/
def post (a : A) (b : Binding) (s : St E C O) : St E C O where
  ex e k := (lastW (fun _ x => wex (S.params a) b.slot e k x) 0 (S.effs a)).getD (s.ex e k)
  col c k := (lastW (fun i x => S.wcol (S.params a) b s i c k x) 0 (S.effs a)).getD (s.col c k)
  own o k := (lastW (fun i x => S.wown (S.params a) b i o k x) 0 (S.effs a)).getD (s.own o k)
  now := (lastW (fun _ x => wnow s x) 0 (S.effs a)).getD s.now

/-- Every parameter is bound to a slot of its entity that its role admits. -/
def BindOk (ps : List (Param E)) (b : Nat → Nat) (s : St E C O) : Prop :=
  ∀ p q, ps[p]? = some q → b p < S.n (S.cls q.entity) ∧ roleOk s q.entity (b p) q.role

/-- The values an owner of `o` takes: a parent slot, or `null` when it is optional. -/
def ODom (o : O) : Option Nat → Prop
  | none => S.opt o = true
  | some k => k < S.n (S.cls (S.par o))

/-- The initial states (D4): a pre-existing entity's slots are present, a created entity's
absent; an absent row holds base values; a present row's variables start by their `Start`. -/
def Init (s : St E C O) : Prop :=
  (∀ e k, s.ex e k = if S.pre e && decide (k < S.n (S.cls e)) then .present else .absent) ∧
  (∀ c k, s.ex (S.cent c) k = .absent → s.col c k = S.cbase c) ∧
  (∀ c k, s.ex (S.cent c) k = .present →
    if S.cany c then s.col c k < S.cdom c else s.col c k = S.cbase c) ∧
  (∀ o k, s.ex (S.child o) k = .absent → s.own o k = S.obase o) ∧
  (∀ o k, s.ex (S.child o) k = .present →
    match S.ostart o with
    | .base => s.own o k = S.obase o
    | .any => S.ODom o (s.own o k)
    | .identity => s.own o k = some k) ∧
  s.now = 0

/-- The cone system's step relation, labelled by the action and the binding. -/
def lts : LTS (St E C O) (A × Binding) where
  init := S.Init
  step s l t := S.BindOk (S.params l.1) l.2.slot s ∧ PicksOk (S.effs l.1) s l.2 ∧
    (∀ φ ∈ S.guards l.1, S.eval s t l.2.slot [] φ = true) ∧ t = S.post l.1 l.2 s

/-- `W`: every absent row holds the base value of each of its columns and owners. -/
def BaseAbsent (s : St E C O) : Prop :=
  (∀ c k, s.ex (S.cent c) k = .absent → s.col c k = S.cbase c) ∧
  (∀ o k, s.ex (S.child o) k = .absent → s.own o k = S.obase o)

/-- A binding under `g`: each parameter's slot moves by its class. -/
def mvSlot (g : Perm S.n) (ps : List (Param E)) (b : Nat → Nat) (p : Nat) : Nat :=
  match ps[p]? with
  | some q => g.f (S.cls q.entity) (b p)
  | none => b p

/-- The picks under `g`: an owner choice's pick is relabelled. -/
def mvPick (g : Perm S.n) (xs : List (Eff C O)) (pk : Nat → Option Nat) (i : Nat) :
    Option Nat :=
  match xs[i]? with
  | some (.chooseOwn o _ _) => S.rel g o (pk i)
  | _ => pk i

/-- A label under `g`: `g` maps the binding `p` to `g(p)`. -/
def actL (g : Perm S.n) (l : A × Binding) : A × Binding :=
  (l.1, ⟨S.mvSlot g (S.params l.1) l.2.slot, S.mvPick g (S.effs l.1) l.2.choice⟩)

/-! ## The hypotheses -/

/-- A term reads a column or an owner on a row of its own entity. -/
def twf (ps : List (Param E)) (ctx : List E) : Term E C O → Prop
  | .col c r => refEnt ps ctx r = some (S.cent c)
  | .own o r => refEnt ps ctx r = some (S.child o)
  | .index e r => refEnt ps ctx r = some e
  | _ => True

/-- Every term of a predicate is well typed, a quantifier's slot being a row of its entity. -/
def pwf (ps : List (Param E)) : List E → Pred E C O → Prop
  | ctx, .cmp x _ y => S.twf ps ctx x ∧ S.twf ps ctx y
  | ctx, .inn x ys => S.twf ps ctx x ∧ ∀ y ∈ ys, S.twf ps ctx y
  | ctx, .not p => pwf ps ctx p
  | ctx, .and p q => pwf ps ctx p ∧ pwf ps ctx q
  | ctx, .or p q => pwf ps ctx p ∧ pwf ps ctx q
  | ctx, .all e p => pwf ps (e :: ctx) p
  | ctx, .any e p => pwf ps (e :: ctx) p
  | ctx, .count e p _ _ => pwf ps (e :: ctx) p
  | _, .tt => True
  | _, .ff => True

/-- An effect writes a column or an owner on a row of its own entity. -/
def ewf (ps : List (Param E)) : Eff C O → Prop
  | .create p => pent ps p ≠ none
  | .delete p => pent ps p ≠ none
  | .setCol c p _ => pent ps p = some (S.cent c)
  | .copyCol c p _ => pent ps p = some (S.cent c)
  | .chooseCol c p _ => pent ps p = some (S.cent c)
  | .trans c p _ => pent ps p = some (S.cent c)
  | .setOwn o p _ => pent ps p = some (S.child o)
  | .setOwnLit o p _ => pent ps p = some (S.child o)
  | .chooseOwn o p _ => pent ps p = some (S.child o)
  | .tick => True

/-- Well-typedness of every guard and effect. -/
structure WF : Prop where
  guards : ∀ a φ, φ ∈ S.guards a → S.pwf (S.params a) [] φ
  effs : ∀ a x, x ∈ S.effs a → S.ewf (S.params a) x

/-- What the builder gives and the scan does not check: a `New` parameter is created by its
action (`build/actions.rs`), and an `Earlier` parameter carries the run-mark conjunct
`mark = k` of `run_marks`, whose base (`none`) is not `k`. -/
structure Built : Prop where
  new_create : ∀ a p q, (S.params a)[p]? = some q → q.role = .new → Eff.create p ∈ S.effs a
  earlier_mark : ∀ a p q st, (S.params a)[p]? = some q → q.role = .earlier st →
    ∃ m k, Pred.cmp (.col m (.param p)) .eq (.lit k) ∈ S.guards a ∧ S.cent m = q.entity ∧
      S.cbase m ≠ k

/-- The class of the rows a term names: an owner's parent, an index's entity. -/
def tcls : Term E C O → Option K
  | .own o _ => some (S.cls (S.par o))
  | .index e _ => some (S.cls e)
  | _ => none

/-- `tcls`, for a class that moves. -/
def ecl (x : Term E C O) : Option K :=
  match S.tcls x with
  | some κ => if S.moves κ then some κ else none
  | none => none

/-- (G1) for one comparison: terms naming rows of a class that moves are compared only by
`=` or `≠`, with a term naming rows of the same class or with `null`. Rules `OrderedId`,
`IdAgainstLiteral`, `MixedIds`. -/
def g1cmp (x : Term E C O) (op : Op) (y : Term E C O) : Prop :=
  (S.ecl x = S.ecl y ∧ (S.ecl x = none ∨ op = .eq ∨ op = .ne)) ∨
  ((op = .eq ∨ op = .ne) ∧ (x = .null ∨ y = .null))

/-- (G1) for a predicate; `In` names no row of a class that moves (`OrderedId`). -/
def G1 : Pred E C O → Prop
  | .cmp x op y => S.g1cmp x op y
  | .inn x ys => S.ecl x = none ∧ ∀ y ∈ ys, S.ecl y = none
  | .not p => G1 p
  | .and p q => G1 p ∧ G1 q
  | .or p q => G1 p ∧ G1 q
  | .all _ p => G1 p
  | .any _ p => G1 p
  | .count _ p _ _ => G1 p
  | .tt => True
  | .ff => True

/-- (G6) for a term of a guard: no owner read on a `New` parameter in the pre-state. Rule
`AbsentOwner`. -/
def g6t (ps : List (Param E)) : Term E C O → Prop
  | .own _ (.param p) => ∀ q, ps[p]? = some q → q.role ≠ .new
  | _ => True

/-- (G6) for a guard. -/
def G6 (ps : List (Param E)) : Pred E C O → Prop
  | .cmp x _ y => g6t ps x ∧ g6t ps y
  | .inn x ys => g6t ps x ∧ ∀ y ∈ ys, g6t ps y
  | .not p => G6 ps p
  | .and p q => G6 ps p ∧ G6 ps q
  | .or p q => G6 ps p ∧ G6 ps q
  | .all _ p => G6 ps p
  | .any _ p => G6 ps p
  | .count _ p _ _ => G6 ps p
  | .tt => True
  | .ff => True

/-- (G2) for an effect: an owner is written from a parameter of the parent's class or
`null`, a choice of an owner ranges over every row, and a data column is not written from a
slot of a class that moves. Rules `OwnerLiteral`, `MixedIds`, `OwnerChoice`. -/
def G2 (ps : List (Param E)) : Eff C O → Prop
  | .setOwn o _ q => ∃ q', ps[q]? = some q' ∧ (S.cls q'.entity = S.cls (S.par o) ∨
      (S.moves (S.cls (S.par o)) = false ∧ S.moves (S.cls q'.entity) = false))
  | .setOwnLit o _ v => v = none ∨ S.moves (S.cls (S.par o)) = false
  | .copyCol _ _ q => ∃ q', ps[q]? = some q' ∧ S.moves (S.cls q'.entity) = false
  | .chooseOwn o _ vs => (∀ k < S.n (S.cls (S.par o)), some k ∈ vs) ∨
      S.moves (S.cls (S.par o)) = false
  | _ => True

/-- (G3): no start pins a slot. An identity start couples the child's class with the
parent's, and a pre-existing row's owner starting at its base names no row. Rule
`PinnedStart`. -/
def G3 : Prop := ∀ o,
  (S.ostart o = .identity → S.cls (S.child o) = S.cls (S.par o) ∨
    (S.moves (S.cls (S.child o)) = false ∧ S.moves (S.cls (S.par o)) = false)) ∧
  (S.ostart o = .base → S.pre (S.child o) = true →
    (S.opt o = true ∧ S.obase o = none) ∨ S.moves (S.cls (S.par o)) = false)

/-- (G4): a creation writes every owner of the created row whose base names a row. Rule
`UnlinkedCreate`. -/
def G4 (a : A) : Prop :=
  ∀ p, Eff.create p ∈ S.effs a → ∀ o, pent (S.params a) p = some (S.child o) →
    (S.opt o = true ∧ S.obase o = none) ∨ S.moves (S.cls (S.par o)) = false ∨
    (∃ q, Eff.setOwn o p q ∈ S.effs a) ∨ (∃ vs, Eff.chooseOwn o p vs ∈ S.effs a)

/-- (G1)–(G4) and (G6) on every guard and effect. -/
structure Hyp : Prop where
  g1 : ∀ a φ, φ ∈ S.guards a → S.G1 φ
  g2 : ∀ a x, x ∈ S.effs a → S.G2 (S.params a) x
  g3 : S.G3
  g4 : ∀ a, S.G4 a
  g6 : ∀ a φ, φ ∈ S.guards a → G6 (S.params a) φ

end Sys

/-! ## Reading the last write -/

section LastW

variable {β γ : Type}

theorem lastW_congr {f f' : Nat → Eff C O → Option β} :
    ∀ (xs : List (Eff C O)) (j : Nat), (∀ i x, xs[i]? = some x → f (j + i) x = f' (j + i) x) →
      lastW f j xs = lastW f' j xs
  | [], _, _ => rfl
  | x :: xs, j, h => by
    have h0 := h 0 x rfl
    rw [Nat.add_zero] at h0
    have ih := lastW_congr (f := f) (f' := f') xs (j + 1) (fun i y hy => by
      have := h (i + 1) y (by simpa using hy)
      rw [show j + (i + 1) = j + 1 + i by omega] at this; exact this)
    simp only [lastW, ih, h0]

theorem lastW_map {f : Nat → Eff C O → Option β} {f' : Nat → Eff C O → Option γ} (φ : γ → β) :
    ∀ (xs : List (Eff C O)) (j : Nat),
      (∀ i x, xs[i]? = some x → f (j + i) x = (f' (j + i) x).map φ) →
      lastW f j xs = (lastW f' j xs).map φ
  | [], _, _ => rfl
  | x :: xs, j, h => by
    have h0 := h 0 x rfl
    rw [Nat.add_zero] at h0
    have ih := lastW_map (f := f) (f' := f') φ xs (j + 1) (fun i y hy => by
      have := h (i + 1) y (by simpa using hy)
      rwa [show j + (i + 1) = j + 1 + i by omega] at this)
    simp only [lastW, ih, h0]
    cases lastW f' (j + 1) xs <;> rfl

theorem lastW_some {f : Nat → Eff C O → Option β} :
    ∀ (xs : List (Eff C O)) (j : Nat) {v : β}, lastW f j xs = some v →
      ∃ i x, xs[i]? = some x ∧ f (j + i) x = some v
  | [], _, _, h => by simp [lastW] at h
  | x :: xs, j, v, h => by
    simp only [lastW, Option.or_eq_some_iff] at h
    rcases h with h | ⟨_, h⟩
    · obtain ⟨i, y, hy, hf⟩ := lastW_some (f := f) xs (j + 1) h
      exact ⟨i + 1, y, by simpa using hy, by rwa [show j + (i + 1) = j + 1 + i by omega]⟩
    · exact ⟨0, x, rfl, by simpa using h⟩

theorem lastW_ne_none {f : Nat → Eff C O → Option β} :
    ∀ (xs : List (Eff C O)) (j i : Nat) {x : Eff C O}, xs[i]? = some x → f (j + i) x ≠ none →
      lastW f j xs ≠ none
  | [], _, _, _, h, _ => by simp at h
  | x :: xs, j, 0, y, h, hf => by
    simp only [List.getElem?_cons_zero, Option.some.injEq] at h
    subst h
    simp only [lastW]
    cases lastW f (j + 1) xs <;> simp_all
  | x :: xs, j, i + 1, y, h, hf => by
    have := lastW_ne_none (f := f) xs (j + 1) i (by simpa using h)
      (by rwa [show j + 1 + i = j + (i + 1) by omega])
    simp only [lastW]
    cases hl : lastW f (j + 1) xs <;> simp_all

end LastW

theorem pent_eq {ps : List (Param E)} {p : Nat} {e : E} :
    pent ps p = some e ↔ ∃ q, ps[p]? = some q ∧ q.entity = e := by
  simp [pent, Option.map_eq_some_iff]

theorem ite_and_congr {α : Type} {P u u' : Prop} [Decidable P] [Decidable u] [Decidable u']
    {v v' w : α} (hu : u ↔ u') (hv : P → u' → v = v') :
    (if P ∧ u then v else w) = (if P ∧ u' then v' else w) := by
  by_cases hP : P
  · by_cases hu' : u'
    · have := hv hP hu'; simp [hP, hu', hu.2 hu', this]
    · have : ¬u := fun h => hu' (hu.1 h); simp [hP, hu', this]
  · simp [hP]

theorem map_ite_some {α β : Type} {c : Prop} [Decidable c] (f : α → β) (v : α) :
    (if c then some v else none).map f = if c then some (f v) else none := by
  split <;> rfl

/-- The combination of an owner's last write and its relabelling, in `post_comm`. -/
theorem own_combine {L' L : Option (Option Nat)} {r : Option Nat → Option Nat} {v : Option Nat}
    {P Q : Prop} [Decidable P] [Decidable Q] (hL : L' = L.map r) (h1 : ∀ w, L = some w → ¬Q)
    (h2 : L = none → P → ¬Q → r v = v) (h3 : ¬P → ¬Q) :
    L'.getD (if P then v else r v) = if Q then L.getD v else r (L.getD v) := by
  subst hL
  cases L with
  | none =>
    by_cases hP : P <;> by_cases hQ : Q
    · simp [hP, hQ]
    · simp [hP, hQ, h2 rfl hP hQ]
    · exact absurd hQ (h3 hP)
    · simp [hP, hQ]
  | some w => simp [h1 w rfl]

theorem wex_ne_absent [DecidableEq E] {ps : List (Param E)} {sl : Nat → Nat} {e : E} {k : Nat}
    {x : Eff C O} {v : Ex} (h : wex ps sl e k x = some v) : v ≠ .absent := by
  cases x <;> simp only [wex] at h
  all_goals (try split at h) <;> cases h <;> decide

theorem wex_some [DecidableEq E] {ps : List (Param E)} {sl : Nat → Nat} {e : E} {k : Nat}
    {x : Eff C O} {v : Ex} (h : wex ps sl e k x = some v) :
    ∃ p q, ps[p]? = some q ∧ q.entity = e ∧ sl p = k ∧ (x = .create p ∨ x = .delete p) := by
  cases x <;> simp only [wex] at h
  all_goals first
    | (cases h; done)
    | (split at h
       · rename_i hc
         obtain ⟨q, hq, he⟩ := pent_eq.1 hc.1
         first
           | exact ⟨_, q, hq, he, hc.2, .inl rfl⟩
           | exact ⟨_, q, hq, he, hc.2, .inr rfl⟩
       · cases h)

namespace Sys

variable (S : Sys K E C O A)

/-! ## The action on terms -/

/-- A value under `g`, for the class of the rows it names. -/
def rlc (g : Perm S.n) : Option K → Option Nat → Option Nat
  | none, v => v
  | some κ, v => v.map (g.f κ)

/-- The slots of the quantifiers under `g`. -/
def mvEnv (g : Perm S.n) : List E → List Nat → List Nat
  | e :: ctx, k :: env => g.f (S.cls e) k :: mvEnv g ctx env
  | _, _ => []

variable {S}

theorem rel_fix {s : St E C O} {g : Perm S.n} (hg : S.mem s g) {o : O}
    (h : S.moves (S.cls (S.par o)) = false) (v : Option Nat) : S.rel g o v = v := by
  cases v <;> simp [rel, hg.1 _ _ h]

theorem mvSlot_of {g : Perm S.n} {ps : List (Param E)} {sl : Nat → Nat} {p : Nat} {q : Param E}
    (h : ps[p]? = some q) : S.mvSlot g ps sl p = g.f (S.cls q.entity) (sl p) := by
  simp [mvSlot, h]

theorem mvSlot_pent {g : Perm S.n} {ps : List (Param E)} {sl : Nat → Nat} {p : Nat} {e : E}
    (h : pent ps p = some e) : S.mvSlot g ps sl p = g.f (S.cls e) (sl p) := by
  obtain ⟨q, hq, rfl⟩ := pent_eq.1 h
  exact mvSlot_of hq

theorem hit_mv {g : Perm S.n} {ps : List (Param E)} {sl : Nat → Nat} {p : Nat} {e : E} (k : Nat) :
    (pent ps p = some e ∧ S.mvSlot g ps sl p = g.f (S.cls e) k) ↔ (pent ps p = some e ∧ sl p = k) :=
  ⟨fun ⟨h, h'⟩ => ⟨h, g.f_inj (by rw [← mvSlot_pent h]; exact h')⟩,
   fun ⟨h, h'⟩ => ⟨h, by rw [mvSlot_pent h, h']⟩⟩

theorem rlc_inj (g : Perm S.n) (c : Option K) {u w : Option Nat} :
    S.rlc g c u = S.rlc g c w ↔ u = w := by
  cases c with
  | none => rfl
  | some κ =>
    constructor
    · intro h
      cases u <;> cases w <;> simp_all [rlc]
      exact g.f_inj h
    · intro h; rw [h]

theorem rlc_none (g : Perm S.n) (c : Option K) {w : Option Nat} :
    S.rlc g c w = none ↔ w = none := by
  cases c <;> cases w <;> simp [rlc]

theorem rlc_ecl {s : St E C O} {g : Perm S.n} (hg : S.mem s g) (x : Term E C O) (v : Option Nat) :
    S.rlc g (S.tcls x) v = S.rlc g (S.ecl x) v := by
  cases hc : S.tcls x with
  | none => simp only [ecl, hc]
  | some κ =>
    cases hm : S.moves κ
    · have : S.ecl x = none := by simp [ecl, hc, hm]
      rw [this]
      cases v <;> simp [rlc, hg.1 κ _ hm]
    · have : S.ecl x = some κ := by simp [ecl, hc, hm]
      rw [this]

theorem ev_rlc (g : Perm S.n) {c c' : Option K} {op : Op} {u w : Option Nat}
    (h : (c = c' ∧ (c = none ∨ op = .eq ∨ op = .ne)) ∨
      ((op = .eq ∨ op = .ne) ∧ ((c = none ∧ u = none) ∨ (c' = none ∧ w = none)))) :
    op.ev (S.rlc g c u) (S.rlc g c' w) = op.ev u w := by
  rcases h with ⟨rfl, hc | hop | hop⟩ | ⟨hop, ⟨rfl, rfl⟩ | ⟨rfl, rfl⟩⟩
  · subst hc; rfl
  · subst hop; simp only [Op.ev, rlc_inj]
  · subst hop; simp only [Op.ev, rlc_inj]
  · have h0 : S.rlc g none none = none := rfl
    rw [h0]
    rcases hop with rfl | rfl <;> simp only [Op.ev, eq_comm (a := (none : Option Nat)), rlc_none]
  · have h0 : S.rlc g none none = none := rfl
    rw [h0]
    rcases hop with rfl | rfl <;> simp only [Op.ev, rlc_none]

theorem stOf_act (g : Perm S.n) (s t : St E C O) (r : Ref) :
    stOf (S.act g s) (S.act g t) r = S.act g (stOf s t r) := by
  cases r <;> rfl

theorem mvEnv_get (g : Perm S.n) : ∀ (ctx : List E) (env : List Nat) (i : Nat) {e : E},
    env.length = ctx.length → ctx[i]? = some e →
      ((S.mvEnv g ctx env)[i]?).getD 0 = g.f (S.cls e) ((env[i]?).getD 0)
  | [], _, _, _, _, h => by simp at h
  | _ :: _, [], _, _, hl, _ => by simp at hl
  | _ :: _, _ :: _, 0, _, _, h => by
    simp only [List.getElem?_cons_zero, Option.some.injEq] at h
    subst h; rfl
  | _ :: ctx, _ :: env, i + 1, _, hl, h =>
    mvEnv_get g ctx env i (by simpa using hl) (by simpa using h)

theorem slotOf_mv (g : Perm S.n) {ps : List (Param E)} {ctx : List E} {b : Nat → Nat}
    {env : List Nat} (hl : env.length = ctx.length) {r : Ref} {e : E}
    (h : refEnt ps ctx r = some e) :
    slotOf (S.mvSlot g ps b) (S.mvEnv g ctx env) r = g.f (S.cls e) (slotOf b env r) := by
  cases r with
  | param p => exact mvSlot_pent h
  | next p => exact mvSlot_pent h
  | bound i => exact mvEnv_get g ctx env i hl h

/-- A term's value under `g` is its value relabelled by the class of the rows it names, when
every owner it reads sits on a row that is not absent. -/
theorem tval_mv (g : Perm S.n) (s t : St E C O) (b : Nat → Nat) (ps : List (Param E))
    (ctx : List E) (env : List Nat) (hl : env.length = ctx.length) (x : Term E C O)
    (hx : S.twf ps ctx x)
    (hsafe : ∀ o r, x = .own o r → (stOf s t r).ex (S.child o) (slotOf b env r) ≠ .absent) :
    tval (S.act g s) (S.act g t) (S.mvSlot g ps b) (S.mvEnv g ctx env) x =
      S.rlc g (S.tcls x) (tval s t b env x) := by
  cases x with
  | col c r =>
    simp only [tval, tcls, rlc, stOf_act, slotOf_mv g hl hx, act_col]
  | own o r =>
    simp only [tval, tcls, rlc, stOf_act, slotOf_mv g hl hx, act_own,
      ite_eq_right (hsafe o r rfl), rel]
  | index e r => simp only [tval, tcls, rlc, slotOf_mv g hl hx, Option.map_some]
  | lit v => rfl
  | null => rfl
  | now => rfl

/-- The three ranges a quantifier reads are the same under a permutation of the slots. -/
theorem range_all_perm (g : Perm S.n) (κ : K) {F F' : Nat → Bool}
    (h : ∀ k, k < S.n κ → F' (g.f κ k) = F k) :
    (List.range (S.n κ)).all F' = (List.range (S.n κ)).all F := by
  apply Bool.eq_iff_iff.2
  simp only [List.all_eq_true, List.mem_range]
  constructor
  · intro H k hk; rw [← h k hk]; exact H _ (g.f_lt hk)
  · intro H j hj
    have := h (g.finv κ j) (g.finv_lt hj)
    rw [g.right] at this
    rw [this]; exact H _ (g.finv_lt hj)

theorem range_any_perm (g : Perm S.n) (κ : K) {F F' : Nat → Bool}
    (h : ∀ k, k < S.n κ → F' (g.f κ k) = F k) :
    (List.range (S.n κ)).any F' = (List.range (S.n κ)).any F := by
  apply Bool.eq_iff_iff.2
  simp only [List.any_eq_true, List.mem_range]
  constructor
  · rintro ⟨j, hj, hF⟩
    have := h (g.finv κ j) (g.finv_lt hj)
    rw [g.right] at this
    exact ⟨_, g.finv_lt hj, this ▸ hF⟩
  · rintro ⟨k, hk, hF⟩
    exact ⟨_, g.f_lt hk, (h k hk).symm ▸ hF⟩

theorem range_countP_perm (g : Perm S.n) (κ : K) {F F' : Nat → Bool}
    (h : ∀ k, k < S.n κ → F' (g.f κ k) = F k) :
    (List.range (S.n κ)).countP F' = (List.range (S.n κ)).countP F := by
  have hperm : List.Perm ((List.range (S.n κ)).map (g.f κ)) (List.range (S.n κ)) := by
    refine (List.perm_ext_iff_of_nodup ?_ List.nodup_range).2 ?_
    · exact List.pairwise_map.2 (List.nodup_range.imp fun hne heq => hne (g.f_inj heq))
    · intro j
      simp only [List.mem_map, List.mem_range]
      constructor
      · rintro ⟨k, hk, rfl⟩; exact g.f_lt hk
      · intro hj; exact ⟨g.finv κ j, g.finv_lt hj, g.right κ j⟩
  rw [← hperm.countP_eq, List.countP_map]
  apply List.countP_congr
  intro k hk
  rw [List.mem_range] at hk
  show F' (g.f κ k) = true ↔ F k = true
  rw [h k hk]

/-- **Predicates commute with `g`.** Under (G1), (G6) and well-typedness, a predicate has the
same value at `(s, t, p)` and at `(g(s), g(t), g(p))`, given that every parameter's row that
the pre-state may read an owner of is not absent in `s`, and every parameter's row is not
absent in `t`. -/
theorem eval_mv (g : Perm S.n) (s t : St E C O) (b : Nat → Nat) (hg : S.mem s g)
    (ps : List (Param E))
    (hown : ∀ p q, ps[p]? = some q → q.role ≠ .new → s.ex q.entity (b p) ≠ .absent)
    (hnext : ∀ p q, ps[p]? = some q → t.ex q.entity (b p) ≠ .absent) :
    ∀ (φ : Pred E C O) (ctx : List E) (env : List Nat), S.pwf ps ctx φ → S.G1 φ → G6 ps φ →
      env.length = ctx.length →
      (∀ (i : Nat) (e : E), ctx[i]? = some e → s.ex e ((env[i]?).getD 0) = .present) →
      S.eval (S.act g s) (S.act g t) (S.mvSlot g ps b) (S.mvEnv g ctx env) φ =
        S.eval s t b env φ := by
  have safe : ∀ (ctx : List E) (env : List Nat) (x : Term E C O), S.twf ps ctx x → g6t ps x →
      (∀ (i : Nat) (e : E), ctx[i]? = some e → s.ex e ((env[i]?).getD 0) = .present) →
      ∀ o r, x = .own o r → (stOf s t r).ex (S.child o) (slotOf b env r) ≠ .absent := by
    intro ctx env x hx h6 hpres o r hxr
    subst hxr
    cases r with
    | param p =>
      obtain ⟨q, hq, he⟩ := pent_eq.1 hx
      rw [← he]; exact hown p q hq (h6 q hq)
    | next p =>
      obtain ⟨q, hq, he⟩ := pent_eq.1 hx
      rw [← he]; exact hnext p q hq
    | bound i =>
      intro h
      have := hpres i _ hx
      simp only [stOf, slotOf] at h
      rw [h] at this; cases this
  have tv : ∀ (ctx : List E) (env : List Nat) (x : Term E C O), env.length = ctx.length →
      S.twf ps ctx x → g6t ps x →
      (∀ (i : Nat) (e : E), ctx[i]? = some e → s.ex e ((env[i]?).getD 0) = .present) →
      tval (S.act g s) (S.act g t) (S.mvSlot g ps b) (S.mvEnv g ctx env) x =
        S.rlc g (S.ecl x) (tval s t b env x) := by
    intro ctx env x hl hx h6 hpres
    rw [tval_mv g s t b ps ctx env hl x hx (safe ctx env x hx h6 hpres), rlc_ecl hg]
  have ecl_none : ∀ x : Term E C O, S.ecl x = none → ∀ v, S.rlc g (S.ecl x) v = v := by
    intro x h v; rw [h]; rfl
  intro φ
  induction φ with
  | tt => intros; rfl
  | ff => intros; rfl
  | cmp x op y =>
    intro ctx env hwf h1 h6 hl hpres
    simp only [eval]
    rw [tv ctx env x hl hwf.1 h6.1 hpres, tv ctx env y hl hwf.2 h6.2 hpres]
    apply ev_rlc
    rcases h1 with h | ⟨hop, hn | hn⟩
    · exact .inl h
    · subst hn; exact .inr ⟨hop, .inl ⟨rfl, rfl⟩⟩
    · subst hn; exact .inr ⟨hop, .inr ⟨rfl, rfl⟩⟩
  | inn x ys =>
    intro ctx env hwf h1 h6 hl hpres
    simp only [eval]
    rw [tv ctx env x hl hwf.1 h6.1 hpres, ecl_none x h1.1]
    apply Bool.eq_iff_iff.2
    simp only [List.any_eq_true]
    apply exists_congr; intro y; apply and_congr_right; intro hy
    rw [tv ctx env y hl (hwf.2 y hy) (h6.2 y hy) hpres, ecl_none y (h1.2 y hy)]
  | not p ih =>
    intro ctx env hwf h1 h6 hl hpres
    simp only [eval, ih ctx env hwf h1 h6 hl hpres]
  | and p q ihp ihq =>
    intro ctx env hwf h1 h6 hl hpres
    simp only [eval, ihp ctx env hwf.1 h1.1 h6.1 hl hpres, ihq ctx env hwf.2 h1.2 h6.2 hl hpres]
  | or p q ihp ihq =>
    intro ctx env hwf h1 h6 hl hpres
    simp only [eval, ihp ctx env hwf.1 h1.1 h6.1 hl hpres, ihq ctx env hwf.2 h1.2 h6.2 hl hpres]
  | all e p ih =>
    intro ctx env hwf h1 h6 hl hpres
    simp only [eval]
    apply range_all_perm g (S.cls e)
    intro k _
    have hp : isPresent (S.act g s) e (g.f (S.cls e) k) = isPresent s e k := by
      simp only [isPresent, act_ex]
    rw [hp]
    cases hk : isPresent s e k
    · rfl
    · have hk' : s.ex e k = .present := of_decide_eq_true hk
      have := ih (e :: ctx) (k :: env) hwf h1 h6 (by simp [hl]) (by
        intro i e' he'
        cases i with
        | zero => simp only [List.getElem?_cons_zero, Option.some.injEq] at he'; subst he'; exact hk'
        | succ i => exact hpres i e' (by simpa using he'))
      simp only [mvEnv] at this
      simp only [this]
  | any e p ih =>
    intro ctx env hwf h1 h6 hl hpres
    simp only [eval]
    apply range_any_perm g (S.cls e)
    intro k _
    have hp : isPresent (S.act g s) e (g.f (S.cls e) k) = isPresent s e k := by
      simp only [isPresent, act_ex]
    rw [hp]
    cases hk : isPresent s e k
    · rfl
    · have hk' : s.ex e k = .present := of_decide_eq_true hk
      have := ih (e :: ctx) (k :: env) hwf h1 h6 (by simp [hl]) (by
        intro i e' he'
        cases i with
        | zero => simp only [List.getElem?_cons_zero, Option.some.injEq] at he'; subst he'; exact hk'
        | succ i => exact hpres i e' (by simpa using he'))
      simp only [mvEnv] at this
      simp only [this]
  | count e p op m ih =>
    intro ctx env hwf h1 h6 hl hpres
    simp only [eval]
    congr 2
    apply range_countP_perm g (S.cls e)
    intro k _
    have hp : isPresent (S.act g s) e (g.f (S.cls e) k) = isPresent s e k := by
      simp only [isPresent, act_ex]
    rw [hp]
    cases hk : isPresent s e k
    · rfl
    · have hk' : s.ex e k = .present := of_decide_eq_true hk
      have := ih (e :: ctx) (k :: env) hwf h1 h6 (by simp [hl]) (by
        intro i e' he'
        cases i with
        | zero => simp only [List.getElem?_cons_zero, Option.some.injEq] at he'; subst he'; exact hk'
        | succ i => exact hpres i e' (by simpa using he'))
      simp only [mvEnv] at this
      simp only [this]

/-! ## The action on effects -/

variable [DecidableEq E] [DecidableEq C] [DecidableEq O]

theorem wcol_some {ps : List (Param E)} {b : Binding} {s : St E C O} {i : Nat} {c : C} {k : Nat}
    {x : Eff C O} {v : Nat} (h : S.wcol ps b s i c k x = some v) :
    ∃ p q, ps[p]? = some q ∧ q.entity = S.cent c ∧ b.slot p = k := by
  cases x <;> simp only [wcol] at h
  all_goals first
    | (cases h; done)
    | (split at h
       · rename_i hc
         obtain ⟨q, hq, he⟩ := pent_eq.1 hc.2.1
         exact ⟨_, q, hq, he, hc.2.2⟩
       · cases h)

theorem wown_some {ps : List (Param E)} {b : Binding} {i : Nat} {o : O} {k : Nat}
    {x : Eff C O} {v : Option Nat} (h : S.wown ps b i o k x = some v) :
    ∃ p q, ps[p]? = some q ∧ q.entity = S.child o ∧ b.slot p = k := by
  cases x <;> simp only [wown] at h
  all_goals first
    | (cases h; done)
    | (split at h
       · rename_i hc
         obtain ⟨q, hq, he⟩ := pent_eq.1 hc.2.1
         exact ⟨_, q, hq, he, hc.2.2⟩
       · cases h)

theorem wex_mv (g : Perm S.n) (ps : List (Param E)) (sl : Nat → Nat) (e : E) (k : Nat)
    (x : Eff C O) :
    wex ps (S.mvSlot g ps sl) e (g.f (S.cls e) k) x = wex ps sl e k x := by
  cases x <;> simp only [wex, hit_mv]

/-- A column write commutes with `g`, by (G2) for a column written from a slot. -/
theorem wcol_mv {s : St E C O} {g : Perm S.n} (hg : S.mem s g) {a : A} {b : Binding} {i : Nat}
    {x : Eff C O} (hx : (S.effs a)[i]? = some x) (h2 : S.G2 (S.params a) x)
    (hw : S.ewf (S.params a) x) (c : C) (k : Nat) :
    S.wcol (S.params a) (S.actL g (a, b)).2 (S.act g s) i c (g.f (S.cls (S.cent c)) k) x =
      S.wcol (S.params a) b s i c k x := by
  cases x with
  | delete p =>
    simp only [wcol]
    exact ite_and_congr (hit_mv k) (fun _ _ => rfl)
  | setCol c' p v =>
    simp only [wcol]
    exact ite_and_congr (hit_mv k) (fun _ _ => rfl)
  | copyCol c' p q =>
    obtain ⟨q', hq', hfix⟩ := h2
    simp only [wcol]
    exact ite_and_congr (hit_mv k) (fun _ _ => by
      show some (S.mvSlot g (S.params a) b.slot q) = some (b.slot q)
      rw [mvSlot_of hq', hg.1 _ _ hfix])
  | chooseCol c' p vs =>
    simp only [wcol]
    exact ite_and_congr (hit_mv k) (fun _ _ => by
      show S.mvPick g (S.effs a) b.choice i = b.choice i
      simp [mvPick, hx])
  | trans c' p arms =>
    simp only [wcol]
    exact ite_and_congr (hit_mv k) (fun hc _ => by
      subst hc
      have h1 : (S.act g s).col c' (S.mvSlot g (S.params a) b.slot p) = s.col c' (b.slot p) := by
        rw [mvSlot_pent hw, act_col]
      have h2' : S.mvPick g (S.effs a) b.choice i = b.choice i := by simp [mvPick, hx]
      show (if (tos arms ((S.act g s).col c' (S.mvSlot g (S.params a) b.slot p))).isEmpty
          then none else S.mvPick g (S.effs a) b.choice i) = _
      rw [h1, h2'])
  | create p => rfl
  | setOwn o p q => rfl
  | setOwnLit o p v => rfl
  | chooseOwn o p vs => rfl
  | tick => rfl

/-- An owner write commutes with `g`, relabelled: by (G2), an owner is written from a slot of
its parent's class, from `null`, or by a choice over every row. -/
theorem wown_mv {s : St E C O} {g : Perm S.n} (hg : S.mem s g) {a : A} {b : Binding} {i : Nat}
    {x : Eff C O} (hx : (S.effs a)[i]? = some x) (h2 : S.G2 (S.params a) x) (o : O) (k : Nat) :
    S.wown (S.params a) (S.actL g (a, b)).2 i o (g.f (S.cls (S.child o)) k) x =
      (S.wown (S.params a) b i o k x).map (S.rel g o) := by
  cases x with
  | setOwn o' p q =>
    simp only [wown]
    rw [map_ite_some]
    exact ite_and_congr (hit_mv k) (fun ho _ => by
      subst ho
      obtain ⟨q', hq', hc⟩ := h2
      show some (some (S.mvSlot g (S.params a) b.slot q)) = some (S.rel g o' (some (b.slot q)))
      rw [mvSlot_of hq']
      simp only [rel, Option.map_some]
      rcases hc with hc | ⟨h1, h1'⟩
      · rw [hc]
      · rw [hg.1 _ _ h1, hg.1 _ _ h1'])
  | setOwnLit o' p v =>
    simp only [wown]
    rw [map_ite_some]
    exact ite_and_congr (hit_mv k) (fun ho _ => by
      subst ho
      rcases h2 with rfl | hfix
      · rfl
      · rw [rel_fix hg hfix])
  | chooseOwn o' p vs =>
    simp only [wown]
    rw [map_ite_some]
    exact ite_and_congr (hit_mv k) (fun ho _ => by
      subst ho
      show some (S.mvPick g (S.effs a) b.choice i) = _
      simp [mvPick, hx])
  | create p => rfl
  | delete p => rfl
  | setCol c p v => rfl
  | copyCol c p q => rfl
  | chooseCol c p vs => rfl
  | trans c p arms => rfl
  | tick => rfl

/-! ## The step -/

section Step

variable {a : A} {b : Binding} {s : St E C O}

theorem post_ex_absent {e : E} {k : Nat} (h : (S.post a b s).ex e k = .absent) :
    s.ex e k = .absent ∧ lastW (fun _ x => wex (S.params a) b.slot e k x) 0 (S.effs a) = none := by
  have h' : (lastW (fun _ x => wex (S.params a) b.slot e k x) 0 (S.effs a)).getD (s.ex e k) =
      .absent := h
  cases hl : lastW (fun _ x => wex (S.params a) b.slot e k x) 0 (S.effs a) with
  | none => rw [hl] at h'; exact ⟨h', rfl⟩
  | some v =>
    rw [hl] at h'
    obtain ⟨_, _, _, hx⟩ := lastW_some _ _ hl
    exact absurd h' (wex_ne_absent hx)

theorem post_ex_ne {e : E} {k : Nat} (h : s.ex e k ≠ .absent) : (S.post a b s).ex e k ≠ .absent :=
  fun h' => h (post_ex_absent h').1

/-- A parameter's row is not absent unless the parameter is `New`: `Existing`, `Parent` and
`Triggered` bind present rows, and an `Earlier` row holds a run mark, which an absent row of
`W` does not. -/
theorem bound_nonabsent {t : St E C O} (hb : S.Built) (hW : S.BaseAbsent s)
    (hB : S.BindOk (S.params a) b.slot s) (hGd : ∀ φ ∈ S.guards a, S.eval s t b.slot [] φ = true)
    {p : Nat} {q : Param E} (hq : (S.params a)[p]? = some q) (hr : q.role ≠ .new) :
    s.ex q.entity (b.slot p) ≠ .absent := by
  have hok := (hB p q hq).2
  intro habs
  cases hrole : q.role with
  | new => exact hr hrole
  | earlier st =>
    obtain ⟨m, k, hm, hent, hbase⟩ := hb.earlier_mark a p q st hq hrole
    have h1 : decide (some (s.col m (b.slot p)) = some k) = true := hGd _ hm
    have h1' : s.col m (b.slot p) = k := by simpa using h1
    have h2 := hW.1 m (b.slot p) (by rw [hent]; exact habs)
    exact hbase (by rw [← h2]; exact h1')
  | existing =>
    rw [hrole] at hok
    have : s.ex q.entity (b.slot p) = .present := hok
    rw [habs] at this; cases this
  | parent =>
    rw [hrole] at hok
    have : s.ex q.entity (b.slot p) = .present := hok
    rw [habs] at this; cases this
  | triggered =>
    rw [hrole] at hok
    have : s.ex q.entity (b.slot p) = .present := hok
    rw [habs] at this; cases this

/-- After the step, no parameter's row is absent: a `New` row is created. -/
theorem post_bound_nonabsent {t : St E C O} (hb : S.Built) (hW : S.BaseAbsent s)
    (hB : S.BindOk (S.params a) b.slot s) (hGd : ∀ φ ∈ S.guards a, S.eval s t b.slot [] φ = true)
    {p : Nat} {q : Param E} (hq : (S.params a)[p]? = some q) :
    (S.post a b s).ex q.entity (b.slot p) ≠ .absent := by
  by_cases hr : q.role = .new
  · intro h
    obtain ⟨i, hi⟩ := List.mem_iff_getElem?.1 (hb.new_create a p q hq hr)
    have := lastW_ne_none (f := fun _ x => wex (S.params a) b.slot q.entity (b.slot p) x)
      (S.effs a) 0 i hi (by simp [wex, pent, hq])
    exact this (post_ex_absent h).2
  · exact post_ex_ne (bound_nonabsent hb hW hB hGd hq hr)

/-- **The successor commutes with `g`**: `post(g(s), g(p)) = g(post(s, p))` on `W`. -/
theorem post_comm {t : St E C O} (hwf : S.WF) (hb : S.Built) (hH : S.Hyp) {g : Perm S.n}
    (hW : S.BaseAbsent s) (hg : S.mem s g) (hB : S.BindOk (S.params a) b.slot s)
    (hGd : ∀ φ ∈ S.guards a, S.eval s t b.slot [] φ = true) :
    S.post a (S.actL g (a, b)).2 (S.act g s) = S.act g (S.post a b s) := by
  have mem_of : ∀ {i x}, (S.effs a)[i]? = some x → x ∈ S.effs a :=
    fun h => List.mem_iff_getElem?.2 ⟨_, h⟩
  apply St.ext'
  · funext e j
    obtain ⟨k, rfl⟩ : ∃ k, j = g.f (S.cls e) k := ⟨g.finv _ j, (g.right _ j).symm⟩
    rw [act_ex]
    simp only [post]
    rw [act_ex]
    congr 1
    apply lastW_congr
    intro i x _
    exact wex_mv g _ _ e k x
  · funext c j
    obtain ⟨k, rfl⟩ : ∃ k, j = g.f (S.cls (S.cent c)) k := ⟨g.finv _ j, (g.right _ j).symm⟩
    rw [act_col]
    simp only [post]
    rw [act_col]
    congr 1
    apply lastW_congr
    intro i x hx
    simp only [Nat.zero_add]
    exact wcol_mv (b := b) hg hx (hH.g2 a x (mem_of hx)) (hwf.effs a x (mem_of hx)) c k
  · funext o j
    obtain ⟨k, rfl⟩ : ∃ k, j = g.f (S.cls (S.child o)) k := ⟨g.finv _ j, (g.right _ j).symm⟩
    rw [act_own]
    simp only [post]
    rw [act_own]
    refine own_combine (r := S.rel g o) (v := s.own o k) ?_ ?_ ?_ ?_
    · apply lastW_map
      intro i x hx
      simp only [Nat.zero_add]
      exact wown_mv (b := b) hg hx (hH.g2 a x (mem_of hx)) o k
    · intro w hw hQ
      obtain ⟨i, x, _, hf⟩ := lastW_some _ _ hw
      obtain ⟨p, q, hq, he, hk⟩ := wown_some hf
      have := post_bound_nonabsent hb hW hB hGd hq
      rw [he, hk] at this
      exact this hQ
    · intro hL hP hQ
      have hW2 := hW.2 o k hP
      cases hv : lastW (fun _ x => wex (S.params a) b.slot (S.child o) k x) 0 (S.effs a) with
      | none =>
        exfalso; apply hQ
        show (lastW (fun _ x => wex (S.params a) b.slot (S.child o) k x) 0 (S.effs a)).getD
          (s.ex (S.child o) k) = .absent
        rw [hv]; exact hP
      | some v =>
        obtain ⟨i, x, _, hf⟩ := lastW_some _ _ hv
        obtain ⟨p, q, hq, he, hk, _⟩ := wex_some hf
        have hnew : q.role = .new := by
          apply Classical.byContradiction
          intro hr
          exact bound_nonabsent hb hW hB hGd hq hr (by rw [he, hk]; exact hP)
        have hc := hb.new_create a p q hq hnew
        have hp : pent (S.params a) p = some (S.child o) := pent_eq.2 ⟨q, hq, he⟩
        rcases hH.g4 a p hc o hp with ⟨_, hbase⟩ | hfix | ⟨q', hq'⟩ | ⟨vs, hvs⟩
        · rw [hW2, hbase]; rfl
        · exact rel_fix hg hfix _
        · exfalso
          obtain ⟨i', hi'⟩ := List.mem_iff_getElem?.1 hq'
          exact lastW_ne_none (f := fun i x => S.wown (S.params a) b i o k x) _ 0 i' hi'
            (by simp [wown, hp, ← hk]) hL
        · exfalso
          obtain ⟨i', hi'⟩ := List.mem_iff_getElem?.1 hvs
          exact lastW_ne_none (f := fun i x => S.wown (S.params a) b i o k x) _ 0 i' hi'
            (by simp [wown, hp, ← hk]) hL
    · intro hP hQ
      exact post_ex_ne hP hQ
  · rfl

end Step

/-- **Theorem A18, the commutation**: under well-typedness, the builder's invariants, and
(G1)–(G4) and (G6), a step from a state of `W` commutes with every `g ∈ G_s`. -/
theorem step_comm (hwf : S.WF) (hb : S.Built) (hH : S.Hyp) {s t : St E C O} {l : A × Binding}
    {g : Perm S.n} (hW : S.BaseAbsent s) (hst : S.lts.step s l t) (hg : S.mem s g) :
    S.lts.step (S.act g s) (S.actL g l) (S.act g t) := by
  obtain ⟨a, b⟩ := l
  obtain ⟨hB, hP, hGd, rfl⟩ := hst
  have mem_of : ∀ {i x}, (S.effs a)[i]? = some x → x ∈ S.effs a :=
    fun h => List.mem_iff_getElem?.2 ⟨_, h⟩
  have hpost := post_comm hwf hb hH hW hg hB hGd
  refine ⟨?_, ?_, ?_, hpost.symm⟩
  · intro p q hq
    have hmv : S.mvSlot g (S.params a) b.slot p = g.f (S.cls q.entity) (b.slot p) := mvSlot_of hq
    obtain ⟨hlt, hrole⟩ := hB p q hq
    have key : ∀ r, roleOk s q.entity (b.slot p) r →
        roleOk (S.act g s) q.entity (g.f (S.cls q.entity) (b.slot p)) r := by
      intro r h
      cases r with
      | existing => exact (act_ex S g s q.entity (b.slot p)).trans h
      | parent => exact (act_ex S g s q.entity (b.slot p)).trans h
      | triggered => exact (act_ex S g s q.entity (b.slot p)).trans h
      | new =>
        obtain ⟨habs, hlow⟩ := h
        rw [hg.2 _ _ habs]
        exact And.intro ((act_absent_iff S hg _ _).2 habs)
          (fun j hj => by rw [Ne, act_absent_iff S hg]; exact hlow j hj)
      | earlier _ => trivial
    refine ⟨?_, ?_⟩
    · show S.mvSlot g (S.params a) b.slot p < _
      rw [hmv]; exact g.f_lt hlt
    · show roleOk (S.act g s) q.entity (S.mvSlot g (S.params a) b.slot p) q.role
      rw [hmv]; exact key _ hrole
  · intro i x hx
    change (S.effs a)[i]? = some x at hx
    have h2 := hH.g2 a x (mem_of hx)
    have hw := hwf.effs a x (mem_of hx)
    have hpk := hP i x hx
    cases x
    case chooseCol c p vs =>
      have : S.mvPick g (S.effs a) b.choice i = b.choice i := by simp [mvPick, hx]
      show ∃ v, S.mvPick g (S.effs a) b.choice i = some v ∧ v ∈ vs
      rw [this]; exact hpk
    case chooseOwn o p vs =>
      have : S.mvPick g (S.effs a) b.choice i = S.rel g o (b.choice i) := by simp [mvPick, hx]
      show S.mvPick g (S.effs a) b.choice i ∈ vs
      rw [this]
      have hpk : b.choice i ∈ vs := hpk
      rcases h2 with hall | hfix
      · cases hv : b.choice i with
        | none => rw [hv] at hpk; exact hpk
        | some k =>
          simp only [rel, Option.map_some]
          by_cases hk : k < S.n (S.cls (S.par o))
          · exact hall _ (g.f_lt hk)
          · rw [g.fix _ _ (Nat.le_of_not_lt hk)]; rw [hv] at hpk; exact hpk
      · rw [rel_fix hg hfix]; exact hpk
    case trans c p arms =>
      have h1 : (S.act g s).col c (S.mvSlot g (S.params a) b.slot p) = s.col c (b.slot p) := by
        rw [mvSlot_pent hw, act_col]
      have h1' : S.mvPick g (S.effs a) b.choice i = b.choice i := by simp [mvPick, hx]
      show (tos arms ((S.act g s).col c (S.mvSlot g (S.params a) b.slot p))).isEmpty = false →
        ∃ v, S.mvPick g (S.effs a) b.choice i = some v ∧
          v ∈ tos arms ((S.act g s).col c (S.mvSlot g (S.params a) b.slot p))
      rw [h1, h1']; exact hpk
    all_goals trivial
  · intro φ hφ
    have := eval_mv g s (S.post a b s) b.slot hg (S.params a)
      (fun p q hq hr => bound_nonabsent hb hW hB hGd hq hr)
      (fun p q hq => post_bound_nonabsent hb hW hB hGd hq)
      φ [] [] (hwf.guards a φ hφ) (hH.g1 a φ hφ) (hH.g6 a φ hφ) rfl (by intro i e h; simp at h)
    exact this.trans (hGd φ hφ)

/-- A step keeps `g ∈ G_s`: a slot leaves `absent` only. This holds on every state. -/
theorem mem_step {s t : St E C O} {l : A × Binding} {g : Perm S.n} (hst : S.lts.step s l t)
    (hg : S.mem s g) : S.mem t g := by
  obtain ⟨a, b⟩ := l
  obtain ⟨_, _, _, rfl⟩ := hst
  exact ⟨hg.1, fun e k h => hg.2 e k (post_ex_absent h).1⟩

/-- `W` is closed under steps: a step writes only rows that are present after it. -/
theorem W_step (hb : S.Built) {s t : St E C O} {l : A × Binding} (hW : S.BaseAbsent s)
    (hst : S.lts.step s l t) : S.BaseAbsent t := by
  obtain ⟨a, b⟩ := l
  obtain ⟨hB, _, hGd, rfl⟩ := hst
  refine ⟨fun c k h => ?_, fun o k h => ?_⟩
  · have hs := (post_ex_absent h).1
    show (lastW (fun i x => S.wcol (S.params a) b s i c k x) 0 (S.effs a)).getD (s.col c k) =
      S.cbase c
    cases hl : lastW (fun i x => S.wcol (S.params a) b s i c k x) 0 (S.effs a) with
    | none => exact hW.1 c k hs
    | some v =>
      exfalso
      obtain ⟨i, x, _, hf⟩ := lastW_some _ _ hl
      obtain ⟨p, q, hq, he, hk⟩ := wcol_some hf
      have := post_bound_nonabsent hb hW hB hGd hq
      rw [he, hk] at this
      exact this h
  · have hs := (post_ex_absent h).1
    show (lastW (fun i x => S.wown (S.params a) b i o k x) 0 (S.effs a)).getD (s.own o k) =
      S.obase o
    cases hl : lastW (fun i x => S.wown (S.params a) b i o k x) 0 (S.effs a) with
    | none => exact hW.2 o k hs
    | some v =>
      exfalso
      obtain ⟨i, x, _, hf⟩ := lastW_some _ _ hl
      obtain ⟨p, q, hq, he, hk⟩ := wown_some hf
      have := post_bound_nonabsent hb hW hB hGd hq
      rw [he, hk] at this
      exact this h

/-- `W` is closed under every permutation: `apply` keeps an absent row's values. -/
theorem W_act {s : St E C O} {g : Perm S.n} (hW : S.BaseAbsent s) : S.BaseAbsent (S.act g s) := by
  refine ⟨fun c j h => hW.1 c _ h, fun o j h => ?_⟩
  have h' : s.ex (S.child o) (g.finv (S.cls (S.child o)) j) = .absent := h
  show (if s.ex (S.child o) (g.finv _ j) = .absent then _ else _) = _
  rw [ite_eq_left h']
  exact hW.2 o _ h'

theorem W_init {s : St E C O} (h : S.Init s) : S.BaseAbsent s := ⟨h.2.1, h.2.2.2.1⟩

/-- (G3): the initial set is closed under every `G_s`. -/
theorem init_act (hH : S.Hyp) {s : St E C O} {g : Perm S.n} (hi : S.Init s) (hg : S.mem s g) :
    S.Init (S.act g s) := by
  obtain ⟨hex, hca, hcp, hoa, hop, hnow⟩ := hi
  refine ⟨fun e j => ?_, fun c j h => hca c _ h, fun c j h => hcp c _ h, fun o j h => ?_,
    fun o j h => ?_, hnow⟩
  · show s.ex e (g.finv _ j) = _
    rw [hex]; simp only [g.finv_lt_iff]
  · have h' : s.ex (S.child o) (g.finv (S.cls (S.child o)) j) = .absent := h
    show (if s.ex (S.child o) (g.finv _ j) = .absent then _ else _) = _
    rw [ite_eq_left h']; exact hoa o _ h'
  · have hp : s.ex (S.child o) (g.finv (S.cls (S.child o)) j) = .present := h
    have hna : s.ex (S.child o) (g.finv (S.cls (S.child o)) j) ≠ .absent := by rw [hp]; decide
    have hown : (S.act g s).own o j = S.rel g o (s.own o (g.finv (S.cls (S.child o)) j)) := by
      show (if _ then _ else _) = _
      rw [ite_eq_right hna]
    have hpre : S.pre (S.child o) = true := by
      have := hex (S.child o) (g.finv (S.cls (S.child o)) j)
      rw [hp] at this
      cases hpr : S.pre (S.child o)
      · rw [hpr] at this; simp at this
      · rfl
    have h3 := hH.g3 o
    have hs := hop o _ hp
    rw [hown]
    split
    · rename_i hso
      have hs' : s.own o (g.finv (S.cls (S.child o)) j) = S.obase o := by rw [hso] at hs; exact hs
      rw [hs']
      rcases h3.2 hso hpre with ⟨_, hnone⟩ | hfix
      · rw [hnone]; rfl
      · exact rel_fix hg hfix _
    · rename_i hso
      have hs' : S.ODom o (s.own o (g.finv (S.cls (S.child o)) j)) := by rw [hso] at hs; exact hs
      cases hv : s.own o (g.finv (S.cls (S.child o)) j) with
      | none => rw [hv] at hs'; exact hs'
      | some k => rw [hv] at hs'; exact g.f_lt hs'
    · rename_i hso
      have hs' : s.own o (g.finv (S.cls (S.child o)) j) = some (g.finv (S.cls (S.child o)) j) := by
        rw [hso] at hs; exact hs
      rw [hs']
      show some (g.f (S.cls (S.par o)) (g.finv (S.cls (S.child o)) j)) = some j
      rcases h3.1 hso with hc | ⟨hfc, hfp⟩
      · rw [← hc, g.right]
      · rw [hg.1 _ _ hfp, g.finv_of_fix (hg.1 _ _ hfc)]

/-- **(G1)–(G6) give the `SymOn` of Theorem A23**, with `W` the states whose absent rows hold
base values. -/
def symOn (hwf : S.WF) (hb : S.Built) (hH : S.Hyp) : SymOn S.lts (Perm S.n) S.BaseAbsent where
  mul := Perm.mul
  inv := Perm.inv
  act := S.act
  actL := S.actL
  mem := S.mem
  act_mul := S.act_mul
  act_inv := S.act_inv
  act_inv' := S.act_inv'
  mul_mem := S.mul_mem
  inv_mem := S.inv_mem
  mem_act := S.mem_act
  step := fun hW hst hg => step_comm hwf hb hH hW hst hg
  mem_step := fun _ hst hg => mem_step hst hg
  init := fun hi hg => init_act hH hi hg
  W_init := W_init
  W_step := fun hW hst => W_step hb hW hst
  W_act := fun hW _ => W_act hW

end Sys

end RdraIsh.SymAct

namespace RdraIsh

open LTS

/-- The canonical quotient of `M` reaches the states that the quotient of `M` restricted to
`W` reaches: every stored state is an image of a state of `W`, so it is in `W`. -/
theorem SymOn.quotient_reach_iff {σ α G : Type} {M : LTS σ α} {W : σ → Prop} (h : SymOn M G W)
    (can : σ → σ) (π : σ → G) (hcan : ∀ s, can s = h.act (π s) s) (hπ : ∀ s, h.mem s (π s))
    (r : σ) : (quotient M can).Reach r ↔ (quotient (M.restrict W) can).Reach r := by
  have hW : ∀ r, (quotient M can).Reach r → W r ∧ (quotient (M.restrict W) can).Reach r := by
    intro r hr
    induction hr with
    | @init r hi =>
      obtain ⟨s, hs, rfl⟩ := hi
      refine ⟨?_, Reach.init ⟨s, hs, rfl⟩⟩
      rw [hcan]; exact h.W_act (h.W_init hs) (hπ s)
    | @step r a r' _ hst ih =>
      obtain ⟨t, hrt, rfl⟩ := hst
      have hWt := h.W_step ih.1 hrt
      refine ⟨?_, Reach.step ih.2 ⟨t, ⟨ih.1, hrt⟩, rfl⟩⟩
      rw [hcan]; exact h.W_act hWt (hπ t)
  constructor
  · exact fun hr => (hW r hr).2
  · intro hr
    induction hr with
    | init hi => exact Reach.init hi
    | @step r a r' _ hst ih =>
      obtain ⟨t, ⟨_, hrt⟩, rfl⟩ := hst
      exact Reach.step ih ⟨t, hrt, rfl⟩

namespace SymAct.Sys

variable {K E C O A : Type} [DecidableEq E] [DecidableEq C] [DecidableEq O]
  {S : Sys K E C O A}

/-- **Theorem A18's verdict sentence for invariants, from (G1)–(G6)**: for an invariant whose
value is the same on `s` and `g(s)` for every `g ∈ G_s`, the canonical quotient of the cone
system gives the plain run's verdict. -/
theorem a18_holds_iff (hwf : S.WF) (hb : S.Built) (hH : S.Hyp) (can : St E C O → St E C O)
    (π : St E C O → Perm S.n) (hcan : ∀ s, can s = S.act (π s) s) (hπ : ∀ s, S.mem s (π s))
    (P : St E C O → Prop) (hP : ∀ s g, S.mem s g → (P (S.act g s) ↔ P s)) :
    S.lts.Holds P ↔ (quotient S.lts can).Holds P := by
  let h := S.symOn hwf hb hH
  have e1 := quotient_holds_iff h.toSym can π hcan hπ P hP
  have hr := fun s => restrict_reach_iff (M := S.lts) (W := S.BaseAbsent) h.W_init h.W_step s
  constructor
  · intro hM r hq
    exact e1.1 (fun s hs => hM s ((hr s).1 hs)) r ((h.quotient_reach_iff can π hcan hπ r).1 hq)
  · intro hQ s hs
    exact e1.2 (fun r hq => hQ r ((h.quotient_reach_iff can π hcan hπ r).2 hq)) s ((hr s).2 hs)

end SymAct.Sys

end RdraIsh
