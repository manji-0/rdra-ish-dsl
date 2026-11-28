import RdraIsh.SymActions
import RdraIsh.Lemma6

/-!
# The slot model as an action model (cones.md, Definitions 3 and 4, Theorem A18)

`SymAct.Sys` gives the actions of a cone system a syntax; `Act.Model` keeps guards and effects
opaque behind a read set `rd` and a write set `wr`, and Lemmas 5 and 6 assume that the two
sets bound what the guards and effects look at (`Act.Local`). This file reads an `Act.Model`
off a `SymAct.Sys` (`Sys.model`), with `rd` and `wr` computed from the syntax as the code
computes them, and proves `Local` of it. So one concrete model carries both the hypotheses of
A18 ((G1)–(G6), on `S`) and those of Lemmas 5 and 6 that concern locality.

- **Variables** (`Var`): an existence per entity, a data column, an owner, the clock. A
  variable is one of the code's `VarId`s, and its value (`Val`) holds every slot of it.
  `enc` reads a state as a valuation and `dec` reads it back (`dec_enc`).
- **Reads** (`Sys.rd`), after `TransitionSystem::action_reads` and `copy_reads`: the existence
  of every `Existing`, `Parent` and `Triggered` parameter, every variable a guard's terms read
  (`term_reads`: a term at a parameter, at its slot in the successor and at a quantifier
  slot all read the variable), the existence of every quantified entity (`pred_reads`), the
  variable of a `Transition` and the clock of a `Tick`.
- **Writes** (`Sys.wr`), after `TransitionSystem::effect_writes`: a create the existence, a
  delete the existence with every column and owner of its entity, a write or choice its
  variable, a tick the clock.
- **Guards** (`Sys.og`): the role conjuncts of `Act.roleConjs` with the semantics `rows`,
  then one conjunct for the slot bounds, one for the choices' picks, and one per guard
  predicate, evaluated against the successor (a guard may read `At::Next`).
- **Effects** (`Sys.eff`): a written variable takes its value in `post`, every other keeps
  its own. On the valuations `enc s` this is `post` (`eff_enc`).

`local_` proves `Local` from `Built` alone (a `New` parameter's existence, which its role
conjunct reads, is written by its create): each variable's value after `post` depends only on
that variable before it, and a predicate's value only on the variables of `predRd`.
`step_enc` and `reach_enc` show that `model.lts` on `enc` states is `S.lts`.
-/

namespace RdraIsh.SymAct

open LTS
open Act (Role Param Binding Rows Conj Model Local Agree roleConj roleConjs)

/-- A variable of the slot model: an entity's existence, a data column, an owner, the
clock. -/
inductive Var (E C O : Type) where
  | ex (e : E)
  | col (c : C)
  | own (o : O)
  | now
  deriving DecidableEq

/-- The value of a variable: its value at every slot, or the clock. -/
inductive Val where
  | ex (f : Nat → Ex)
  | col (f : Nat → Nat)
  | own (f : Nat → Option Nat)
  | now (n : Nat)

namespace Val

/-- The slots of an existence. -/
def exOf : Val → Nat → Ex
  | .ex f => f
  | _ => fun _ => .absent

/-- The slots of a column. -/
def colOf : Val → Nat → Nat
  | .col f => f
  | _ => fun _ => 0

/-- The slots of an owner. -/
def ownOf : Val → Nat → Option Nat
  | .own f => f
  | _ => fun _ => none

/-- The clock. -/
def nowOf : Val → Nat
  | .now n => n
  | _ => 0

end Val

variable {K E C O A : Type}

/-- A state as a valuation. -/
def enc (s : St E C O) : Var E C O → Val
  | .ex e => .ex (s.ex e)
  | .col c => .col (s.col c)
  | .own o => .own (s.own o)
  | .now => .now s.now

/-- A valuation as a state; a value of the wrong kind reads as absent, `0` or `null`. -/
def dec (σ : Var E C O → Val) : St E C O where
  ex e := (σ (.ex e)).exOf
  col c := (σ (.col c)).colOf
  own o := (σ (.own o)).ownOf
  now := (σ .now).nowOf

