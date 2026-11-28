import RdraIsh.Split
import RdraIsh.Stages

/-!
# Definition 4 over actions: the extended sub-lifecycle `X` (cones.md, Definition 4)

`Split.lean` splits the action-level cone system of `Actions.lean` into `T_v × S_v`. This file
builds, from the same model and a split, the extended sub-lifecycle `X` of
`reduce/extended.rs::Extended::build`, as a transition system over `S_v` valuations and input
bytes:

- **Roles** (`Rows.cand`, `candidate_slots`): `existing`, `parent` and `triggered` bind a
  present row, `new` the lowest absent one, `earlier` any slot (its run guard is an ordinary
  conjunct). A role is a guard conjunct on the existence of the parameter's entity
  (`roleConj`); `Shape` asks that an action's guard is its role conjuncts followed by its
  other conjuncts `og`.
- **Sides** (`side`): a parameter's entity is on `S` (its existence is in `S_v`), an input
  (its existence is an input), a stub (`restrict_to` keeps it as an entity of its existence
  alone, which `X` does not hold) or dropped.
- **The input bytes** (`Inp`): the raw inputs and one bit per key `K` (`Reduction::input_bytes`,
  the raw inputs followed by the bits `push_bits` computes, `bitOf`). `inp t` reads them off a
  `T_v` valuation.
- **`xval`**: the valuation an `X` state stands for. `S_v` and the inputs come from the state;
  every other variable, a stub's existence among them, takes `dflt` (the value of `X`'s first
  initial state, which `Reduction::key` appends).
- **The restriction** (`xrole`, `xguard`): `restrict_to(S_v ∪ inputs)` with havoc off keeps a
  parameter on `S` or an input with its role and drops one on a dropped entity, and a `new`
  one on a stub. `fold_param` gives a fold's key on an input or a stub the role
  `Earlier { step: 0 }`, and `bind_stub_params` gives the same role to a parameter on a stub
  whose role needs a present row, when the action is a rise or a folded `S_a` action
  (`bindStub`; the flag `bindStubs` turns it off, to show what it is for). `X` keeps the
  conjuncts `kept` names (those reading `S_v ∪ inputs` and naming no dropped parameter) and
  reads the bit at the key's slot in place of the folded ones.
- **`X`** (`ext`): a silent edge is a step of an `S_a` action under `xguard`; a rise is a
  step of a `T_a` action that writes an input (`riseAct`) under `xguard`, which writes the
  raw inputs and keeps the bits, or the free flip of one bit (`reduce/extended.rs::flips`).
  `X` holds no action that writes nothing: `restrict_to` drops an action whose effects write
  nothing in `S_v ∪ inputs`, and such an action outside `T_a` is in neither `T_a` nor `S_a`.
  `coneW` is the cone system without those actions' steps, which stutter (`Local.idle`);
  `coneW_reach_iff` shows it reaches what `cone` reaches.
- **`stages`** (`Reduction::stages`): the raw inputs first, when they change, then one stage
  per bit slot that changes, in the order of `keys`.

What this file takes as given rather than building from the code:

