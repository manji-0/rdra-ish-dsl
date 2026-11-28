import RdraIsh.SymActModel

/-!
# Lemma 6's hypotheses from the syntax (cones.md, Definition 4 and Lemma 6)

`SymActModel.lean` reads an `Act.Model` off a `SymAct.Sys` and proves `Local` of it. This
file builds, from the same syntax and a split, the data `Act.Ext` of `Extended::build`:

- **What a predicate and an effect name** (`predPs`, `effPs`, after `named_params` and
  `fold_of`'s `touched`), and whether a predicate reads the successor (`predNext`, after
  `Pred::reads_next`). A predicate's value depends on the binding only at the parameters it
  names (`eval_bind`), and not on the successor when it reads none (`eval_succ`); the
  successor depends on the binding only at the parameters the effects name and on the choices
  (`post_bind`).
- **`fold_of`** (`foldKey`): an `S_a` action with a `T` parameter folds when every guard that
  names a `T` parameter reads no successor and either reads `T` alone (a folded guard) or is
  a link `v[k] = Index(p)` (`linkOf`); the key is the `T` parameter an effect names or a link
  names, else the one `S` parameter the folded guards name, else the first `New` or
  `Existing` parameter; and every effect, link and folded guard names no other `T` parameter
  and no other `S` parameter than the key (`foldOk`). The code's tests (at most one touched
  `T` parameter, one link parameter, one `S` key, not both of the last two) are the same: a
  touched or linked parameter is a `T` one and an `S` key is not, so "every one is the key"
  rules out the mixed cases. A `New` key does not fold. An action with no `T` parameter does
  not fold: node folds (`node_fold_of`) are not modelled.
- **The folds the split keeps** (`keyOf`): `split_with` folds only `S_a` actions, keeps a fold
  only when `fold_initial` gives its bit a start (`Cut.sel`, which this file leaves open, as it
  does not model initial states) and drops one whose key `restrict_to(S ∪ inputs)` drops
  (`Fold::key_kept`, `keepsP`).
- **The fold's conjuncts** (`foldedC`): the slot bounds of the `T` parameters and of the key,
  and the folded guards.
- **`restrict_to`'s kept conjuncts** (`keptOg`): a conjunct reading only `S_v ∪ inputs` and
  naming no parameter the restriction drops (`keepsP`, `keeps_param` with havoc off: a `New`
  parameter is kept when its existence is in `S_v ∪ inputs`, any other when its entity is,
  or is a stub, the parent of an owner of `S_v ∪ inputs`, `stubE`).
- **The raw reads** (`rawRd`, `raw_reads`): an unfolded action's reads, and a folded one's
  outside its fold.

`lemma6Hyp` proves `Lemma6Hyp` of `ext` from `Built`, Lemma 5 (a)(b), Definition 3's side
rule for columns, the code's definition of the inputs (they hold the raw reads of `T_v` and
the existences of those), and step (a), `exact`. `lemma6_of_syntax` and `a8_of_syntax` give
Lemma 6 and Theorem A8 with the code's stages from those alone.
-/

namespace RdraIsh.SymAct

open LTS
open Act (Role Param Binding Rows Conj Model Local Agree roleConj roleConjs)

variable {K E C O A : Type}

/-! ## What a term, a predicate and an effect name -/

/-- The parameter a reference names. -/
def refPs : Ref → List Nat
  | .param p => [p]
  | .next p => [p]
  | .bound _ => []

/-- `named_params` of a term: the parameter of a slot or an index at `Param` or `Next`. -/
def termPs : Term E C O → List Nat
  | .col _ r => refPs r
  | .own _ r => refPs r
  | .index _ r => refPs r
  | _ => []

/-- `named_params`: every parameter a predicate's terms name. -/
def predPs : Pred E C O → List Nat
  | .cmp x _ y => termPs x ++ termPs y
  | .inn x ys => termPs x ++ ys.flatMap termPs
  | .not p => predPs p
  | .and p q => predPs p ++ predPs q
  | .or p q => predPs p ++ predPs q
  | .all _ p => predPs p
  | .any _ p => predPs p
  | .count _ p _ _ => predPs p
  | .tt => []
  | .ff => []

/-- A reference to the successor. -/
def refNext : Ref → Bool
  | .next _ => true
  | _ => false

/-- A term at the successor. -/
def termNext : Term E C O → Bool
  | .col _ r => refNext r
  | .own _ r => refNext r
  | .index _ r => refNext r
  | _ => false

/-- `Pred::reads_next`: some term is at the successor. -/
def predNext : Pred E C O → Bool
  | .cmp x _ y => termNext x || termNext y
  | .inn x ys => termNext x || ys.any termNext
  | .not p => predNext p
  | .and p q => predNext p || predNext q
  | .or p q => predNext p || predNext q
  | .all _ p => predNext p
  | .any _ p => predNext p
  | .count _ p _ _ => predNext p
  | .tt => false
  | .ff => false

