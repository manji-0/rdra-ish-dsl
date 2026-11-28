import RdraIsh.Lemma6

/-!
# A concrete instance of `Lemma6Hyp`, with a fold, stub parameters and a rise

Five variables over `Nat` and three entities:

- `ds`, the existence of a desk (`T`, an input), `sh`, the existence of a shop (`T`, a stub:
  `X` does not hold it), `tf`, a `T` flag no `S_a` action reads raw, `st`, an `S` variable,
  and `so`, the existence of an order (`S`);
- `note` (`S_a`, folded) binds an `Existing` desk (the fold's key, on an input) and an
  `Existing` shop (a free `T` parameter on a stub), needs `tf = 1`, and sets `st`. Its fold
  bit holds when the desk, the shop and the flag are all `1`;
- `mark` (`S_a`, unfolded) binds an `Existing` desk (on an input) and an `Existing` order (on
  `S`), needs `ds = 1` (a conjunct `X` keeps), and sets `st`;
- `open_` (`T_a`, a rise) binds a `Parent` shop (on a stub) and sets `ds`;
- `setf` (`T_a`, a bit writer) sets `tf`.

`hyp` discharges `Lemma6Hyp` for `D true`, so `lemma6` (Lemma 6 with the code's stages) is a
theorem here. `rise_from_init` is a reachable `T_a` step that changes the raw inputs,
`fold_step` a reachable step of `note` matched by a silent edge of `X` whose bit is true, and
`mark_step` a reachable step of `mark` taken to `X` and back. `stub_bug` is the stub-parameter
bug: with `bind_stub_params` off (`D false`), `X` keeps the shop parameter's `Parent` role,
which `X`'s constant absent shop never meets, so the rise of `open_` is missing and the
`C ⇒ X` half of Lemma 6 fails. The instance has no parameter on a dropped entity and no `new`
or `earlier` parameter.
-/

namespace RdraIsh.ExampleLemma6

open RdraIsh.Act
open LTS

inductive Var | ds | sh | tf | st | so
  deriving DecidableEq

inductive Ent | desk | shop | order
  deriving DecidableEq

inductive Act | note | open_ | setf | mark
  deriving DecidableEq

def rows : Rows Nat where
  present v _ := v = 1
  lowestAbsent v c := v = 0 ∧ c = 0

def ex : Ent → Var
  | .desk => .ds
  | .shop => .sh
  | .order => .so

def params : Act → List (Param Ent)
  | .note => [⟨.desk, .existing⟩, ⟨.shop, .existing⟩]
  | .open_ => [⟨.shop, .parent⟩]
  | .setf => []
  | .mark => [⟨.desk, .existing⟩, ⟨.order, .existing⟩]

/-- `note`'s folded guard. -/
def tfg : Conj Var Nat := ⟨[.tf], fun _ σ => σ .tf = 1⟩

/-- `mark`'s kept guard, on the input `ds`. -/
def kd : Conj Var Nat := ⟨[.ds], fun _ σ => σ .ds = 1⟩

def og : Act → List (Conj Var Nat)
  | .note => [tfg]
  | .mark => [kd]
  | _ => []

def set1 (x : Var) (σ : Var → Nat) : Var → Nat := fun v => if v = x then 1 else σ v

def M : Model Var Ent Act Nat where
  vars := [.ds, .sh, .tf, .st, .so]
  acts := [.note, .open_, .setf, .mark]
  vars_all v := by cases v <;> simp
  acts_all a := by cases a <;> simp
  own _ := none
  existences _ := []
  counted _ := false
  params := params
  guards a := roleConjs rows ex 0 (params a) ++ og a
  rd
    | .note => [.ds, .sh, .tf]
    | .open_ => [.sh]
    | .setf => []
    | .mark => [.ds, .so]
  wr
    | .note => [.st]
    | .open_ => [.ds]
    | .setf => [.tf]
    | .mark => [.st]
  eff
    | .note => fun _ => set1 .st
    | .open_ => fun _ => set1 .ds
    | .setf => fun _ => set1 .tf
    | .mark => fun _ => set1 .st
  init σ := σ .ds = 0 ∧ σ .sh = 1 ∧ σ .tf = 0 ∧ σ .st = 0 ∧ σ .so = 1

def tv : Var → Bool
  | .st => false
  | .so => false
  | _ => true

def ta : Act → Bool
  | .note => false
  | .mark => false
  | _ => true

def inputs : Var → Bool
  | .ds => true
  | _ => false

def stub : Ent → Bool
  | .shop => true
  | _ => false

def F : Fold Var Nat Unit := ⟨0, [tfg], fun _ => ()⟩

def fold : Act → Option (Fold Var Nat Unit)
  | .note => some F
  | _ => none

/-- The bit: the desk, the shop and the flag are all present. -/
def bitOf (_ : Unit) (t : TV tv (Val := Nat)) : Bool :=
  decide (t ⟨.ds, rfl⟩ = 1 ∧ t ⟨.sh, rfl⟩ = 1 ∧ t ⟨.tf, rfl⟩ = 1)

/-- The data of `Extended::build`, with `bind_stub_params` on or off. -/
def D (bs : Bool) : Ext M Unit where
  tv := tv
  ta := ta
  inputs := inputs
  inputs_t v h := by cases v <;> simp_all [inputs, tv]
  ex := ex
  stub := stub
  rows := rows
  og := og
  kept a g := a = .mark ∧ g = kd
  fold := fold
  keys := [()]
  keys_all _ := List.mem_singleton.mpr rfl
  bitOf := bitOf
  dflt _ := 0
  bindStubs := bs

def obs : Var → Prop := fun _ => False

theorem local_ : Local M where
  conj_rd := by
    intro a g hg v hv
    cases a <;> simp [M, params, og, roleConjs, roleConj, Role.readsRow, ex, tfg, kd] at hg <;>
      rcases hg with rfl | rfl | rfl <;>
      simp at hv <;> (try subst hv) <;> simp [M]
  conj_local := by
    intro a g hg b σ τ hag h
    cases a <;> simp [M, params, og, roleConjs] at hg
    · rcases hg with rfl | rfl | rfl
      · exact roleConj_local _ _ _ _ _ hag h
      · exact roleConj_local _ _ _ _ _ hag h
      · show τ .tf = 1
        rw [← hag .tf (by simp [tfg])]; exact h
    · subst hg; exact roleConj_local _ _ _ _ _ hag h
    · rcases hg with rfl | rfl | rfl
      · exact roleConj_local _ _ _ _ _ hag h
      · exact roleConj_local _ _ _ _ _ hag h
      · show τ .ds = 1
        rw [← hag .ds (by simp [kd])]; exact h
  eff_frame := by
    intro a b σ v hv
    cases a <;> cases v <;> simp [M, set1] at hv ⊢
  eff_local := by
    intro a b σ τ _ v hv
    cases a <;> cases v <;> simp [M, set1] at hv ⊢

theorem lemma5 : Lemma5ab M tv ta obs where
  a a ha v hv := by
    cases a <;> simp [Sa, ta] at ha
    all_goals simp [M] at hv; subst hv; rfl
  b_wr a ha v hv := by
    cases a <;> simp [ta] at ha <;> simp [M] at hv <;> subst hv <;> rfl
  b_rd a ha v hv hs := by
    cases a <;> simp [ta] at ha <;> simp [M] at hv <;> subst hv <;> simp [tv] at hs
  obs_s _ h := h.elim

/-- `note`'s role conditions, as `FoldConds` reads them. -/
theorem foldConds_iff (bs : Bool) (b : Binding) (σ : Var → Nat) :
    (D bs).FoldConds .note F b σ ↔ σ .ds = 1 ∧ σ .sh = 1 ∧ σ .tf = 1 := by
  constructor
  · rintro ⟨hr, hf⟩
    refine ⟨hr 0 _ rfl rfl rfl, hr 1 _ rfl rfl rfl, hf tfg (by simp [F])⟩
  · rintro ⟨h1, h2, h3⟩
    refine ⟨fun j p hp _ _ => ?_, fun g hg => ?_⟩
    · rcases j with _ | _ | j <;> simp [M, params] at hp <;> subst hp
      · exact h1
      · exact h2
    · simp [F] at hg; subst hg; exact h3

theorem hyp : Lemma6Hyp (D true) obs where
  local_ := local_
  lemma5 := lemma5
  reads a ha v hv := by
    cases a <;> simp [Sa, D, ta] at ha <;> simp [M] at hv
    · rcases hv with rfl | rfl | rfl
      · exact .inr (.inl rfl)
      · exact .inr (.inr ⟨F, rfl, .inr ⟨1, ⟨.shop, .existing⟩, rfl, rfl, rfl⟩⟩)
      · exact .inr (.inr ⟨F, rfl, .inl ⟨tfg, by simp [F], by simp [tfg]⟩⟩)
    · rcases hv with rfl | rfl
      · exact .inr (.inl rfl)
      · exact .inl rfl
  shape _ := rfl
  new_writes a j p hp hn := by
    cases a <;> rcases j with _ | _ | j <;> simp [M, params] at hp <;> subst hp <;> cases hn
  kept_rd a g h v hv := by
    obtain ⟨rfl, rfl⟩ := h
    simp [kd] at hv; subst hv; exact .inr rfl
  cover a ha g hg := by
    cases a <;> simp [Sa, D, ta] at ha <;> simp [D, og] at hg <;> subst hg
    · exact .inr ⟨F, rfl, by simp [F]⟩
    · exact .inl ⟨rfl, rfl⟩
  rise_eff a b σ τ _ hag v hv := by
    cases v <;> simp [D, inputs] at hv
    cases a <;> simp [M, set1]
    all_goals exact hag .ds (.inr rfl)
  fold_sa a G hG := by
    cases a <;> simp [D, fold] at hG
    exact ⟨rfl, by simp [M]⟩
  folded_og a G hG g hg := by
    cases a <;> simp [D, fold] at hG
    subst hG; simp [F] at hg; subst hg; simp [D, og]
  exact a G hG t s c := by
    cases a <;> simp [D, fold] at hG
    subst hG
    rw [show (D true).bitOf (F.bit c) t = bitOf () t from rfl]
    constructor
    · intro h
      simp only [bitOf, decide_eq_true_eq] at h
      refine ⟨⟨fun _ => c, fun _ => 0⟩, rfl, (foldConds_iff true _ _).mpr ?_⟩
      simp only [join, D, tv]
      exact h
    · rintro ⟨b, _, hb⟩
      have := (foldConds_iff true _ _).mp hb
      simp only [join, D, tv] at this
      simp only [bitOf, decide_eq_true_eq]
      exact this
  folded_names a G hG g hg b b' σ _ h := by
    cases a <;> simp [D, fold] at hG
    subst hG; simp [F] at hg; subst hg; exact h
  other_names a G hG g hg hn := by
    cases a <;> simp [D, fold] at hG
    subst hG; simp [D, og] at hg; subst hg; exact absurd (by simp [F]) hn
  eff_names a _ _ _ _ _ _ _ _ := by cases a <;> rfl
  eff_s a G hG b σ τ _ v hv := by
    cases a <;> simp [D, fold] at hG
    simp [M] at hv; subst hv; simp [M, set1]
  bind := rfl

/-- **Lemma 6 with the code's stages**, for this cone and its `X`. -/
theorem lemma6 :
    Lemma6Staged (coneW M tv ta) (D true).ext (D true).inp (D true).stages :=
  Ext.lemma6_staged hyp

/-- The initial state: every variable `0` but the shop and the order, which are present. -/
def t0 : TV tv (Val := Nat) := fun v => if v.1 = .sh then 1 else 0
def s0 : SV tv (Val := Nat) := fun v => if v.1 = .so then 1 else 0
/-- After `open_`: the desk is present too. -/
def t1 : TV tv (Val := Nat) := fun v => if v.1 = .sh ∨ v.1 = .ds then 1 else 0
/-- After `setf`: the flag is set too. -/
def t2 : TV tv (Val := Nat) := fun v => if v.1 = .st then 0 else 1

def bind0 : Binding := ⟨fun _ => 0, fun _ => 0⟩

theorem init0 : (coneW M tv ta).Reach (t0, s0) :=
  Reach.init (show M.init (join tv (t0, s0)) by simp [M, join, tv, t0, s0])

theorem step_open : (coneW M tv ta).step (t0, s0) (.inl ⟨(.open_, bind0), rfl⟩) (t1, s0) := by
  refine ⟨fun g hg => ?_, ?_⟩
  · simp [M, params, og, roleConjs] at hg; subst hg
    simp [roleConj, Rows.cand, rows, ex, join, tv, t0]
  · funext v; cases v <;> simp [M, set1, join, tv, t0, t1, s0]

theorem step_setf : (coneW M tv ta).step (t1, s0) (.inl ⟨(.setf, bind0), rfl⟩) (t2, s0) := by
  refine ⟨fun g hg => ?_, ?_⟩
  · simp [M, params, og, roleConjs] at hg
  · funext v; cases v <;> simp [M, set1, join, tv, t1, t2, s0]

/-- A reachable `T_a` step that changes the raw inputs (the desk's existence), so `lemma6`
matches it by a rise of `open_` in `X`, whose shop parameter is on a stub. -/
theorem rise_from_init :
    (coneW M tv ta).Reach (t0, s0) ∧
      (coneW M tv ta).step (t0, s0) (.inl ⟨(.open_, bind0), rfl⟩) (t1, s0) ∧
      ((D true).inp t0).1 ≠ ((D true).inp t1).1 ∧
      RiseThrough (D true).ext (s0, (D true).inp t0)
        ((D true).stages ((D true).inp t0) ((D true).inp t1)) (s0, (D true).inp t1) := by
  refine ⟨init0, step_open, fun h => ?_, lemma6.rise_staged init0 step_open⟩
  have := congrFun h ⟨.ds, rfl⟩
  simp [Ext.inp, D, t0, t1] at this

/-- `note`'s binding: the desk at slot `0`, the shop at slot `5`. -/
def bnote : Binding := ⟨fun j => if j = 1 then 5 else 0, fun _ => 0⟩

def lnote : SaL ta := ⟨(.note, bnote), rfl⟩

/-- After `note`. -/
def s1 : SV tv (Val := Nat) := fun _ => 1

/-- A reachable `S_a` step of the folded `note`, with the shop parameter at slot `5`: `X` has
the silent edge by the same action and binding, and its bit is true at `x(t, s)`. -/
theorem fold_step :
    (coneW M tv ta).Reach (t2, s0) ∧
      (coneW M tv ta).step (t2, s0) (.inr lnote) (t2, s1) ∧
      (D true).ext.step (s0, (D true).inp t2) (.inl lnote.1) (s1, (D true).inp t2) ∧
      ((D true).inp t2).2 () = true := by
  have hst : (coneW M tv ta).step (t2, s0) (.inr lnote) (t2, s1) := by
    refine ⟨by simp [M, lnote], fun g hg => ?_, ?_⟩
    · simp [M, params, og, roleConjs, lnote] at hg
      rcases hg with rfl | rfl | rfl <;>
        simp [roleConj, Rows.cand, rows, ex, tfg, join, tv, t2]
    · funext v; cases v <;> simp [M, set1, join, tv, t2, s0, s1, lnote]
  exact ⟨Reach.step (Reach.step init0 step_open) step_setf, hst,
    Ext.s_fwd hyp hst, by simp [Ext.inp, D, bitOf, t2]⟩

/-- `mark`'s binding: the desk and the order at slot `0`. -/
def lmark : SaL ta := ⟨(.mark, bind0), rfl⟩

/-- A reachable `S_a` step of the unfolded `mark`, whose desk parameter is on an input and
whose order parameter is on `S`, and whose guard on `ds` is kept: the silent edge of `X` by
the same action and binding, and back from that edge to a step of the cone (`s_back`). -/
theorem mark_step :
    (coneW M tv ta).Reach (t1, s0) ∧
      (coneW M tv ta).step (t1, s0) (.inr lmark) (t1, s1) ∧
      (D true).ext.step (s0, (D true).inp t1) (.inl lmark.1) (s1, (D true).inp t1) ∧
      ∃ l, (D true).stands l lmark.1 ∧ (coneW M tv ta).step (t1, s0) (.inr l) (t1, s1) := by
  have hst : (coneW M tv ta).step (t1, s0) (.inr lmark) (t1, s1) := by
    refine ⟨by simp [M, lmark], fun g hg => ?_, ?_⟩
    · simp [M, params, og, roleConjs, lmark] at hg
      rcases hg with rfl | rfl | rfl <;>
        simp [roleConj, Rows.cand, rows, ex, kd, join, tv, t1, s0]
    · funext v; cases v <;> simp [M, set1, join, tv, t1, s0, s1, lmark]
  have hx := Ext.s_fwd hyp hst
  exact ⟨Reach.step init0 step_open, hst, hx, Ext.s_back hyp hx⟩

/-- With `bind_stub_params` off, a rise of `X = (D false).ext` never changes the desk, since
`open_`'s shop parameter keeps its `Parent` role and the shop is absent in `X`. -/
theorem no_rise_without_bind {x y : (D false).XS} (h : RiseStar (D false).ext x y)
    (hx : x.2.1 ⟨.ds, rfl⟩ = 0) : y.2.1 ⟨.ds, rfl⟩ = 0 := by
  induction h with
  | refl => exact hx
  | @cons x y z r hs _ ih =>
    apply ih
    rcases r with ⟨a, b⟩ | ⟨k, v⟩
    · obtain ⟨⟨hta, hw⟩, hg, _⟩ := hs
      cases a
      · simp [D, ta] at hta
      · exfalso
        have hxr : (D false).xrole .open_ 0 ⟨.shop, .parent⟩ = some .parent := by
          simp [Ext.xrole, Ext.side, Ext.isKey, Ext.bindStub, D, ex, tv, inputs, stub, fold,
            Role.needsPresent]
        have := hg.1 0 _ .parent rfl hxr
        simp [Rows.cand, D, rows, Ext.xval, ex, tv, inputs] at this
      · obtain ⟨v, hv, hi⟩ := hw
        simp [M] at hv; subst hv; simp [D, inputs] at hi
      · simp [D, ta] at hta
    · obtain ⟨_, rfl⟩ := hs
      exact hx

/-- **The stub-parameter bug.** With `bind_stub_params` off, the `C ⇒ X` half of Lemma 6
fails: the reachable step of `open_` changes the desk, and no sequence of rises of `X` does. -/
theorem stub_bug : ¬ Lemma6Fwd (coneW M tv ta) (D false).ext (D false).inp := by
  intro h
  have := no_rise_without_bind (h.rise init0 step_open) (by simp [Ext.inp, D, t0])
  simp [Ext.inp, D, t1] at this

end RdraIsh.ExampleLemma6