theorem dec_enc (s : St E C O) : dec (enc s) = s := rfl

/-- `enc (dec σ)` at `v` depends only on `σ v`. -/
theorem enc_dec_congr {σ τ : Var E C O → Val} {v : Var E C O} (h : σ v = τ v) :
    enc (dec σ) v = enc (dec τ) v := by
  cases v <;> simp only [enc, dec, h]

/-- Two states agree on a list of variables. -/
def VAgree (l : List (Var E C O)) (s s' : St E C O) : Prop := ∀ v ∈ l, enc s v = enc s' v

section VAgree

variable {l : List (Var E C O)} {s s' : St E C O}

theorem VAgree.ex (h : VAgree l s s') {e} (he : Var.ex e ∈ l) : s.ex e = s'.ex e :=
  Val.ex.inj (h _ he)

theorem VAgree.col (h : VAgree l s s') {c} (hc : Var.col c ∈ l) : s.col c = s'.col c :=
  Val.col.inj (h _ hc)

theorem VAgree.own (h : VAgree l s s') {o} (ho : Var.own o ∈ l) : s.own o = s'.own o :=
  Val.own.inj (h _ ho)

theorem VAgree.now (h : VAgree l s s') (hn : Var.now ∈ l) : s.now = s'.now :=
  Val.now.inj (h _ hn)

theorem VAgree.left {l₁ l₂ : List (Var E C O)} (h : VAgree (l₁ ++ l₂) s s') : VAgree l₁ s s' :=
  fun v hv => h v (List.mem_append_left _ hv)

theorem VAgree.right {l₁ l₂ : List (Var E C O)} (h : VAgree (l₁ ++ l₂) s s') : VAgree l₂ s s' :=
  fun v hv => h v (List.mem_append_right _ hv)

theorem VAgree.tail {v : Var E C O} (h : VAgree (v :: l) s s') : VAgree l s s' :=
  fun w hw => h w (List.mem_cons_of_mem _ hw)

theorem VAgree.of_dec {σ τ : Var E C O → Val} (h : Agree l σ τ) : VAgree l (dec σ) (dec τ) :=
  fun _ hv => enc_dec_congr (h _ hv)

end VAgree

/-! ## What the syntax reads and writes -/

/-- `term_reads`: the variable a term reads, at a parameter, in the successor or at a
quantifier's slot alike, and the clock. -/
def termRd : Term E C O → List (Var E C O)
  | .col c _ => [.col c]
  | .own o _ => [.own o]
  | .now => [.now]
  | _ => []

/-- `pred_reads`: every term's reads, and the existence of every quantified entity. -/
def predRd : Pred E C O → List (Var E C O)
  | .cmp x _ y => termRd x ++ termRd y
  | .inn x ys => termRd x ++ ys.flatMap termRd
  | .not p => predRd p
  | .and p q => predRd p ++ predRd q
  | .or p q => predRd p ++ predRd q
  | .all e p => .ex e :: predRd p
  | .any e p => .ex e :: predRd p
  | .count e p _ _ => .ex e :: predRd p
  | .tt => []
  | .ff => []

/-- The reads of an effect in `plain_reads`: a `Transition` reads its variable, a `Tick` the
clock. -/
def effRd : Eff C O → List (Var E C O)
  | .trans c _ _ => [.col c]
  | .tick => [.now]
  | _ => []

/-- The existences the roles read: those of `Existing`, `Parent` and `Triggered` parameters.
Taking a `New` slot is not a read (`action_reads`), and an `Earlier` slot is selected by a
guard. -/
def paramRd (ps : List (Param E)) : List (Var E C O) :=
  ps.filterMap fun q => if q.role.needsPresent then some (.ex q.entity) else none

namespace Sys

variable (S : Sys K E C O A) [DecidableEq E] [DecidableEq C] [DecidableEq O]

/-- `effect_writes`: a create writes the existence, a delete the existence and every column
and owner of its entity (`LC`, `LO` list the columns and owners), a write or a choice its
variable, a tick the clock. -/
def effWr (LC : List C) (LO : List O) (ps : List (Param E)) : Eff C O → List (Var E C O)
  | .create p => match pent ps p with
    | some e => [.ex e]
    | none => []
  | .delete p => match pent ps p with
    | some e => .ex e :: ((LC.filter fun c => decide (S.cent c = e)).map .col ++
        (LO.filter fun o => decide (S.child o = e)).map .own)
    | none => []
  | .setCol c _ _ => [.col c]
  | .copyCol c _ _ => [.col c]
  | .chooseCol c _ _ => [.col c]
  | .trans c _ _ => [.col c]
  | .setOwn o _ _ => [.own o]
  | .setOwnLit o _ _ => [.own o]
  | .chooseOwn o _ _ => [.own o]
  | .tick => [.now]

/-- The read set of an action (`action_reads` with `copy_reads`). -/
def rd (a : A) : List (Var E C O) :=
  paramRd (S.params a) ++ (S.guards a).flatMap predRd ++ (S.effs a).flatMap effRd

/-- The write set of an action (`action_writes`). -/
def wr (LC : List C) (LO : List O) (a : A) : List (Var E C O) :=
  (S.effs a).flatMap (S.effWr LC LO (S.params a))

/-- The semantics of `Role` on valuations: `rows` on the slots of an existence. -/
def rowsV : Rows Val where
  present v k := v.exOf k = .present
  lowestAbsent v k := v.exOf k = .absent ∧ ∀ j < k, v.exOf j ≠ .absent

theorem rowsV_cand (r : Role) (v : Val) (k : Nat) : rowsV.cand r v k ↔ rows.cand r v.exOf k := by
  cases r <;> exact Iff.rfl

/-- The conjunct that bounds every parameter's slot by its class's count. It reads
nothing. -/
def boundConj (a : A) : Conj (Var E C O) Val where
  rd := []
  holds b _ := ∀ p q, (S.params a)[p]? = some q → b.slot p < S.n (S.cls q.entity)

/-- The conjunct that every choice picks one of its values. -/
def picksConj (a : A) : Conj (Var E C O) Val where
  rd := (S.effs a).flatMap effRd
  holds b σ := PicksOk (S.effs a) (dec σ) b

/-- A guard predicate as a conjunct, evaluated against the successor. -/
def predConj (a : A) (φ : Pred E C O) : Conj (Var E C O) Val where
  rd := predRd φ
  holds b σ := S.eval (dec σ) (S.post a b (dec σ)) b.slot [] φ = true

/-- The conjuncts of an action besides its roles. -/
def og (a : A) : List (Conj (Var E C O) Val) :=
  S.boundConj a :: S.picksConj a :: (S.guards a).map (S.predConj a)

/-- The effect on valuations: a written variable takes its value in `post`. -/
def eff (LC : List C) (LO : List O) (a : A) (b : Binding) (σ : Var E C O → Val) :
    Var E C O → Val :=
  fun v => if v ∈ S.wr LC LO a then enc (S.post a b (dec σ)) v else σ v

/-- **The slot model as an action model.** `LE`, `LC`, `LO` and `LA` list the entities,
columns, owners and actions. -/
def model (LE : List E) (LC : List C) (LO : List O) (LA : List A)
    (hE : ∀ e, e ∈ LE) (hC : ∀ c, c ∈ LC) (hO : ∀ o, o ∈ LO) (hA : ∀ a, a ∈ LA) :
    Model (Var E C O) E A Val where
  vars := LE.map .ex ++ LC.map .col ++ LO.map .own ++ [.now]
  acts := LA
  vars_all v := by
    cases v with
    | ex e => simp [hE e]
    | col c => simp [hC c]
    | own o => simp [hO o]
    | now => simp
  acts_all := hA
  own
    | .col c => some (.ex (S.cent c))
    | .own o => some (.ex (S.child o))
    | _ => none
  existences
    | .col c => [.ex (S.cent c)]
    | .own o => [.ex (S.child o), .ex (S.par o)]
    | _ => []
  counted _ := false
  params := S.params
  guards a := roleConjs rowsV Var.ex 0 (S.params a) ++ S.og a
  rd := S.rd
  wr := S.wr LC LO
  eff := S.eff LC LO
  init σ := ∃ s, S.Init s ∧ σ = enc s

end Sys

/-! ## Locality -/

theorem any_congr_mem {γ : Type} {l : List γ} {f g : γ → Bool} (h : ∀ x ∈ l, f x = g x) :
    l.any f = l.any g := by
  induction l with
  | nil => rfl
  | cons x xs ih =>
    simp only [List.any_cons]
    rw [h x List.mem_cons_self, ih fun y hy => h y (List.mem_cons_of_mem _ hy)]

theorem tval_congr {s t s' t' : St E C O} (b : Nat → Nat) (env : List Nat) (x : Term E C O)
    (hs : VAgree (termRd x) s s') (ht : VAgree (termRd x) t t') :
    tval s t b env x = tval s' t' b env x := by
  cases x with
  | col c r =>
    cases r <;> simp only [tval, stOf, hs.col (c := c) (List.mem_singleton_self _),
      ht.col (c := c) (List.mem_singleton_self _)]
  | own o r =>
    cases r <;> simp only [tval, stOf, hs.own (o := o) (List.mem_singleton_self _),
      ht.own (o := o) (List.mem_singleton_self _)]
  | now => simp only [tval, hs.now (List.mem_singleton_self _)]
  | index => rfl
  | lit => rfl
  | null => rfl

namespace Sys

variable (S : Sys K E C O A)

/-- **A predicate depends only on what `predRd` names**, in the pre-state and in the
successor. -/
theorem eval_congr {s t s' t' : St E C O} (b : Nat → Nat) :
    ∀ (φ : Pred E C O) (env : List Nat), VAgree (predRd φ) s s' → VAgree (predRd φ) t t' →
      S.eval s t b env φ = S.eval s' t' b env φ
  | .tt, _, _, _ => rfl
  | .ff, _, _, _ => rfl
  | .cmp x op y, env, hs, ht => by
    simp only [eval, tval_congr b env x hs.left ht.left, tval_congr b env y hs.right ht.right]
  | .inn x ys, env, hs, ht => by
    simp only [eval, tval_congr b env x hs.left ht.left]
    exact any_congr_mem fun y hy => by
      have hsub : ∀ {u u' : St E C O}, VAgree (predRd (.inn x ys)) u u' → VAgree (termRd y) u u' :=
        fun h v hv => h v (List.mem_append_right _ (List.mem_flatMap.2 ⟨y, hy, hv⟩))
      rw [tval_congr b env y (hsub hs) (hsub ht)]
  | .not p, env, hs, ht => by simp only [eval, eval_congr b p env hs ht]
  | .and p q, env, hs, ht => by
    simp only [eval, eval_congr b p env hs.left ht.left, eval_congr b q env hs.right ht.right]
  | .or p q, env, hs, ht => by
    simp only [eval, eval_congr b p env hs.left ht.left, eval_congr b q env hs.right ht.right]
  | .all e p, env, hs, ht => by
    have hp := fun k => eval_congr b p (k :: env) hs.tail ht.tail
    simp only [eval, isPresent, hs.ex List.mem_cons_self, hp]
  | .any e p, env, hs, ht => by
    have hp := fun k => eval_congr b p (k :: env) hs.tail ht.tail
    simp only [eval, isPresent, hs.ex List.mem_cons_self, hp]
  | .count e p op m, env, hs, ht => by
    have hp := fun k => eval_congr b p (k :: env) hs.tail ht.tail
    simp only [eval, isPresent, hs.ex List.mem_cons_self, hp]

variable [DecidableEq E] [DecidableEq C] [DecidableEq O]

theorem wcol_congr {ps : List (Param E)} {b : Binding} {s s' : St E C O} {i : Nat} {c : C}
    {k : Nat} (h : s.col c = s'.col c) (x : Eff C O) :
    S.wcol ps b s i c k x = S.wcol ps b s' i c k x := by
  cases x <;> simp only [wcol, h]

theorem wnow_congr {s s' : St E C O} (h : s.now = s'.now) (x : Eff C O) :
    wnow s x = wnow s' x := by
  cases x <;> simp only [wnow, h]

/-- **`post` is pointwise**: a variable's value after the step depends only on its value
before it (and the binding). -/
theorem post_pt (a : A) (b : Binding) {s s' : St E C O} {v : Var E C O}
    (h : enc s v = enc s' v) : enc (S.post a b s) v = enc (S.post a b s') v := by
  cases v with
  | ex e =>
    have he : s.ex e = s'.ex e := Val.ex.inj h
    simp only [enc, post, he]
  | col c =>
    have hc : s.col c = s'.col c := Val.col.inj h
    simp only [enc, post, hc, wcol_congr S hc]
  | own o =>
    have ho : s.own o = s'.own o := Val.own.inj h
    simp only [enc, post, ho]
  | now =>
    have hn : s.now = s'.now := Val.now.inj h
    simp only [enc, post, hn, wnow_congr hn]

theorem post_agree (a : A) (b : Binding) {l : List (Var E C O)} {s s' : St E C O}
    (h : VAgree l s s') : VAgree l (S.post a b s) (S.post a b s') :=
  fun v hv => S.post_pt a b (h v hv)

end Sys

/-! ## The frame: `post` keeps what `wr` leaves out -/

theorem lastW_none {β : Type} {f : Nat → Eff C O → Option β} :
    ∀ (xs : List (Eff C O)) (j : Nat), (∀ i x, xs[i]? = some x → f (j + i) x = none) →
      lastW f j xs = none
  | [], _, _ => rfl
  | x :: xs, j, h => by
    have h0 := h 0 x rfl
    rw [Nat.add_zero] at h0
    have ih := lastW_none (f := f) xs (j + 1) fun i y hy => by
      have := h (i + 1) y (by simpa using hy)
      rwa [show j + (i + 1) = j + 1 + i by omega] at this
    simp only [lastW, ih, h0, Option.or]

namespace Sys

variable (S : Sys K E C O A) [DecidableEq E] [DecidableEq C] [DecidableEq O]
  (LC : List C) (LO : List O)

theorem mem_wr {a : A} {x : Eff C O} {v : Var E C O} (hx : x ∈ S.effs a)
    (hv : v ∈ S.effWr LC LO (S.params a) x) : v ∈ S.wr LC LO a :=
  List.mem_flatMap.2 ⟨x, hx, hv⟩

variable {LC LO}

/-- **The frame**: a variable outside `wr` keeps its value. -/
theorem post_frame (hC : ∀ c, c ∈ LC) (a : A) (b : Binding) (s : St E C O)
    {v : Var E C O} (hv : v ∉ S.wr LC LO a) : enc (S.post a b s) v = enc s v := by
  have mem_of : ∀ {i x}, (S.effs a)[i]? = some x → x ∈ S.effs a :=
    fun h => List.mem_iff_getElem?.2 ⟨_, h⟩
  cases v with
  | ex e =>
    show Val.ex _ = Val.ex _
    congr 1
    funext k
    show Option.getD (lastW _ 0 (S.effs a)) _ = _
    rw [lastW_none (S.effs a) 0 fun i x hx => ?_]
    · rfl
    cases x <;> simp only [wex] <;> (try rfl) <;> rename_i p <;>
      (split
       · rename_i hpe
         refine absurd (S.mem_wr LC LO (mem_of hx) ?_) hv
         simp only [effWr, hpe.1, List.mem_cons, true_or]
       · rfl)
  | col c =>
    show Val.col _ = Val.col _
    congr 1
    funext k
    show Option.getD (lastW _ 0 (S.effs a)) _ = _
    rw [lastW_none (S.effs a) 0 fun i x hx => ?_]
    · rfl
    have hw : ∀ y, y ∈ S.effWr LC LO (S.params a) x → y = .col c → False :=
      fun y hy hyc => hv (hyc ▸ S.mem_wr LC LO (mem_of hx) hy)
    cases x <;> simp only [wcol] <;> (try rfl)
    · rename_i p
      split
      · rename_i hpe
        refine (hw (.col c) ?_ rfl).elim
        simp only [effWr, hpe.2.1, List.mem_cons, List.mem_append, List.mem_map, List.mem_filter,
          decide_eq_true_eq]
        exact .inr (.inl ⟨c, ⟨hC c, rfl⟩, rfl⟩)
      · rfl
    all_goals
      split
      · rename_i hpe
        exact (hw (.col c) (by simp [effWr, hpe.1]) rfl).elim
      · rfl
  | own o =>
    show Val.own _ = Val.own _
    congr 1
    funext k
    show Option.getD (lastW _ 0 (S.effs a)) _ = _
    rw [lastW_none (S.effs a) 0 fun i x hx => ?_]
    · rfl
    have hw : ∀ y, y ∈ S.effWr LC LO (S.params a) x → y = .own o → False :=
      fun y hy hyc => hv (hyc ▸ S.mem_wr LC LO (mem_of hx) hy)
    cases x <;> simp only [wown] <;> (try rfl)
    all_goals
      split
      · rename_i hpe
        exact (hw (.own o) (by simp [effWr, hpe.1]) rfl).elim
      · rfl
  | now =>
    show Val.now _ = Val.now _
    congr 1
    show Option.getD (lastW _ 0 (S.effs a)) _ = _
    rw [lastW_none (S.effs a) 0 fun i x hx => ?_]
    · rfl
    cases x <;> simp only [wnow]
    exact (hv (S.mem_wr LC LO (mem_of hx) (by simp [effWr]))).elim

/-- On a state read as a valuation, the effect is `post`. -/
theorem eff_enc (hC : ∀ c, c ∈ LC) (a : A) (b : Binding) (s : St E C O) :
    S.eff LC LO a b (enc s) = enc (S.post a b s) := by
  funext v
  simp only [eff, dec_enc]
  split
  · rfl
  · rename_i hv; exact (S.post_frame hC a b s hv).symm

end Sys

/-! ## `Local` from the syntax -/

namespace Sys

variable {S : Sys K E C O A} [DecidableEq E] [DecidableEq C] [DecidableEq O]
  {LE : List E} {LC : List C} {LO : List O} {LA : List A}
  {hE : ∀ e, e ∈ LE} {hC : ∀ c, c ∈ LC} {hO : ∀ o, o ∈ LO} {hA : ∀ a, a ∈ LA}

theorem picks_local {a : A} {b : Binding} {σ τ : Var E C O → Val}
    (h : Agree (S.effs a |>.flatMap effRd) σ τ) (hp : PicksOk (S.effs a) (dec σ) b) :
    PicksOk (S.effs a) (dec τ) b := by
  intro i x hx
  have hpk := hp i x hx
  have hmem : x ∈ S.effs a := List.mem_iff_getElem?.2 ⟨_, hx⟩
  cases x with
  | trans c p arms =>
    have hc : (dec σ).col c = (dec τ).col c :=
      (VAgree.of_dec h).col (List.mem_flatMap.2 ⟨_, hmem, List.mem_singleton_self _⟩)
    simp only [pickOk] at hpk ⊢
    rw [← hc]; exact hpk
  | _ => exact hpk

/-- **`Local` from the syntax.** The read and write sets computed from the syntax bound what
the guards and the effects of `model` look at. `Built` gives the one case the syntax alone
does not: a `New` parameter's role conjunct reads its existence, which its create writes. -/
theorem local_ (hb : S.Built) : Local (S.model LE LC LO LA hE hC hO hA) where
  conj_rd := by
    intro a g hg v hv
    show v ∈ S.rd a ++ S.wr LC LO a
    rcases List.mem_append.1 hg with hr | ho
    · obtain ⟨j, q, hq, rfl⟩ := Act.mem_roleConjs_inv rowsV Var.ex 0 (S.params a) g hr
      simp only [roleConj] at hv
      split at hv
      · rename_i hrr
        rw [List.mem_singleton] at hv
        subst hv
        cases hrole : q.role with
        | new =>
          refine List.mem_append_right _ (S.mem_wr LC LO (hb.new_create a j q hq hrole) ?_)
          simp [effWr, pent, hq]
        | earlier st => rw [hrole] at hrr; cases hrr
        | _ =>
          refine List.mem_append_left _ (List.mem_append_left _ (List.mem_append_left _ ?_))
          exact List.mem_filterMap.2 ⟨q, List.mem_iff_getElem?.2 ⟨_, hq⟩, by
            simp [hrole, Role.needsPresent]⟩
      · cases hv
    · simp only [og, List.mem_cons, List.mem_map] at ho
      rcases ho with rfl | rfl | ⟨φ, hφ, rfl⟩
      · cases hv
      · exact List.mem_append_left _ (List.mem_append_right _ hv)
      · exact List.mem_append_left _ (List.mem_append_left _ (List.mem_append_right _
          (List.mem_flatMap.2 ⟨φ, hφ, hv⟩)))
  conj_local := by
    intro a g hg b σ τ hag hh
    rcases List.mem_append.1 hg with hr | ho
    · obtain ⟨j, q, _, rfl⟩ := Act.mem_roleConjs_inv rowsV Var.ex 0 (S.params a) g hr
      exact Act.roleConj_local rowsV Var.ex _ q b hag hh
    · simp only [og, List.mem_cons, List.mem_map] at ho
      rcases ho with rfl | rfl | ⟨φ, _, rfl⟩
      · exact hh
      · exact picks_local hag hh
      · have hs := VAgree.of_dec hag
        show S.eval (dec τ) (S.post a b (dec τ)) b.slot [] φ = true
        rw [← S.eval_congr b.slot φ [] hs (S.post_agree a b hs)]
        exact hh
  eff_frame := by
    intro a b σ v hv
    have hv' : v ∉ S.wr LC LO a := hv
    show (if v ∈ S.wr LC LO a then _ else _) = _
    simp only [hv', ↓reduceIte]
  eff_local := by
    intro a b σ τ hag v hv
    have hv' : v ∈ S.wr LC LO a := hv
    show (if v ∈ S.wr LC LO a then _ else _) = (if v ∈ S.wr LC LO a then _ else _)
    simp only [hv', ↓reduceIte]
    exact S.post_pt a b (enc_dec_congr (hag v (List.mem_append_right _ hv)))

/-! ## The action model's steps are the slot model's -/

theorem roles_iff {a : A} {b : Binding} {s : St E C O} :
    (∀ g ∈ roleConjs rowsV Var.ex 0 (S.params a), g.holds b (enc s)) ↔
      ∀ p q, (S.params a)[p]? = some q → roleOk s q.entity (b.slot p) q.role := by
  constructor
  · intro h p q hq
    have := h _ (Act.mem_roleConjs rowsV Var.ex 0 (S.params a) p q hq)
    simp only [roleConj, Nat.zero_add] at this
    exact (rowsV_cand _ _ _).1 this
  · intro h g hg
    obtain ⟨j, q, hq, rfl⟩ := Act.mem_roleConjs_inv rowsV Var.ex 0 (S.params a) g hg
    show rowsV.cand q.role (enc s (.ex q.entity)) (b.slot (0 + j))
    rw [Nat.zero_add]
    exact (rowsV_cand _ _ _).2 (h j q hq)

/-- The guard of `model` on `enc s` is the step condition of `S.lts`. -/
theorem guard_enc {a : A} {b : Binding} {s : St E C O} :
    (S.model LE LC LO LA hE hC hO hA).guard a b (enc s) ↔
      S.BindOk (S.params a) b.slot s ∧ PicksOk (S.effs a) s b ∧
        ∀ φ ∈ S.guards a, S.eval s (S.post a b s) b.slot [] φ = true := by
  show (∀ g ∈ roleConjs rowsV Var.ex 0 (S.params a) ++ S.og a, g.holds b (enc s)) ↔ _
  simp only [List.mem_append, or_imp, forall_and, og, List.mem_cons, List.mem_map,
    forall_eq, roles_iff]
  constructor
  · rintro ⟨hr, hbd, hpk, hg⟩
    exact ⟨fun p q hq => ⟨hbd p q hq, hr p q hq⟩, hpk, fun φ hφ => hg _ ⟨φ, hφ, rfl⟩⟩
  · rintro ⟨hB, hpk, hg⟩
    refine ⟨fun p q hq => (hB p q hq).2, fun p q hq => (hB p q hq).1, hpk, ?_⟩
    rintro _ ⟨φ, hφ, rfl⟩
    exact hg φ hφ

/-- **The steps of `model` from `enc s` are those of `S.lts` from `s`.** -/
theorem step_enc {s : St E C O} {l : A × Binding} {σ' : Var E C O → Val} :
    (S.model LE LC LO LA hE hC hO hA).lts.step (enc s) l σ' ↔
      ∃ t, S.lts.step s l t ∧ σ' = enc t := by
  show (S.model LE LC LO LA hE hC hO hA).guard l.1 l.2 (enc s) ∧
      σ' = S.eff LC LO l.1 l.2 (enc s) ↔ _
  rw [guard_enc, S.eff_enc hC]
  constructor
  · rintro ⟨⟨hB, hpk, hg⟩, rfl⟩
    exact ⟨_, ⟨hB, hpk, hg, rfl⟩, rfl⟩
  · rintro ⟨t, ⟨hB, hpk, hg, rfl⟩, rfl⟩
    exact ⟨⟨hB, hpk, hg⟩, rfl⟩

/-- **The reachable valuations of `model` are the reachable states of `S.lts`, read as
valuations.** -/
theorem reach_enc (σ : Var E C O → Val) :
    (S.model LE LC LO LA hE hC hO hA).lts.Reach σ ↔ ∃ s, S.lts.Reach s ∧ σ = enc s := by
  constructor
  · intro h
    induction h with
    | init hi =>
      obtain ⟨s, hs, rfl⟩ := hi
      exact ⟨s, Reach.init hs, rfl⟩
    | step _ hst ih =>
      obtain ⟨s, hs, rfl⟩ := ih
      obtain ⟨t, ht, rfl⟩ := step_enc.1 hst
      exact ⟨t, Reach.step hs ht, rfl⟩
  · rintro ⟨s, hs, rfl⟩
    induction hs with
    | init hi => exact Reach.init ⟨_, hi, rfl⟩
    | step _ hst ih => exact Reach.step ih (step_enc.2 ⟨_, hst, rfl⟩)

/-! ## Lemma 5 and Lemma 6's hypotheses on the same model -/

/-- **Lemma 5 (a)(b) give `Split`** on the cone system of `model`, for the computed split of
any anchor: `split_computed` with `Local` discharged. -/
theorem split_computed (hb : S.Built) {mono : Var E C O → Bool} {keep : A → Bool}
    {anchor : Option (Var E C O)} {seeds : List (List (Var E C O))} :
    let M := S.model LE LC LO LA hE hC hO hA
    RdraIsh.Split (Act.cone M (Act.computed M mono keep anchor).1 (Act.computed M mono keep anchor).2)
      (Act.obsMap (Act.computed M mono keep anchor).1
        (Act.lemma5ab_of_closed (seeds := seeds) Act.computed_closed).obs_s) :=
  Act.split_computed (local_ hb)

/-- `Lemma6Hyp.shape` holds of `model` for an `Ext` with `rowsV`, `Var.ex` and `og`. -/
theorem shape {Kb : Type} (D : Act.Ext (S.model LE LC LO LA hE hC hO hA) Kb)
    (hr : D.rows = rowsV) (hx : D.ex = Var.ex) (hog : D.og = S.og) (a : A) :
    (S.model LE LC LO LA hE hC hO hA).guards a =
      roleConjs D.rows D.ex 0 ((S.model LE LC LO LA hE hC hO hA).params a) ++ D.og a := by
  rw [hr, hx, hog]; rfl

/-- `Lemma6Hyp.new_writes` holds of `model` under `Built`. -/
theorem new_writes (hb : S.Built) (a : A) (j : Nat) (q : Param E) (hq : (S.params a)[j]? = some q)
    (hr : q.role = .new) : Var.ex q.entity ∈ (S.model LE LC LO LA hE hC hO hA).wr a := by
  refine S.mem_wr LC LO (hb.new_create a j q hq hr) ?_
  simp [effWr, pent, hq]

end Sys

end RdraIsh.SymAct