/-- The parameters an effect names (`fold_of`'s `touched`): its row, and the row an owner or
a column is set to. -/
def effPs : Eff C O → List Nat
  | .create p => [p]
  | .delete p => [p]
  | .setCol _ p _ => [p]
  | .copyCol _ p q => [p, q]
  | .setOwn _ p q => [p, q]
  | .setOwnLit _ p _ => [p]
  | .chooseCol _ p _ => [p]
  | .chooseOwn _ p _ => [p]
  | .trans _ p _ => [p]
  | .tick => []

/-! ## A predicate's value and the successor depend only on what they name -/

theorem tval_bind {s t : St E C O} {b b' : Nat → Nat} (env : List Nat) (x : Term E C O)
    (h : ∀ p ∈ termPs x, b p = b' p) : tval s t b env x = tval s t b' env x := by
  cases x with
  | col c r => cases r <;> simp_all [tval, slotOf, termPs, refPs]
  | own o r => cases r <;> simp_all [tval, slotOf, termPs, refPs]
  | index e r => cases r <;> simp_all [tval, slotOf, termPs, refPs]
  | lit => rfl
  | null => rfl
  | now => rfl

theorem tval_succ {s t t' : St E C O} {b : Nat → Nat} (env : List Nat) (x : Term E C O)
    (h : termNext x = false) : tval s t b env x = tval s t' b env x := by
  cases x with
  | col c r => cases r <;> simp_all [tval, stOf, termNext, refNext]
  | own o r => cases r <;> simp_all [tval, stOf, termNext, refNext]
  | index e r => rfl
  | lit => rfl
  | null => rfl
  | now => rfl

namespace Sys

variable (S : Sys K E C O A)

/-- **A predicate depends on the binding only at the parameters it names.** -/
theorem eval_bind {s t : St E C O} {b b' : Nat → Nat} :
    ∀ (φ : Pred E C O) (env : List Nat), (∀ p ∈ predPs φ, b p = b' p) →
      S.eval s t b env φ = S.eval s t b' env φ
  | .tt, _, _ => rfl
  | .ff, _, _ => rfl
  | .cmp x op y, env, h => by
    simp only [eval, tval_bind env x (fun p hp => h p (List.mem_append_left _ hp)),
      tval_bind env y (fun p hp => h p (List.mem_append_right _ hp))]
  | .inn x ys, env, h => by
    simp only [eval, tval_bind env x (fun p hp => h p (List.mem_append_left _ hp))]
    exact any_congr_mem fun y hy => by
      rw [tval_bind env y (fun p hp => h p (List.mem_append_right _
        (List.mem_flatMap.2 ⟨y, hy, hp⟩)))]
  | .not p, env, h => by simp only [eval, eval_bind p env h]
  | .and p q, env, h => by
    simp only [eval, eval_bind p env (fun x hx => h x (List.mem_append_left _ hx)),
      eval_bind q env (fun x hx => h x (List.mem_append_right _ hx))]
  | .or p q, env, h => by
    simp only [eval, eval_bind p env (fun x hx => h x (List.mem_append_left _ hx)),
      eval_bind q env (fun x hx => h x (List.mem_append_right _ hx))]
  | .all e p, env, h => by
    have hp := fun k => eval_bind (s := s) (t := t) p (k :: env) h
    simp only [eval, hp]
  | .any e p, env, h => by
    have hp := fun k => eval_bind (s := s) (t := t) p (k :: env) h
    simp only [eval, hp]
  | .count e p op m, env, h => by
    have hp := fun k => eval_bind (s := s) (t := t) p (k :: env) h
    simp only [eval, hp]

/-- **A predicate that reads no successor does not depend on it.** -/
theorem eval_succ {s t t' : St E C O} {b : Nat → Nat} :
    ∀ (φ : Pred E C O) (env : List Nat), predNext φ = false →
      S.eval s t b env φ = S.eval s t' b env φ
  | .tt, _, _ => rfl
  | .ff, _, _ => rfl
  | .cmp x op y, env, h => by
    simp only [predNext, Bool.or_eq_false_iff] at h
    simp only [eval, tval_succ (s := s) (t := t) (t' := t') (b := b) env x h.1, tval_succ (s := s) (t := t) (t' := t') (b := b) env y h.2]
  | .inn x ys, env, h => by
    simp only [predNext, Bool.or_eq_false_iff, List.any_eq_false] at h
    simp only [eval, tval_succ (s := s) (t := t) (t' := t') (b := b) env x h.1]
    exact any_congr_mem fun y hy => by
      rw [tval_succ (s := s) (t := t) (t' := t') (b := b) env y (by simpa using h.2 y hy)]
  | .not p, env, h => by simp only [eval, eval_succ (s := s) (t := t) (t' := t') (b := b) p env h]
  | .and p q, env, h => by
    simp only [predNext, Bool.or_eq_false_iff] at h
    simp only [eval, eval_succ (s := s) (t := t) (t' := t') (b := b) p env h.1, eval_succ (s := s) (t := t) (t' := t') (b := b) q env h.2]
  | .or p q, env, h => by
    simp only [predNext, Bool.or_eq_false_iff] at h
    simp only [eval, eval_succ (s := s) (t := t) (t' := t') (b := b) p env h.1, eval_succ (s := s) (t := t) (t' := t') (b := b) q env h.2]
  | .all e p, env, h => by
    have hp := fun k => eval_succ (s := s) (t := t) (t' := t') (b := b) p (k :: env) h
    simp only [eval, hp]
  | .any e p, env, h => by
    have hp := fun k => eval_succ (s := s) (t := t) (t' := t') (b := b) p (k :: env) h
    simp only [eval, hp]
  | .count e p op m, env, h => by
    have hp := fun k => eval_succ (s := s) (t := t) (t' := t') (b := b) p (k :: env) h
    simp only [eval, hp]

variable [DecidableEq E] [DecidableEq C] [DecidableEq O]

omit [DecidableEq C] [DecidableEq O] in
theorem wex_bind {ps : List (Param E)} {sl sl' : Nat → Nat} {e : E} {k : Nat} {x : Eff C O}
    (h : ∀ p ∈ effPs x, sl p = sl' p) : wex ps sl e k x = wex ps sl' e k x := by
  cases x <;> simp_all [wex, effPs]

omit [DecidableEq O] in
theorem wcol_bind {ps : List (Param E)} {b b' : Binding} {s : St E C O} {i : Nat} {c : C}
    {k : Nat} {x : Eff C O} (h : ∀ p ∈ effPs x, b.slot p = b'.slot p)
    (hc : b.choice = b'.choice) : S.wcol ps b s i c k x = S.wcol ps b' s i c k x := by
  cases x <;> simp_all [wcol, effPs]

omit [DecidableEq C] in
theorem wown_bind {ps : List (Param E)} {b b' : Binding} {i : Nat} {o : O} {k : Nat}
    {x : Eff C O} (h : ∀ p ∈ effPs x, b.slot p = b'.slot p) (hc : b.choice = b'.choice) :
    S.wown ps b i o k x = S.wown ps b' i o k x := by
  cases x <;> simp_all [wown, effPs]

/-- **The successor depends on the binding only at the parameters the effects name, and on
the choices.** -/
theorem post_bind {a : A} {b b' : Binding} {s : St E C O}
    (h : ∀ x ∈ S.effs a, ∀ p ∈ effPs x, b.slot p = b'.slot p) (hc : b.choice = b'.choice) :
    S.post a b s = S.post a b' s := by
  have hx : ∀ i x, (S.effs a)[i]? = some x → x ∈ S.effs a :=
    fun i x hx => List.mem_iff_getElem?.2 ⟨i, hx⟩
  apply St.ext'
  · funext e k
    simp only [post]
    congr 1
    exact lastW_congr _ 0 fun i x hxi => wex_bind (h x (hx _ _ hxi))
  · funext c k
    simp only [post]
    congr 1
    exact lastW_congr _ 0 fun i x hxi => S.wcol_bind (h x (hx _ _ hxi)) hc
  · funext o k
    simp only [post]
    congr 1
    exact lastW_congr _ 0 fun i x hxi => S.wown_bind (h x (hx _ _ hxi)) hc
  · rfl

omit [DecidableEq E] [DecidableEq C] [DecidableEq O] in
/-- The picks depend on the binding only at the parameters the effects name, and on the
choices. -/
theorem picks_bind {xs : List (Eff C O)} {s : St E C O} {b b' : Binding}
    (h : ∀ x ∈ xs, ∀ p ∈ effPs x, b.slot p = b'.slot p) (hc : b.choice = b'.choice)
    (hp : PicksOk xs s b) : PicksOk xs s b' := by
  intro i x hx
  have hpk := hp i x hx
  have hm : x ∈ xs := List.mem_iff_getElem?.2 ⟨_, hx⟩
  cases x with
  | trans c p arms =>
    have := h _ hm p (by simp [effPs])
    simp only [pickOk] at hpk ⊢
    rw [← hc, ← this]; exact hpk
  | chooseCol c p vs => simp only [pickOk] at hpk ⊢; rw [← hc]; exact hpk
  | chooseOwn o p vs => simp only [pickOk] at hpk ⊢; rw [← hc]; exact hpk
  | _ => trivial

end Sys

/-! ## `fold_of` on the syntax -/

/-- Position `j` of a parameter list is a parameter on an entity whose existence is in `T_v`
(`fold_of`'s `t_params`). -/
def isT (tv : Var E C O → Bool) (ps : List (Param E)) (j : Nat) : Bool :=
  match ps[j]? with
  | some q => tv (.ex q.entity)
  | none => false

/-- A slot term at a parameter, `v[k]`: its variable and `k`. -/
def slotVar : Term E C O → Option (Var E C O × Nat)
  | .col c (.param k) => some (.col c, k)
  | .own o (.param k) => some (.own o, k)
  | _ => none

/-- An index term at a parameter, `Index(p)`: `p`. -/
def idxP : Term E C O → Option Nat
  | .index _ (.param p) => some p
  | _ => none

/-- `v[k] = Index(p)` with `v` in `S_v`, `k` not a `T` parameter and `p` one: `p`. -/
def link1 (tv : Var E C O → Bool) (ps : List (Param E)) (x y : Term E C O) : Option Nat :=
  match slotVar x, idxP y with
  | some (v, k), some p => if !tv v && !isT tv ps k && isT tv ps p then some p else none
  | _, _ => none

/-- `fold_of`'s link guard, either way round: the `T` parameter it links. -/
def linkOf (tv : Var E C O → Bool) (ps : List (Param E)) : Pred E C O → Option Nat
  | .cmp x .eq y => (link1 tv ps x y).or (link1 tv ps y x)
  | _ => none

/-- A guard names a `T` parameter. -/
def tNamed (tv : Var E C O → Bool) (ps : List (Param E)) (φ : Pred E C O) : Bool :=
  (predPs φ).any (isT tv ps)

/-- A guard reads `T_v` alone. -/
def readsT (tv : Var E C O → Bool) (φ : Pred E C O) : Bool := (predRd φ).all tv

/-- A folded guard: it names a `T` parameter and reads `T_v` alone. -/
def foldedG (tv : Var E C O → Bool) (ps : List (Param E)) (φ : Pred E C O) : Bool :=
  tNamed tv ps φ && readsT tv φ

theorem slotVar_ps {x : Term E C O} {v : Var E C O} {k : Nat} (h : slotVar x = some (v, k)) :
    termPs x = [k] := by
  rcases x with ⟨c, r⟩ | ⟨o, r⟩ | ⟨e, r⟩ | _ | _ | _ <;> (try cases r) <;>
    simp_all [slotVar, termPs, refPs]

theorem idxP_ps {x : Term E C O} {p : Nat} (h : idxP x = some p) : termPs x = [p] := by
  rcases x with ⟨c, r⟩ | ⟨o, r⟩ | ⟨e, r⟩ | _ | _ | _ <;> (try cases r) <;>
    simp_all [idxP, termPs, refPs]

theorem link1_spec {tv : Var E C O → Bool} {ps : List (Param E)} {x y : Term E C O} {p : Nat}
    (h : link1 tv ps x y = some p) :
    isT tv ps p = true ∧ ∀ j ∈ termPs x ++ termPs y, isT tv ps j = true → j = p := by
  unfold link1 at h
  split at h
  · rename_i v k p' hx hy
    split at h
    · rename_i hc
      cases h
      simp only [Bool.and_eq_true, Bool.not_eq_true'] at hc
      refine ⟨hc.2, fun j hj hT => ?_⟩
      rw [slotVar_ps hx, idxP_ps hy] at hj
      simp only [List.cons_append, List.nil_append, List.mem_cons, List.not_mem_nil,
        or_false] at hj
      rcases hj with rfl | rfl
      · rw [hc.1.2] at hT; cases hT
      · rfl
    · cases h
  · cases h

/-- What a link guard names: a `T` parameter, `p`, and no other. -/
theorem linkOf_spec {tv : Var E C O → Bool} {ps : List (Param E)} {φ : Pred E C O} {p : Nat}
    (h : linkOf tv ps φ = some p) :
    isT tv ps p = true ∧ ∀ j ∈ predPs φ, isT tv ps j = true → j = p := by
  cases φ with
  | cmp x op y =>
    cases op with
    | eq =>
      simp only [linkOf, Option.or_eq_some_iff] at h
      rcases h with h | ⟨_, h⟩
      · exact link1_spec h
      · obtain ⟨h1, h2⟩ := link1_spec h
        refine ⟨h1, fun j hj => h2 j ?_⟩
        simp only [predPs, List.mem_append] at hj ⊢
        exact hj.symm
    | _ => simp [linkOf] at h
  | _ => simp [linkOf] at h

namespace Sys

variable (S : Sys K E C O A)

/-- `fold_of`'s key candidate: the first touched or linked `T` parameter, else the first `S`
parameter a folded guard names, else the first `New` or `Existing` parameter. -/
def keyCand (tv : Var E C O → Bool) (a : A) : Option Nat :=
  let ps := S.params a
  let tg := (S.guards a).filter (tNamed tv ps)
  let cands := ((S.effs a).flatMap effPs).filter (isT tv ps) ++
    (tg.filter (fun φ => !readsT tv φ)).filterMap (linkOf tv ps)
  let sk := (tg.filter (readsT tv)).flatMap fun φ => (predPs φ).filter fun j => !isT tv ps j
  match cands.head?, sk.head? with
  | some p, _ => some p
  | none, some k => some k
  | none, none => (List.range ps.length).find? fun j => match ps[j]? with
    | some q => q.role == .new || q.role == .existing
    | none => false

/-- `fold_of`'s tests for a key: the action has a `T` parameter; every guard that names one
reads no successor and either reads `T_v` alone and names no `S` parameter but the key, or
is a link of the key; every effect names no `T` parameter but the key; the key is not
`New`. -/
def foldOk (tv : Var E C O → Bool) (a : A) (key : Nat) : Bool :=
  let ps := S.params a
  ps.any (fun q => tv (.ex q.entity)) &&
  (S.guards a).all (fun φ => !tNamed tv ps φ || (!predNext φ &&
    (if readsT tv φ then (predPs φ).all (fun j => isT tv ps j || j == key)
     else linkOf tv ps φ == some key))) &&
  (S.effs a).all (fun x => (effPs x).all fun j => !isT tv ps j || j == key) &&
  (match ps[key]? with
   | some q => q.role != .new
   | none => true)

/-- **`fold_of`**: the key of the action's fold, if it folds. -/
def foldKey (tv : Var E C O → Bool) (a : A) : Option Nat :=
  (S.keyCand tv a).bind fun key => if S.foldOk tv a key then some key else none

/-- What `foldKey` gives, by the guard, effect and key. -/
structure FoldSpec (tv : Var E C O → Bool) (a : A) (key : Nat) : Prop where
  no_next : ∀ φ ∈ S.guards a, tNamed tv (S.params a) φ = true → predNext φ = false
  folded : ∀ φ ∈ S.guards a, foldedG tv (S.params a) φ = true →
    ∀ j ∈ predPs φ, isT tv (S.params a) j = false → j = key
  link : ∀ φ ∈ S.guards a, tNamed tv (S.params a) φ = true → readsT tv φ = false →
    linkOf tv (S.params a) φ = some key
  effs : ∀ x ∈ S.effs a, ∀ j ∈ effPs x, isT tv (S.params a) j = true → j = key
  not_new : ∀ q, (S.params a)[key]? = some q → q.role ≠ .new

theorem foldKey_spec {tv : Var E C O → Bool} {a : A} {key : Nat}
    (h : S.foldKey tv a = some key) : S.FoldSpec tv a key := by
  simp only [foldKey, Option.bind_eq_some_iff] at h
  obtain ⟨k, _, h⟩ := h
  split at h
  · rename_i hok
    cases h
    simp only [foldOk, Bool.and_eq_true, List.all_eq_true, Bool.or_eq_true,
      Bool.not_eq_true'] at hok
    obtain ⟨⟨⟨_, hg⟩, he⟩, hk⟩ := hok
    refine ⟨fun φ hφ ht => ?_, fun φ hφ hf j hj hT => ?_, fun φ hφ ht hr => ?_,
      fun x hx j hj hT => ?_, fun q hq => ?_⟩
    · rcases hg φ hφ with h | h
      · rw [ht] at h; cases h
      · exact h.1
    · simp only [foldedG, Bool.and_eq_true] at hf
      rcases hg φ hφ with h | h
      · rw [hf.1] at h; cases h
      · simp only [hf.2, ↓reduceIte, List.all_eq_true, Bool.or_eq_true] at h
        rcases h.2 j hj with h' | h'
        · rw [hT] at h'; cases h'
        · simpa using h'
    · rcases hg φ hφ with h | h
      · rw [ht] at h; cases h
      · simp only [hr, Bool.false_eq_true, ↓reduceIte] at h
        simpa using h.2
    · rcases he x hx j hj with h | h
      · rw [hT] at h; cases h
      · simpa using h
    · rw [hq] at hk
      simpa using hk
  · cases h

theorem isT_iff {tv : Var E C O → Bool} {ps : List (Param E)} {j : Nat} :
    isT tv ps j = true ↔ ∃ q, ps[j]? = some q ∧ tv (.ex q.entity) = true := by
  unfold isT
  cases ps[j]? <;> simp

end Sys

/-! ## The split, the stubs, the kept conjuncts, the folds -/

/-- What `Extended::build` starts from besides the cone: the split `T_v`, `T_a`, the inputs,
and which of the folds `fold_of` finds `split_with` keeps on account of their start (`sel`,
`fold_initial`; this file does not model initial states). -/
structure Cut (E C O A : Type) where
  tv : Var E C O → Bool
  ta : A → Bool
  inputs : Var E C O → Bool
  inputs_t : ∀ v, inputs v = true → tv v = true
  sel : A → Bool

/-- A variable of `S_v ∪ inputs`, which `restrict_to` keeps. -/
def Cut.inV (X : Cut E C O A) (v : Var E C O) : Bool := !X.tv v || X.inputs v

theorem Cut.inV_iff {X : Cut E C O A} {v : Var E C O} :
    X.inV v = true ↔ X.tv v = false ∨ X.inputs v = true := by
  simp [Cut.inV]

namespace Sys

variable (S : Sys K E C O A) [DecidableEq E] [DecidableEq C] [DecidableEq O]
  (X : Cut E C O A) (LC : List C) (LO : List O)

/-- A stub (`restrict_to`): the parent of an owner variable that `restrict_to` keeps. -/
def stubE (e : E) : Bool := LO.any fun o => decide (S.par o = e) && X.inV (.own o)

/-- `keeps_param` with havoc off: a `New` parameter when its existence is kept, any other
when its entity's existence is kept or the entity is a stub. -/
def keepsP (q : Param E) : Bool :=
  X.inV (.ex q.entity) || (decide (q.role ≠ .new) && S.stubE X LO q.entity)

/-- Position `j` of `a` names no parameter the restriction drops. -/
def keepsPos (a : A) (j : Nat) : Bool :=
  match (S.params a)[j]? with
  | some q => S.keepsP X LO q
  | none => true

/-- **The folds `split_with` keeps**: an `S_a` action whose fold `sel` keeps, whose key
`restrict_to(S ∪ inputs)` keeps (`Fold::key_kept`): its key. -/
def keyOf (a : A) : Option Nat :=
  if X.ta a = false ∧ (S.wr LC LO a).isEmpty = false ∧ X.sel a = true then
    (S.foldKey X.tv a).bind fun key => match (S.params a)[key]? with
      | some q => if S.keepsP X LO q then some key else none
      | none => none
  else none

/-- The bound conjuncts of the positions `P` holds of, numbered from `n`. -/
def boundsWhere (P : Nat → Param E → Bool) : Nat → List (Param E) → List (Conj (Var E C O) Val)
  | _, [] => []
  | n, q :: ps => if P n q then S.boundConj n q :: boundsWhere P (n + 1) ps
    else boundsWhere P (n + 1) ps

/-- **A fold's conjuncts**: the slot bounds of its `T` parameters and of its key, and the
folded guards. -/
def foldedC (a : A) (key : Nat) : List (Conj (Var E C O) Val) :=
  S.boundsWhere (fun j q => X.tv (.ex q.entity) || j == key) 0 (S.params a) ++
    ((S.guards a).filter (foldedG X.tv (S.params a))).map (S.predConj a)

/-- **`restrict_to`'s kept conjuncts**: those of `og` that read only `S_v ∪ inputs` and name
no parameter the restriction drops. The picks stand for the choice effects, which name the
parameters the effects name. -/
def keptOg (a : A) : List (Conj (Var E C O) Val) :=
  S.boundsWhere (fun _ q => S.keepsP X LO q) 0 (S.params a) ++
    (if ((S.effs a).flatMap effRd).all X.inV &&
        ((S.effs a).flatMap effPs).all (S.keepsPos X LO a) then [S.picksConj a] else []) ++
    ((S.guards a).filter fun φ =>
      (predRd φ).all X.inV && (predPs φ).all (S.keepsPos X LO a)).map (S.predConj a)

/-- **`raw_reads`**: what an action reads outside its fold. -/
def rawRd (a : A) : List (Var E C O) :=
  match S.keyOf X LC LO a with
  | none => S.rd a
  | some _ =>
    (S.params a).filterMap (fun q =>
      if q.role.needsPresent && !X.tv (.ex q.entity) then some (.ex q.entity) else none) ++
    ((S.guards a).filter fun φ => !foldedG X.tv (S.params a) φ).flatMap predRd ++
    (S.effs a).flatMap effRd

/-- **The data of `Extended::build`, from the syntax.** The bits (`keys`, `bitKey`, `bitOf`)
and the value of the variables `X` does not hold (`dflt`) are left open. -/
def ext (LE : List E) (LC : List C) (LO : List O) (LA : List A)
    (hE : ∀ e, e ∈ LE) (hC : ∀ c, c ∈ LC) (hO : ∀ o, o ∈ LO) (hA : ∀ a, a ∈ LA)
    (X : Cut E C O A) {Kb : Type} (keys : List Kb) (keys_all : ∀ k, k ∈ keys)
    (bitKey : A → Nat → Kb) (bitOf : Kb → Act.TV X.tv (Val := Val) → Bool)
    (dflt : Var E C O → Val) : Act.Ext (S.model LE LC LO LA hE hC hO hA) Kb where
  tv := X.tv
  ta := X.ta
  inputs := X.inputs
  inputs_t := X.inputs_t
  ex := Var.ex
  stub := S.stubE X LO
  rows := rowsV
  og := S.og
  kept a g := g ∈ S.keptOg X LO a
  fold a := (S.keyOf X LC LO a).map fun key => ⟨key, S.foldedC X a key, bitKey a⟩
  keys := keys
  keys_all := keys_all
  bitOf := bitOf
  dflt := dflt
  bindStubs := true

omit [DecidableEq E] [DecidableEq C] [DecidableEq O] in
theorem mem_boundsWhere_inv (P : Nat → Param E → Bool) :
    ∀ (n : Nat) (ps : List (Param E)) (g : Conj (Var E C O) Val), g ∈ S.boundsWhere P n ps →
      ∃ j q, ps[j]? = some q ∧ P (n + j) q = true ∧ g = S.boundConj (n + j) q
  | _, [], g, h => by simp [boundsWhere] at h
  | n, q :: ps, g, h => by
    simp only [boundsWhere] at h
    split at h
    · rename_i hP
      rcases List.mem_cons.1 h with rfl | h
      · exact ⟨0, q, rfl, by simpa using hP, by simp⟩
      · obtain ⟨j, q', hq, hP', rfl⟩ := mem_boundsWhere_inv P (n + 1) ps g h
        exact ⟨j + 1, q', by simpa using hq, by rwa [show n + (j + 1) = n + 1 + j by omega],
          by rw [show n + (j + 1) = n + 1 + j by omega]⟩
    · obtain ⟨j, q', hq, hP', rfl⟩ := mem_boundsWhere_inv P (n + 1) ps g h
      exact ⟨j + 1, q', by simpa using hq, by rwa [show n + (j + 1) = n + 1 + j by omega],
        by rw [show n + (j + 1) = n + 1 + j by omega]⟩

omit [DecidableEq E] [DecidableEq C] [DecidableEq O] in
theorem mem_boundsWhere (P : Nat → Param E → Bool) :
    ∀ (n : Nat) (ps : List (Param E)) (j : Nat) (q : Param E), ps[j]? = some q →
      P (n + j) q = true → S.boundConj (n + j) q ∈ S.boundsWhere P n ps
  | _, [], _, _, h, _ => by simp at h
  | n, p :: ps, 0, q, h, hP => by
    simp only [List.getElem?_cons_zero, Option.some.injEq] at h
    subst h
    simp only [boundsWhere, Nat.add_zero] at hP ⊢
    simp only [hP, ↓reduceIte]
    exact List.mem_cons_self
  | n, p :: ps, j + 1, q, h, hP => by
    simp only [List.getElem?_cons_succ] at h
    have := mem_boundsWhere P (n + 1) ps j q h (by rwa [show n + 1 + j = n + (j + 1) by omega])
    rw [show n + (j + 1) = n + 1 + j by omega]
    simp only [boundsWhere]
    split
    · exact List.mem_cons_of_mem _ this
    · exact this

omit [DecidableEq E] [DecidableEq C] [DecidableEq O] in
theorem rd_param {a : A} {q : Param E} (hq : q ∈ S.params a) (hn : q.role.needsPresent = true) :
    (Var.ex q.entity : Var E C O) ∈ S.rd a :=
  List.mem_append_left _ (List.mem_append_left _ (List.mem_filterMap.2 ⟨q, hq, by simp [hn]⟩))

omit [DecidableEq E] [DecidableEq C] [DecidableEq O] in
theorem rd_guard {a : A} {φ : Pred E C O} {v : Var E C O} (hφ : φ ∈ S.guards a)
    (hv : v ∈ predRd φ) : v ∈ S.rd a :=
  List.mem_append_left _ (List.mem_append_right _ (List.mem_flatMap.2 ⟨φ, hφ, hv⟩))

omit [DecidableEq E] [DecidableEq C] [DecidableEq O] in
theorem rd_eff {a : A} {x : Eff C O} {v : Var E C O} (hx : x ∈ S.effs a) (hv : v ∈ effRd x) :
    v ∈ S.rd a :=
  List.mem_append_right _ (List.mem_flatMap.2 ⟨x, hx, hv⟩)

omit [DecidableEq E] [DecidableEq C] [DecidableEq O] in
theorem rd_cases {a : A} {v : Var E C O} (hv : v ∈ S.rd a) :
    (∃ q ∈ S.params a, q.role.needsPresent = true ∧ v = .ex q.entity) ∨
      (∃ φ ∈ S.guards a, v ∈ predRd φ) ∨ ∃ x ∈ S.effs a, v ∈ effRd x := by
  simp only [rd, List.mem_append, List.mem_flatMap] at hv
  rcases hv with (hv | hv) | hv
  · obtain ⟨q, hq, h⟩ := List.mem_filterMap.1 hv
    split at h
    · cases h; exact .inl ⟨q, hq, by assumption, rfl⟩
    · cases h
  · exact .inr (.inl hv)
  · exact .inr (.inr hv)

omit [DecidableEq C] [DecidableEq O] in
theorem rawRd_none {a : A} (h : S.keyOf X LC LO a = none) : S.rawRd X LC LO a = S.rd a := by
  simp [rawRd, h]

omit [DecidableEq C] [DecidableEq O] in
theorem rawRd_eff {a : A} {x : Eff C O} {v : Var E C O} (hx : x ∈ S.effs a)
    (hv : v ∈ effRd x) : v ∈ S.rawRd X LC LO a := by
  unfold rawRd
  split
  · exact S.rd_eff hx hv
  · exact List.mem_append_right _ (List.mem_flatMap.2 ⟨x, hx, hv⟩)

omit [DecidableEq C] [DecidableEq O] in
theorem rawRd_guard {a : A} {key : Nat} (hk : S.keyOf X LC LO a = some key) {φ : Pred E C O}
    {v : Var E C O} (hφ : φ ∈ S.guards a) (hf : foldedG X.tv (S.params a) φ = false)
    (hv : v ∈ predRd φ) : v ∈ S.rawRd X LC LO a := by
  simp only [rawRd, hk]
  exact List.mem_append_left _ (List.mem_append_right _
    (List.mem_flatMap.2 ⟨φ, List.mem_filter.2 ⟨hφ, by simp [hf]⟩, hv⟩))

omit [DecidableEq C] [DecidableEq O] in
theorem keyOf_spec {a : A} {key : Nat} (h : S.keyOf X LC LO a = some key) :
    X.ta a = false ∧ S.wr LC LO a ≠ [] ∧ S.FoldSpec X.tv a key ∧
      ∃ q, (S.params a)[key]? = some q ∧ S.keepsP X LO q = true := by
  unfold keyOf at h
  split at h
  · rename_i hc
    obtain ⟨h1, h2, _⟩ := hc
    simp only [Option.bind_eq_some_iff] at h
    obtain ⟨k, hk, h⟩ := h
    split at h
    · rename_i q hq
      split at h
      · rename_i hkp
        cases h
        exact ⟨h1, fun he => by simp [he] at h2, S.foldKey_spec hk, q, hq, hkp⟩
      · cases h
    · cases h
  · cases h

end Sys

end RdraIsh.SymAct

namespace RdraIsh.Act.Ext

/-- Step (a) of Lemma 6 for the data `D`: `Fold::holds` computes the bit exactly
(`Lemma6Hyp.exact`). -/
def Exact {V E A Val K : Type} {M : Model V E A Val} (D : Ext M K) : Prop :=
  ∀ a F, D.fold a = some F → ∀ t s c, D.bitOf (F.bit c) t = true ↔
    ∃ b : Binding, b.slot F.key = c ∧ D.FoldConds a F b (join D.tv (t, s))

end RdraIsh.Act.Ext

namespace RdraIsh.SymAct

open LTS
open Act (Role Param Binding Rows Conj Model Local Agree roleConj roleConjs)

variable {K E C O A : Type}

namespace Sys

section Fields

variable {S : Sys K E C O A} [DecidableEq E] [DecidableEq C] [DecidableEq O]
  {LE : List E} {LC : List C} {LO : List O} {LA : List A}
  {hE : ∀ e, e ∈ LE} {hC : ∀ c, c ∈ LC} {hO : ∀ o, o ∈ LO} {hA : ∀ a, a ∈ LA}
  {X : Cut E C O A} {Kb : Type} {keys : List Kb} {keys_all : ∀ k, k ∈ keys}
  {bitKey : A → Nat → Kb} {bitOf : Kb → Act.TV X.tv (Val := Val) → Bool}
  {dflt : Var E C O → Val}

local notation "M" => S.model LE LC LO LA hE hC hO hA
local notation "D" => S.ext LE LC LO LA hE hC hO hA X keys keys_all bitKey bitOf dflt

theorem fold_some {a : A} {key : Nat} (hk : S.keyOf X LC LO a = some key) :
    (D).fold a = some ⟨key, S.foldedC X a key, bitKey a⟩ := by
  show Option.map _ (S.keyOf X LC LO a) = _
  rw [hk]; rfl

theorem fold_none {a : A} (hk : S.keyOf X LC LO a = none) : (D).fold a = none := by
  show Option.map _ (S.keyOf X LC LO a) = _
  rw [hk]; rfl

theorem fold_eq {a : A} {F : Act.Fold (Var E C O) Val Kb} (h : (D).fold a = some F) :
    ∃ key, S.keyOf X LC LO a = some key ∧ F = ⟨key, S.foldedC X a key, bitKey a⟩ := by
  change Option.map _ (S.keyOf X LC LO a) = some F at h
  cases hk : S.keyOf X LC LO a with
  | none => rw [hk] at h; cases h
  | some key => rw [hk] at h; cases h; exact ⟨key, rfl, rfl⟩

theorem bind_of_free {a : A} {key : Nat} (hk : S.keyOf X LC LO a = some key) {b b' : Binding}
    (hb : Act.BindAgree ((D).free a) b b') {j : Nat}
    (hj : isT X.tv (S.params a) j = false ∨ j = key) : b.slot j = b'.slot j := by
  refine hb.1 j fun ⟨F, hF, ⟨p, hp, ht⟩, hne⟩ => ?_
  obtain ⟨key', hk', rfl⟩ := fold_eq hF
  rw [hk] at hk'
  cases hk'
  rcases hj with hj | rfl
  · have : isT X.tv (S.params a) j = true := isT_iff.2 ⟨p, hp, ht⟩
    rw [hj] at this; cases this
  · exact hne rfl

theorem foldedC_og {a : A} {key : Nat} {g : Conj (Var E C O) Val} (hg : g ∈ S.foldedC X a key) :
    g ∈ S.og a := by
  rcases List.mem_append.1 hg with h | h
  · obtain ⟨j, q, hq, _, rfl⟩ := S.mem_boundsWhere_inv _ 0 _ g h
    exact List.mem_append_left _ (S.mem_boundConjs 0 _ j q hq)
  · obtain ⟨φ, hφ, rfl⟩ := List.mem_map.1 h
    exact List.mem_append_right _ (List.mem_cons_of_mem _
      (List.mem_map.2 ⟨φ, (List.mem_filter.1 hφ).1, rfl⟩))

theorem foldedC_names {a : A} {key : Nat} (hk : S.keyOf X LC LO a = some key)
    {g : Conj (Var E C O) Val} (hg : g ∈ S.foldedC X a key) {b b' : Binding}
    {σ : Var E C O → Val} (hb : ∀ j, ((D).tParam a j ∨ j = key) → b.slot j = b'.slot j)
    (h : g.holds b σ) : g.holds b' σ := by
  obtain ⟨_, _, hs, _⟩ := S.keyOf_spec X LC LO hk
  rcases List.mem_append.1 hg with hg | hg
  · obtain ⟨j, q, hq, hP, rfl⟩ := S.mem_boundsWhere_inv _ 0 _ g hg
    have heq : b.slot (0 + j) = b'.slot (0 + j) := hb _ (by
      simp only [Bool.or_eq_true, beq_iff_eq] at hP
      rcases hP with hP | hP
      · exact .inl ⟨q, by rw [Nat.zero_add]; exact hq, hP⟩
      · exact .inr hP)
    show b'.slot (0 + j) < _
    rw [← heq]; exact h
  · obtain ⟨φ, hφ, rfl⟩ := List.mem_map.1 hg
    obtain ⟨hφg, hf⟩ := List.mem_filter.1 hφ
    have hn := hs.no_next φ hφg (by simp only [foldedG, Bool.and_eq_true] at hf; exact hf.1)
    have hps : ∀ p ∈ predPs φ, b.slot p = b'.slot p := fun p hp => by
      cases hT : isT X.tv (S.params a) p
      · exact hb p (.inr (hs.folded φ hφg hf p hp hT))
      · exact hb p (.inl (isT_iff.1 hT))
    have e1 := S.eval_succ (s := dec σ) (t := S.post a b' (dec σ)) (t' := S.post a b (dec σ))
      (b := b'.slot) φ [] hn
    have e2 := S.eval_bind (s := dec σ) (t := S.post a b (dec σ)) φ [] hps
    show S.eval (dec σ) (S.post a b' (dec σ)) b'.slot [] φ = true
    rw [e1, ← e2]; exact h

theorem og_names {a : A} {key : Nat} (hk : S.keyOf X LC LO a = some key)
    {g : Conj (Var E C O) Val} (hg : g ∈ S.og a) (hn : g ∉ S.foldedC X a key) {b b' : Binding}
    {σ : Var E C O → Val} (hb : Act.BindAgree ((D).free a) b b') (h : g.holds b σ) :
    g.holds b' σ := by
  obtain ⟨_, _, hs, _⟩ := S.keyOf_spec X LC LO hk
  have hfree := fun j (hj : isT X.tv (S.params a) j = false ∨ j = key) => bind_of_free hk hb hj
  have heff : ∀ x ∈ S.effs a, ∀ p ∈ effPs x, b.slot p = b'.slot p := fun x hx p hp => hfree p (by
    cases hT : isT X.tv (S.params a) p
    · exact .inl rfl
    · exact .inr (hs.effs x hx p hp hT))
  simp only [og, List.mem_append, List.mem_cons, List.mem_map] at hg
  rcases hg with hg | rfl | ⟨φ, hφ, rfl⟩
  · obtain ⟨j, q, hq, rfl⟩ := S.mem_boundConjs_inv 0 _ g hg
    by_cases hP : (X.tv (.ex q.entity) || (0 + j) == key) = true
    · exact absurd (List.mem_append_left _ (S.mem_boundsWhere _ 0 _ j q hq hP)) hn
    · have heq : b.slot (0 + j) = b'.slot (0 + j) := hfree _ (.inl (by
        simp only [Bool.or_eq_true, beq_iff_eq, not_or, Bool.not_eq_true] at hP
        unfold isT
        rw [Nat.zero_add, hq]; exact hP.1))
      show b'.slot (0 + j) < _
      rw [← heq]; exact h
  · exact Sys.picks_bind heff hb.2 h
  · by_cases hf : foldedG X.tv (S.params a) φ = true
    · exact absurd (List.mem_append_right _
        (List.mem_map.2 ⟨φ, List.mem_filter.2 ⟨hφ, hf⟩, rfl⟩)) hn
    · have hps : ∀ p ∈ predPs φ, b.slot p = b'.slot p := fun p hp => hfree p (by
        cases hT : isT X.tv (S.params a) p
        · exact .inl rfl
        · right
          have htn : tNamed X.tv (S.params a) φ = true := List.any_eq_true.2 ⟨p, hp, hT⟩
          have hr : readsT X.tv φ = false := by simpa [foldedG, htn] using hf
          exact (linkOf_spec (hs.link φ hφ htn hr)).2 p hp hT)
      show S.eval (dec σ) (S.post a b' (dec σ)) b'.slot [] φ = true
      rw [← S.post_bind (s := dec σ) heff hb.2, ← S.eval_bind φ [] hps]
      exact h

theorem eff_bind {a : A} {key : Nat} (hk : S.keyOf X LC LO a = some key) {b b' : Binding}
    (σ : Var E C O → Val) (hb : Act.BindAgree ((D).free a) b b') (v : Var E C O) :
    (M).eff a b σ v = (M).eff a b' σ v := by
  obtain ⟨_, _, hs, _⟩ := S.keyOf_spec X LC LO hk
  have heff : ∀ x ∈ S.effs a, ∀ p ∈ effPs x, b.slot p = b'.slot p := fun x hx p hp =>
    bind_of_free hk hb (by
      cases hT : isT X.tv (S.params a) p
      · exact .inl rfl
      · exact .inr (hs.effs x hx p hp hT))
  show S.eff LC LO a b σ v = S.eff LC LO a b' σ v
  simp only [eff]
  rw [S.post_bind (s := dec σ) heff hb.2]

theorem eff_side {obs : Var E C O → Prop} (h5 : Act.Lemma5ab (M) X.tv X.ta obs) {a : A}
    (ha : Act.Sa (M) X.ta a) (b : Binding) {σ τ : Var E C O → Val}
    (hag : Act.AgreeOn (fun v => X.tv v = false ∨ X.inputs v = true) σ τ) (v : Var E C O)
    (hv : v ∈ (M).wr a) : (M).eff a b σ v = (M).eff a b τ v := by
  have hs : X.tv v = false := h5.a a ha v hv
  have hv' : v ∈ S.wr LC LO a := hv
  show S.eff LC LO a b σ v = S.eff LC LO a b τ v
  simp only [eff, hv', ↓reduceIte]
  exact S.post_pt a b (enc_dec_congr (hag v (.inl hs)))

theorem eff_inputs (a : A) (b : Binding) {σ τ : Var E C O → Val}
    (hag : Act.AgreeOn (fun v => X.tv v = false ∨ X.inputs v = true) σ τ) (v : Var E C O)
    (hv : X.inputs v = true) : (M).eff a b σ v = (M).eff a b τ v := by
  show S.eff LC LO a b σ v = S.eff LC LO a b τ v
  simp only [eff]
  split
  · exact S.post_pt a b (enc_dec_congr (hag v (.inr hv)))
  · exact hag v (.inr hv)

theorem keptOg_rd {a : A} {g : Conj (Var E C O) Val} (hg : g ∈ S.keptOg X LO a) :
    ∀ v ∈ g.rd, X.tv v = false ∨ X.inputs v = true := by
  intro v hv
  rw [← Cut.inV_iff]
  simp only [keptOg, List.mem_append] at hg
  rcases hg with (hg | hg) | hg
  · obtain ⟨j, q, _, _, rfl⟩ := S.mem_boundsWhere_inv _ 0 _ g hg
    simp [boundConj] at hv
  · split at hg
    · rename_i hc
      rw [List.mem_singleton] at hg
      subst hg
      simp only [Bool.and_eq_true, List.all_eq_true] at hc
      exact hc.1 v hv
    · cases hg
  · obtain ⟨φ, hφ, rfl⟩ := List.mem_map.1 hg
    simp only [List.mem_filter, Bool.and_eq_true, List.all_eq_true] at hφ
    exact hφ.2.1 v hv

end Fields

section Main

variable {S : Sys K E C O A} [DecidableEq E] [DecidableEq C] [DecidableEq O]
  {LE : List E} {LC : List C} {LO : List O} {LA : List A}
  {hE : ∀ e, e ∈ LE} {hC : ∀ c, c ∈ LC} {hO : ∀ o, o ∈ LO} {hA : ∀ a, a ∈ LA}
  {X : Cut E C O A} {Kb : Type} {keys : List Kb} {keys_all : ∀ k, k ∈ keys}
  {bitKey : A → Nat → Kb} {bitOf : Kb → Act.TV X.tv (Val := Val) → Bool}
  {dflt : Var E C O → Val} {obs : Var E C O → Prop}

local notation "M" => S.model LE LC LO LA hE hC hO hA
local notation "D" => S.ext LE LC LO LA hE hC hO hA X keys keys_all bitKey bitOf dflt

/-- What the split and its inputs give Lemma 6, beyond Lemma 5 (a)(b):

- `col_side`: Definition 3's side rule for columns: a column of an entity whose existence is
  in `T_v` is in `T_v` (`split_faults` checks the rule, `Clause::Sides`; `Closed.down` is
  the rule);
- `inputs_col`: the existence of a column's entity is an input when the column is
  (`split_with`'s `inputs_of` adds the `existences` of every input);
- `raw`: every raw read of an `S_a` action (`raw_reads`) is in `S_v` or an input
  (`inputs_of` holds the raw reads of `T_v`; `split_faults` checks it, `Clause::C`). -/
structure Inputs (S : Sys K E C O A) (X : Cut E C O A) (LC : List C) (LO : List O)
    (wr : A → List (Var E C O)) : Prop where
  col_side : ∀ c, X.tv (.ex (S.cent c)) = true → X.tv (.col c) = true
  inputs_col : ∀ c, X.inputs (.col c) = true → X.inputs (.ex (S.cent c)) = true
  raw : ∀ a, X.ta a = false → wr a ≠ [] → ∀ v ∈ S.rawRd X LC LO a, X.inV v = true

/-- An unfolded `S_a` action drops none of its parameters: a parameter whose role reads its
row is on `S_v` or an input by its raw read, a `New` one is created on `S_v` (`Built`,
Lemma 5 (a)), and an `Earlier` one carries a mark on its entity (`Built`), whose read puts
the entity's existence on `S_v` or among the inputs (`Inputs`). -/
theorem unfolded_keeps (hb : S.Built) (h5 : Act.Lemma5ab (M) X.tv X.ta obs)
    (hI : S.Inputs X LC LO (S.wr LC LO)) {a : A} (ha : Act.Sa (M) X.ta a)
    (hk : S.keyOf X LC LO a = none) {j : Nat} {q : Param E} (hq : (S.params a)[j]? = some q) :
    S.keepsP X LO q = true := by
  have hr : ∀ v ∈ S.rd a, X.inV v = true := fun v hv =>
    hI.raw a ha.1 ha.2 v (by rw [S.rawRd_none X LC LO hk]; exact hv)
  unfold keepsP
  cases hrole : q.role with
  | new =>
    have hw : Var.ex q.entity ∈ (M).wr a :=
      S.mem_wr LC LO (hb.new_create a j q hq hrole) (by simp [effWr, pent, hq])
    have := h5.a a ha _ hw
    simp [Cut.inV, this]
  | earlier st =>
    obtain ⟨m, k, hm, hcent, _⟩ := hb.earlier_mark a j q st hq hrole
    have hv : (Var.col m : Var E C O) ∈ S.rd a := S.rd_guard hm (by simp [predRd, termRd])
    rcases Cut.inV_iff.1 (hr _ hv) with h | h
    · have hx : X.tv (.ex (S.cent m)) = false := by
        cases h' : X.tv (.ex (S.cent m))
        · rfl
        · rw [hI.col_side m h'] at h; cases h
      rw [hcent] at hx
      simp [Cut.inV, hx]
    · have hx := hI.inputs_col m h
      rw [hcent] at hx
      simp [Cut.inV, hx]
  | existing =>
    have := hr _ (S.rd_param (List.mem_iff_getElem?.2 ⟨j, hq⟩) (by simp [hrole, Role.needsPresent]))
    simp [this]
  | parent =>
    have := hr _ (S.rd_param (List.mem_iff_getElem?.2 ⟨j, hq⟩) (by simp [hrole, Role.needsPresent]))
    simp [this]
  | triggered =>
    have := hr _ (S.rd_param (List.mem_iff_getElem?.2 ⟨j, hq⟩) (by simp [hrole, Role.needsPresent]))
    simp [this]

/-- `Lemma6Hyp.reads`, Lemma 5 (c) with the folds' reads: a read of an `S_a` action is raw,
or a folded guard's, or the existence of a `T` parameter. -/
theorem rd_split (hI : S.Inputs X LC LO (S.wr LC LO)) {a : A} (ha : Act.Sa (M) X.ta a) :
    ∀ v ∈ (M).rd a, X.tv v = false ∨ X.inputs v = true ∨
      ∃ F, (D).fold a = some F ∧ (D).foldReads a F v := by
  intro v hv
  have raw : v ∈ S.rawRd X LC LO a →
      X.tv v = false ∨ X.inputs v = true ∨ ∃ F, (D).fold a = some F ∧ (D).foldReads a F v :=
    fun hv' => by
      rcases Cut.inV_iff.1 (hI.raw a ha.1 ha.2 v hv') with h | h
      · exact .inl h
      · exact .inr (.inl h)
  cases hk : S.keyOf X LC LO a with
  | none => exact raw (by rw [S.rawRd_none X LC LO hk]; exact hv)
  | some key =>
    have hF := fold_some (LE := LE) (LA := LA) (hE := hE) (hC := hC) (hO := hO) (hA := hA)
      (keys := keys) (keys_all := keys_all) (bitKey := bitKey) (bitOf := bitOf) (dflt := dflt) hk
    rcases S.rd_cases hv with ⟨q, hq, hn, rfl⟩ | ⟨φ, hφ, hv⟩ | ⟨x, hx, hv⟩
    · cases ht : X.tv (.ex q.entity)
      · exact .inl rfl
      · obtain ⟨j, hj⟩ := List.mem_iff_getElem?.1 hq
        exact .inr (.inr ⟨_, hF, .inr ⟨j, q, hj, ht, rfl⟩⟩)
    · cases hf : foldedG X.tv (S.params a) φ
      · exact raw (S.rawRd_guard X LC LO hk hφ hf hv)
      · exact .inr (.inr ⟨_, hF, .inl ⟨S.predConj a φ, List.mem_append_right _
          (List.mem_map.2 ⟨φ, List.mem_filter.2 ⟨hφ, hf⟩, rfl⟩), hv⟩⟩)
    · exact raw (S.rawRd_eff X LC LO hx hv)

/-- `Lemma6Hyp.cover`: every conjunct of an `S_a` action besides its roles is kept or
folded. -/
theorem og_cover (hb : S.Built) (h5 : Act.Lemma5ab (M) X.tv X.ta obs)
    (hI : S.Inputs X LC LO (S.wr LC LO)) {a : A} (ha : Act.Sa (M) X.ta a)
    {g : Conj (Var E C O) Val} (hg : g ∈ S.og a) :
    g ∈ S.keptOg X LO a ∨ ∃ F, (D).fold a = some F ∧ g ∈ F.folded := by
  have hr := hI.raw a ha.1 ha.2
  simp only [og, List.mem_append, List.mem_cons, List.mem_map] at hg
  cases hk : S.keyOf X LC LO a with
  | none =>
    have hkp := fun j q (hq : (S.params a)[j]? = some q) => unfolded_keeps hb h5 hI ha hk hq
    have hpos : ∀ j, S.keepsPos X LO a j = true := fun j => by
      unfold keepsPos
      split
      · exact hkp _ _ ‹_›
      · rfl
    rw [S.rawRd_none X LC LO hk] at hr
    left
    rcases hg with hg | rfl | ⟨φ, hφ, rfl⟩
    · obtain ⟨j, q, hq, rfl⟩ := S.mem_boundConjs_inv 0 _ g hg
      exact List.mem_append_left _ (List.mem_append_left _
        (S.mem_boundsWhere _ 0 _ j q hq (hkp _ _ hq)))
    · refine List.mem_append_left _ (List.mem_append_right _ ?_)
      have hc : (((S.effs a).flatMap effRd).all X.inV &&
          ((S.effs a).flatMap effPs).all (S.keepsPos X LO a)) = true := by
        simp only [Bool.and_eq_true, List.all_eq_true, List.mem_flatMap]
        exact ⟨fun v ⟨x, hx, hv⟩ => hr v (S.rd_eff hx hv), fun j _ => hpos j⟩
      simp only [hc, ↓reduceIte]
      exact List.mem_singleton_self _
    · refine List.mem_append_right _ (List.mem_map.2 ⟨φ, List.mem_filter.2 ⟨hφ, ?_⟩, rfl⟩)
      simp only [Bool.and_eq_true, List.all_eq_true]
      exact ⟨fun v hv => hr v (S.rd_guard hφ hv), fun j _ => hpos j⟩
  | some key =>
    obtain ⟨_, _, hs, qk, hqk, hkk⟩ := S.keyOf_spec X LC LO hk
    have hF := fold_some (LE := LE) (LA := LA) (hE := hE) (hC := hC) (hO := hO) (hA := hA)
      (keys := keys) (keys_all := keys_all) (bitKey := bitKey) (bitOf := bitOf) (dflt := dflt) hk
    have hpos : ∀ j, (isT X.tv (S.params a) j = false ∨ j = key) →
        S.keepsPos X LO a j = true := by
      intro j hj
      unfold keepsPos
      split
      · rename_i q hq
        rcases hj with hj | rfl
        · unfold isT at hj
          rw [hq] at hj
          simp [keepsP, Cut.inV, hj]
        · rw [hq] at hqk
          cases hqk
          exact hkk
      · rfl
    rcases hg with hg | rfl | ⟨φ, hφ, rfl⟩
    · obtain ⟨j, q, hq, rfl⟩ := S.mem_boundConjs_inv 0 _ g hg
      by_cases hP : (X.tv (.ex q.entity) || (0 + j) == key) = true
      · exact .inr ⟨_, hF, List.mem_append_left _ (S.mem_boundsWhere _ 0 _ j q hq hP)⟩
      · left
        refine List.mem_append_left _ (List.mem_append_left _
          (S.mem_boundsWhere _ 0 _ j q hq ?_))
        simp only [Bool.or_eq_true, beq_iff_eq, not_or, Bool.not_eq_true] at hP
        simp [keepsP, Cut.inV, hP.1]
    · left
      refine List.mem_append_left _ (List.mem_append_right _ ?_)
      have hc : (((S.effs a).flatMap effRd).all X.inV &&
          ((S.effs a).flatMap effPs).all (S.keepsPos X LO a)) = true := by
        simp only [Bool.and_eq_true, List.all_eq_true, List.mem_flatMap]
        refine ⟨fun v ⟨x, hx, hv⟩ => hr v (S.rawRd_eff X LC LO hx hv),
          fun j ⟨x, hx, hj⟩ => hpos j ?_⟩
        cases hT : isT X.tv (S.params a) j
        · exact .inl rfl
        · exact .inr (hs.effs x hx j hj hT)
      simp only [hc, ↓reduceIte]
      exact List.mem_singleton_self _
    · by_cases hf : foldedG X.tv (S.params a) φ = true
      · exact .inr ⟨_, hF, List.mem_append_right _
          (List.mem_map.2 ⟨φ, List.mem_filter.2 ⟨hφ, hf⟩, rfl⟩)⟩
      · left
        refine List.mem_append_right _ (List.mem_map.2 ⟨φ, List.mem_filter.2 ⟨hφ, ?_⟩, rfl⟩)
        simp only [Bool.and_eq_true, List.all_eq_true]
        refine ⟨fun v hv => hr v (S.rawRd_guard X LC LO hk hφ (by simpa using hf) hv),
          fun j hj => hpos j ?_⟩
        cases hT : isT X.tv (S.params a) j
        · exact .inl rfl
        · right
          have htn : tNamed X.tv (S.params a) φ = true := List.any_eq_true.2 ⟨j, hj, hT⟩
          have hrd : readsT X.tv φ = false := by simpa [foldedG, htn] using hf
          exact (linkOf_spec (hs.link φ hφ htn hrd)).2 j hj hT

/-- **`Lemma6Hyp` from the syntax.** For the data `ext` reads off a slot system and a split,
every field of `Lemma6Hyp` follows from what the builder gives (`Built`), Lemma 5 (a)(b),
the side rule and the code's inputs (`Inputs`), and step (a) (`Exact`, that `Fold::holds`
computes the bit). `local_`, `shape` and `new_writes` are `SymActModel.lean`'s; `bind` holds
as `ext` runs `bind_stub_params`. -/
theorem lemma6Hyp [DecidableEq Kb] (hb : S.Built) (h5 : Act.Lemma5ab (M) X.tv X.ta obs)
    (hI : S.Inputs X LC LO (S.wr LC LO)) (hx : Act.Ext.Exact (D)) : Act.Lemma6Hyp (D) obs where
  local_ := local_ hb
  lemma5 := h5
  reads _ ha := rd_split hI ha
  shape := S.shape (D) rfl rfl rfl
  new_writes a j q hq hr := S.new_writes hb a j q hq hr
  kept_rd _ _ hg := keptOg_rd hg
  cover _ ha _ hg := og_cover hb h5 hI ha hg
  rise_eff a b _ _ _ hag v hv := eff_inputs a b hag v hv
  fold_sa a F hF := by
    obtain ⟨key, hk, _⟩ := fold_eq hF
    obtain ⟨h1, h2, _⟩ := S.keyOf_spec X LC LO hk
    exact ⟨h1, h2⟩
  folded_og a F hF g hg := by
    obtain ⟨key, _, rfl⟩ := fold_eq hF
    exact foldedC_og hg
  exact := hx
  folded_names a F hF g hg b b' σ hbb h := by
    obtain ⟨key, hk, rfl⟩ := fold_eq hF
    exact foldedC_names hk hg hbb h
  other_names a F hF g hg hn b b' σ hbb h := by
    obtain ⟨key, hk, rfl⟩ := fold_eq hF
    exact og_names hk hg hn hbb h
  eff_names a F hF b b' σ hbb v _ := by
    obtain ⟨key, hk, rfl⟩ := fold_eq hF
    exact eff_bind hk σ hbb v
  eff_s a F hF b σ τ hag v hv := by
    obtain ⟨key, hk, rfl⟩ := fold_eq hF
    obtain ⟨h1, h2, _⟩ := S.keyOf_spec X LC LO hk
    exact eff_side h5 ⟨h1, h2⟩ b hag v hv
  bind := rfl

/-- **Lemma 6 with the code's stages, from the syntax**: under `Built`, Lemma 5 (a)(b), the
side rule and the code's inputs, and step (a), the cone of the writing actions and the
extended sub-lifecycle `ext` reads off the syntax meet `Lemma6Staged`. -/
theorem lemma6_of_syntax [DecidableEq Kb] (hb : S.Built) (h5 : Act.Lemma5ab (M) X.tv X.ta obs)
    (hI : S.Inputs X LC LO (S.wr LC LO)) (hx : Act.Ext.Exact (D)) :
    Lemma6Staged (Act.coneW (M) X.tv X.ta) (D).ext (D).inp (D).stages :=
  Act.Ext.lemma6_staged (lemma6Hyp hb h5 hI hx)

/-- **Theorem A8 with the code's stages, from the syntax**: under the hypotheses of
`lemma6_of_syntax`, (B1)–(B3) of a partition of `ext` and a complete rise table, A8's relation
is a divergence-blind branching bisimulation between the cone of the writing actions and the
reduced cone, and preserves `T_v` and `obs`. -/
theorem a8_of_syntax [DecidableEq Kb] (hb : S.Built) (h5 : Act.Lemma5ab (M) X.tv X.ta obs)
    (hI : S.Inputs X LC LO (S.wr LC LO)) (hx : Act.Ext.Exact (D)) {Blk : Type}
    {B : (D).XS → Blk} {rep : Blk → (D).XS} {rt : Blk → (D).Inp → Option Blk}
    (hX : Stable (D).ext (Act.obsMap X.tv h5.obs_s) B rep) (ht : RiseTable (D).ext B rep rt) :
    BranchBisim (Act.coneW (M) X.tv X.ta)
        (reducedStaged (Act.coneW (M) X.tv X.ta) (D).ext (D).inp B rep rt (D).stages)
        tLabel tLabel (RdraIsh.rel (Act.coneW (M) X.tv X.ta) (D).inp B) ∧
      PreservesObs (RdraIsh.rel (Act.coneW (M) X.tv X.ta) (D).inp B)
        (obsC (Act.obsMap X.tv h5.obs_s)) (obsR (Act.obsMap X.tv h5.obs_s) rep) :=
  Act.a8_actions (lemma6Hyp hb h5 hI hx) hX ht

/-- Lemma 5 (a)(b) and the side rule from Definition 3: for a closed split, as `split_at`
computes it, `lemma6_of_syntax` needs of the split only the code's inputs (`raw`,
`inputs_col`) and step (a). -/
theorem inputs_of_closed {mono : Var E C O → Bool} {keep : A → Bool}
    {anchor : Option (Var E C O)} (hc : Act.Closed (M) mono keep anchor X.tv X.ta)
    (hio : ∀ c, X.inputs (.col c) = true → X.inputs (.ex (S.cent c)) = true)
    (hraw : ∀ a, X.ta a = false → S.wr LC LO a ≠ [] → ∀ v ∈ S.rawRd X LC LO a,
      X.inV v = true) :
    S.Inputs X LC LO (S.wr LC LO) where
  col_side c h := hc.down (.col c) (.ex (S.cent c)) rfl h
  inputs_col := hio
  raw := hraw

/-! ## Step (a) for the bits read off the syntax -/

/-- One bit per action and slot below `B`; `none` stands for every slot from `B` on, where
no bit holds. -/
def bitKeyB (B : Nat) (a : A) (c : Nat) : Option (A × Fin B) :=
  if h : c < B then some (a, ⟨c, h⟩) else none

/-- Every bit of `bitKeyB`. -/
def keysB (LA : List A) (B : Nat) : List (Option (A × Fin B)) :=
  none :: LA.flatMap fun a => (List.finRange B).map fun i => some (a, i)

theorem keysB_all {LA : List A} (hA : ∀ a, a ∈ LA) (B : Nat) :
    ∀ k : Option (A × Fin B), k ∈ keysB LA B
  | none => List.mem_cons_self
  | some (a, i) => List.mem_cons_of_mem _ (List.mem_flatMap.2 ⟨a, hA a,
      List.mem_map.2 ⟨i, List.mem_finRange i, rfl⟩⟩)

variable (S) in
/-- What `Fold::holds` checks of a binding, on the syntax: each `T` parameter whose role needs
a present row is on one, and every conjunct of the fold holds (`Ext.FoldConds` of `ext`). -/
def FC (X : Cut E C O A) (a : A) (key : Nat) (b : Binding) (σ : Var E C O → Val) : Prop :=
  (∀ j p, (S.params a)[j]? = some p → X.tv (.ex p.entity) = true →
    p.role.needsPresent = true → rowsV.present (σ (.ex p.entity)) (b.slot j)) ∧
  ∀ g ∈ S.foldedC X a key, g.holds b σ

variable (S) in
open Classical in
/-- **The bit `Fold::holds` computes**, for the fold of `a` at slot `c`: some binding with the
key at `c` meets `FC` at `t` (the `S_v` part, here `dflt`, does not matter: `fc_t`). -/
noncomputable def holdsB (X : Cut E C O A) (LC : List C) (LO : List O) (B : Nat)
    (dflt : Var E C O → Val) : Option (A × Fin B) → Act.TV X.tv (Val := Val) → Bool
  | none, _ => false
  | some (a, i), t => decide (∃ key, S.keyOf X LC LO a = some key ∧ ∃ b : Binding,
      b.slot key = i.val ∧ S.FC X a key b (Act.join X.tv (t, fun v => dflt v.1)))

/-- `FC` reads only `T_v`. -/
theorem fc_t {a : A} {key : Nat} {b : Binding} (t : Act.TV X.tv (Val := Val))
    (s s' : Act.SV X.tv (Val := Val)) :
    S.FC X a key b (Act.join X.tv (t, s)) → S.FC X a key b (Act.join X.tv (t, s')) := by
  rintro ⟨hp, hg⟩
  refine ⟨fun j p hj ht hn => ?_, fun g hm => ?_⟩
  · have := hp j p hj ht hn
    rw [Act.join_t ht] at this ⊢
    exact this
  · have hh := hg g hm
    rcases List.mem_append.1 hm with h | h
    · obtain ⟨j, q, _, _, rfl⟩ := S.mem_boundsWhere_inv _ 0 _ g h
      exact hh
    · obtain ⟨φ, hφ, rfl⟩ := List.mem_map.1 h
      simp only [List.mem_filter, foldedG, readsT, Bool.and_eq_true, List.all_eq_true] at hφ
      have hs : VAgree (predRd φ) (dec (Act.join X.tv (t, s))) (dec (Act.join X.tv (t, s'))) :=
        VAgree.of_dec fun v hv => by rw [Act.join_t (hφ.2.2 v hv), Act.join_t (hφ.2.2 v hv)]
      show S.eval _ (S.post a b _) b.slot [] φ = true
      rw [← S.eval_congr b.slot φ [] hs (S.post_agree a b hs)]
      exact hh

/-- **Step (a) for the bits of `holdsB`**: when every class has at most `B` slots, the bit
`holdsB` gives the fold of `a` at slot `c` holds iff some binding with the key at `c` meets
`FoldConds` (`Exact`). So the one hypothesis on the bits that `lemma6_of_syntax` keeps is
that the code's `Fold::holds` is `holdsB`. -/
theorem exact_holdsB {B : Nat} (hB : ∀ κ, S.n κ ≤ B) :
    Act.Ext.Exact (S.ext LE LC LO LA hE hC hO hA X (keysB LA B) (keysB_all hA B) (bitKeyB B)
      (S.holdsB X LC LO B dflt) dflt) := by
  intro a F hF t s c
  obtain ⟨key, hk, rfl⟩ := fold_eq hF
  show S.holdsB X LC LO B dflt (bitKeyB B a c) t = true ↔
    ∃ b : Binding, b.slot key = c ∧ S.FC X a key b (Act.join X.tv (t, s))
  by_cases hc : c < B
  · simp only [bitKeyB, hc, ↓reduceDIte, holdsB, decide_eq_true_eq]
    constructor
    · rintro ⟨key', hk', b, hbk, hfc⟩
      rw [hk] at hk'
      cases hk'
      exact ⟨b, hbk, fc_t t _ s hfc⟩
    · rintro ⟨b, hbk, hfc⟩
      exact ⟨key, hk, b, hbk, fc_t t s _ hfc⟩
  · simp only [bitKeyB, hc, ↓reduceDIte, holdsB, Bool.false_eq_true, false_iff]
    rintro ⟨b, hbk, -, hg⟩
    obtain ⟨_, _, _, q, hq, _⟩ := S.keyOf_spec X LC LO hk
    have hm : S.boundConj (0 + key) q ∈ S.foldedC X a key :=
      List.mem_append_left _ (S.mem_boundsWhere _ 0 _ key q hq (by simp))
    have := hg _ hm
    simp only [boundConj, Nat.zero_add] at this
    have := hB (S.cls q.entity)
    omega

end Main

end Sys

end RdraIsh.SymAct