- `X`'s initial states are the projections of the cone's (`ext.init`): that `restrict_to`'s
  initial states are those projections (Lemma 2's argument) and that every bit starts where
  the cone starts (`fold_initial`) are not derived.
- One bit per folded action: a group of folds sharing a bit (`Signature::Param`, the
  disjunction of `push_bits`) is not modelled, nor node folds (`FoldKey::Linked`, which add a
  parameter), nor the renumbering of parameters (`Extended::cone_params`, step (b)): a binding
  of `X` keeps the cone's parameter positions.
-/

namespace RdraIsh.Act

open LTS

variable {V E A Val K : Type}

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

/-- How an existence value holds its rows: which slots are present, and which slot is the
lowest absent one. -/
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

/-- The guard conjunct of a parameter's role, at position `j`. -/
def roleConj (R : Rows Val) (ex : E → V) (j : Nat) (p : Param E) : Conj V Val where
  rd := if p.role.readsRow then [ex p.entity] else []
  holds b σ := R.cand p.role (σ (ex p.entity)) (b.slot j)

/-- The role conjuncts of a parameter list, numbered from `n`. -/
def roleConjs (R : Rows Val) (ex : E → V) : Nat → List (Param E) → List (Conj V Val)
  | _, [] => []
  | n, p :: ps => roleConj R ex n p :: roleConjs R ex (n + 1) ps

theorem mem_roleConjs (R : Rows Val) (ex : E → V) :
    ∀ (n : Nat) (ps : List (Param E)) (j : Nat) (p : Param E), ps[j]? = some p →
      roleConj R ex (n + j) p ∈ roleConjs R ex n ps
  | _, [], _, _, h => by simp at h
  | n, q :: ps, 0, p, h => by
    simp only [List.getElem?_cons_zero, Option.some.injEq] at h
    subst h
    exact List.mem_cons_self
  | n, q :: ps, j + 1, p, h => by
    simp only [List.getElem?_cons_succ] at h
    have := mem_roleConjs R ex (n + 1) ps j p h
    rw [show n + (j + 1) = n + 1 + j by omega]
    exact List.mem_cons_of_mem _ this

/-- A fold of an `S_a` action (`FoldKey::Param`): the key's position, the folded guards, and the
bit each slot of the key reads. -/
structure Fold (V Val K : Type) where
  key : Nat
  folded : List (Conj V Val)
  bit : Nat → K

/-- Two bindings agree outside `P`, and on the choices. -/
def BindAgree (P : Nat → Prop) (b b' : Binding) : Prop :=
  (∀ j, ¬ P j → b.slot j = b'.slot j) ∧ b.choice = b'.choice

/-- The side of a parameter's entity. -/
inductive Side where
  | s
  | input
  | stub
  | dropped
  deriving DecidableEq

/-- The data `Extended::build` starts from: the split, the inputs, the entities' existences,
the stubs, the rows' semantics, the guard conjuncts besides the roles, which of them `X`
keeps, the folds, and the bits. -/
structure Ext (M : Model V E A Val) (K : Type) where
  tv : V → Bool
  ta : A → Bool
  inputs : V → Bool
  inputs_t : ∀ v, inputs v = true → tv v = true
  ex : E → V
  stub : E → Bool
  rows : Rows Val
  og : A → List (Conj V Val)
  kept : A → Conj V Val → Prop
  fold : A → Option (Fold V Val K)
  keys : List K
  keys_all : ∀ k, k ∈ keys
  bitOf : K → TV tv (Val := Val) → Bool
  dflt : V → Val
  bindStubs : Bool

namespace Ext

variable {M : Model V E A Val} (D : Ext M K)

/-- The raw inputs. -/
abbrev Raw := {v : V // D.inputs v = true} → Val

/-- The input bytes: the raw inputs and the bits. -/
abbrev Inp := D.Raw × (K → Bool)

/-- A state of `X`. -/
abbrev XS := SV D.tv (Val := Val) × D.Inp

/-- The input bytes of a `T_v` valuation (`Reduction::input_bytes`). -/
def inp (t : TV D.tv (Val := Val)) : D.Inp :=
  (fun v => t ⟨v.1, D.inputs_t v.1 v.2⟩, fun k => D.bitOf k t)

/-- The valuation an `X` state stands for. -/
def xval (x : D.XS) (v : V) : Val :=
  if h : D.tv v = false then x.1 ⟨v, h⟩
  else if h' : D.inputs v = true then x.2.1 ⟨v, h'⟩ else D.dflt v

/-- The side of an entity. -/
def side (e : E) : Side :=
  if D.tv (D.ex e) = false then .s
  else if D.inputs (D.ex e) = true then .input
  else if D.stub e = true then .stub else .dropped

/-- Position `j` is the key of `a`'s fold. -/
def isKey (a : A) (j : Nat) : Bool :=
  match D.fold a with
  | some F => decide (F.key = j)
  | none => false

/-- `bind_stub_params` rebinds a stub parameter of a rise or of a folded `S_a` action. -/
def bindStub (a : A) : Bool := D.bindStubs && (D.ta a || (D.fold a).isSome)

/-- The role `X` gives parameter `j` of `a`; `none` when the restriction drops it. -/
def xrole (a : A) (j : Nat) (p : Param E) : Option Role :=
  match D.side p.entity with
  | .s => some p.role
  | .input => if D.isKey a j then some (.earlier 0) else some p.role
  | .stub =>
    if D.isKey a j then some (.earlier 0) else
    match p.role with
    | .new => none
    | r => if r.needsPresent && D.bindStub a then some (.earlier 0) else some r
  | .dropped => none

/-- The guard of `a` in `X` at binding `b`: the kept parameters' roles, the kept conjuncts,
and the bit at the key's slot. -/
def xguard (a : A) (b : Binding) (x : D.XS) : Prop :=
  (∀ j p r, (M.params a)[j]? = some p → D.xrole a j p = some r →
    D.rows.cand r (D.xval x (D.ex p.entity)) (b.slot j)) ∧
  (∀ g ∈ D.og a, D.kept a g → g.holds b (D.xval x)) ∧
  (∀ F, D.fold a = some F → x.2.2 (F.bit (b.slot F.key)) = true)

/-- A rise action: a `T_a` action that writes an input, which `restrict_to` keeps. -/
def riseAct (a : A) : Prop := D.ta a = true ∧ ∃ v ∈ M.wr a, D.inputs v = true

/-- A bit set to a value. -/
def upd [DecidableEq K] (β : K → Bool) (k : K) (v : Bool) : K → Bool :=
  fun k' => if k' = k then v else β k'

/-- The labels of `X`: a silent edge by an action and binding, a rise by one, or a flip. -/
abbrev XL := (A × Binding) ⊕ ((A × Binding) ⊕ (K × Bool))

/-- The extended sub-lifecycle `X` (Definition 4). -/
def ext [DecidableEq K] : LTS D.XS (A × Binding ⊕ ((A × Binding) ⊕ (K × Bool))) where
  init x := ∃ p, M.init (join D.tv p) ∧ x = (p.2, D.inp p.1)
  step x l y := match l with
    | .inl (a, b) => Sa M D.ta a ∧ D.xguard a b x ∧
        y = ((fun v => M.eff a b (D.xval x) v.1), x.2)
    | .inr (.inl (a, b)) => D.riseAct a ∧ D.xguard a b x ∧
        y = (x.1, ((fun v => M.eff a b (D.xval x) v.1), x.2.2))
    | .inr (.inr (k, v)) => x.2.2 k = !v ∧ y = (x.1, (x.2.1, upd x.2.2 k v))

/-- The flips from bits `β` to `β'`, over the keys `ks` in order, at raw inputs `r`. -/
def flips [DecidableEq K] (r : D.Raw) : (K → Bool) → (K → Bool) → List K → List D.Inp
  | _, _, [] => []
  | β, β', k :: ks =>
    if β k = β' k then flips r β β' ks
    else (r, upd β k (β' k)) :: flips r (upd β k (β' k)) β' ks

open Classical in
/-- `Reduction::stages`: the raw inputs first, when they change, then each bit that changes. -/
noncomputable def stages [DecidableEq K] (i i' : D.Inp) : List D.Inp :=
  (if i.1 = i'.1 then [] else [(i'.1, i.2)]) ++ D.flips i'.1 i.2 i'.2 D.keys

/-- Position `j` of `a` is a parameter on a `T` entity (`Fold::t_params`). -/
def tParam (a : A) (j : Nat) : Prop :=
  ∃ p, (M.params a)[j]? = some p ∧ D.tv (D.ex p.entity) = true

/-- Position `j` of `a` is a `T` parameter of its fold other than the key: the positions a
silent edge of `X` does not fix. -/
def free (a : A) (j : Nat) : Prop := ∃ F, D.fold a = some F ∧ D.tParam a j ∧ j ≠ F.key

/-- The edge's action stands for the cone's `S_a` action: the same action, with the same
binding outside the fold's free `T` parameters (Lemma 6, "Parameters"). -/
def stands (l : SaL D.ta) (l' : A × Binding) : Prop :=
  l.1.1 = l'.1 ∧ BindAgree (D.free l.1.1) l.1.2 l'.2

/-- What `Fold::holds` checks of a binding: each `T` parameter whose role needs a present row
is on one, and every folded guard holds. -/
def FoldConds (a : A) (F : Fold V Val K) (b : Binding) (σ : V → Val) : Prop :=
  (∀ j p, (M.params a)[j]? = some p → D.tv (D.ex p.entity) = true →
    p.role.needsPresent = true → D.rows.present (σ (D.ex p.entity)) (b.slot j)) ∧
  ∀ g ∈ F.folded, g.holds b σ

end Ext

/-- Two valuations agree on a set of variables. -/
def AgreeOn (P : V → Prop) (σ τ : V → Val) : Prop := ∀ v, P v → σ v = τ v

/-- The cone system without the steps of the actions outside `T_a` that write nothing: its
`.inr` steps are the `S_a` steps. -/
def coneW (M : Model V E A Val) (tv : V → Bool) (ta : A → Bool) :
    LTS (TV tv (Val := Val) × SV tv (Val := Val)) (TaL ta ⊕ SaL ta) where
  init := (cone M tv ta).init
  step p l q := match l with
    | .inl l => (cone M tv ta).step p (.inl l) q
    | .inr l => M.wr l.1.1 ≠ [] ∧ (cone M tv ta).step p (.inr l) q

/-- What Lemma 6 needs of the code, for the data `D` and a set `obs`. Each field is a fact
about what an action's conjuncts and effects read or name, or about what a fold bit computes:

- `local_`, `lemma5`: the locality of `Rd` and `Wr` (`Local`) and Lemma 5 (a)(b);
- `reads`: Lemma 5 (c), with the folded reads those of the folded guards;
- `shape`: an action's guard is its role conjuncts followed by the conjuncts `og`;
- `new_writes`: a `new` parameter comes with its `Create`, which writes its entity's existence;
- `kept_rd`: a conjunct `X` keeps reads only `S_v ∪ inputs`;
- `cover`: every conjunct of an `S_a` action besides its roles is kept or folded;
- `rise_eff`: step (c), a `T_a` action's value on an input depends only on `S_v ∪ inputs`;
- `fold_sa`, `folded_og`: a fold belongs to an `S_a` action, and its folded guards are
  conjuncts of it;
- `exact`: step (a), `Fold::holds` computes the bit exactly: the bit at slot `c` holds iff
  some binding with the key at `c` meets `FoldConds`;
- `folded_names`: the folded guards name only `T` parameters and the key (`fold_of`'s
  `s_keys`);
- `other_names`, `eff_names`: no other conjunct and no effect names a `T` parameter but the
  key (`fold_of`'s link guards and `touched_t`);
- `eff_s`: a folded action's effect reads only `S_v ∪ inputs` (for an unfolded action,
  `Local` and Lemma 5 (c) give it);
- `bind`: `bind_stub_params` runs. -/
structure Lemma6Hyp {M : Model V E A Val} [DecidableEq K] (D : Ext M K) (obs : V → Prop) :
    Prop where
  local_ : Local M
  lemma5 : Lemma5ab M D.tv D.ta obs
  reads : ∀ a, Sa M D.ta a → ∀ v ∈ M.rd a,
    D.tv v = false ∨ D.inputs v = true ∨ ∃ F, D.fold a = some F ∧ ∃ g ∈ F.folded, v ∈ g.rd
  shape : ∀ a, M.guards a = roleConjs D.rows D.ex 0 (M.params a) ++ D.og a
  new_writes : ∀ a (j : Nat) (p : Param E), (M.params a)[j]? = some p → p.role = .new → D.ex p.entity ∈ M.wr a
  kept_rd : ∀ a g, D.kept a g → ∀ v ∈ g.rd, D.tv v = false ∨ D.inputs v = true
  cover : ∀ a, Sa M D.ta a → ∀ g ∈ D.og a, D.kept a g ∨ ∃ F, D.fold a = some F ∧ g ∈ F.folded
  rise_eff : ∀ a b σ τ, D.ta a = true →
    AgreeOn (fun v => D.tv v = false ∨ D.inputs v = true) σ τ →
    ∀ v, D.inputs v = true → M.eff a b σ v = M.eff a b τ v
  fold_sa : ∀ a F, D.fold a = some F → Sa M D.ta a
  folded_og : ∀ a F, D.fold a = some F → ∀ g ∈ F.folded, g ∈ D.og a
  exact : ∀ a F, D.fold a = some F → ∀ t s c,
    D.bitOf (F.bit c) t = true ↔ ∃ b : Binding, b.slot F.key = c ∧ D.FoldConds a F b (join D.tv (t, s))
  folded_names : ∀ a F, D.fold a = some F → ∀ g ∈ F.folded, ∀ (b b' : Binding) σ,
    (∀ j, (D.tParam a j ∨ j = F.key) → b.slot j = b'.slot j) → g.holds b σ → g.holds b' σ
  other_names : ∀ a F, D.fold a = some F → ∀ g ∈ D.og a, g ∉ F.folded → ∀ b b' σ,
    BindAgree (D.free a) b b' → g.holds b σ → g.holds b' σ
  eff_names : ∀ a F, D.fold a = some F → ∀ b b' σ, BindAgree (D.free a) b b' →
    ∀ v ∈ M.wr a, M.eff a b σ v = M.eff a b' σ v
  eff_s : ∀ a F, D.fold a = some F → ∀ b σ τ,
    AgreeOn (fun v => D.tv v = false ∨ D.inputs v = true) σ τ →
    ∀ v ∈ M.wr a, M.eff a b σ v = M.eff a b τ v
  bind : D.bindStubs = true

end RdraIsh.Act
