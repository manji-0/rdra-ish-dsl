import RdraIsh.Split

/-!
# A concrete instance of Lemma 5's hypotheses, with an input and an observed variable

Three Boolean variables and two actions:

- `fire` (kept) reads `t` and `s`, needs both false, and sets `t` and `m`;
- `flip` reads `m` and `s`, needs `m`, and toggles `s`.

`m` only rises, so it is monotone; `s` toggles, so it is not. With the anchor `t`,
Definition 3 gives `T_v = {t, m}`, `T_a = {fire}`, `S_v = {s}` and `S_a = {flip}`: `fire` is
kept and pulls its write `m` into `T_v`, and `flip` reads `T_v` only at the monotone `m`.
`obs = {s}`, since `fire` reads it, and `inputs = {m}`, which `flip` reads raw.

`sets` evaluates `computed` (Definition 3 by iteration). `local_` and `foldData` discharge
the hypotheses of `split_computed`, so `split` is `Reduction.lean`'s `Split` of this cone
system, derived and not assumed.
-/

namespace RdraIsh.ExampleSplit

open RdraIsh.Act

inductive Var | t | m | s
  deriving DecidableEq

inductive Act | fire | flip
  deriving DecidableEq

def fireEff (σ : Var → Bool) : Var → Bool
  | .t => true
  | .m => true
  | .s => σ .s

def flipEff (σ : Var → Bool) : Var → Bool
  | .s => !σ .s
  | v => σ v

def M : Model Var Unit Act Bool where
  vars := [.t, .m, .s]
  acts := [.fire, .flip]
  vars_all v := by cases v <;> simp
  acts_all a := by cases a <;> simp
  own _ := none
  existences _ := []
  counted _ := false
  params _ := []
  guards
    | .fire => [⟨[.t, .s], fun _ σ => σ .t = false ∧ σ .s = false⟩]
    | .flip => [⟨[.m], fun _ σ => σ .m = true⟩]
  rd
    | .fire => [.t, .s]
    | .flip => [.m, .s]
  wr
    | .fire => [.t, .m]
    | .flip => [.s]
  eff
    | .fire => fun _ => fireEff
    | .flip => fun _ => flipEff
  init σ := ∀ v, σ v = false

def mono : Var → Bool
  | .s => false
  | _ => true

def keep : Act → Bool
  | .fire => true
  | .flip => false

def seeds : List (List Var) := [[.t]]

/-- `raw_reads`: no fold, so every read. -/
def raw : Act → List Var := M.rd

def folded : Act → Var → Prop := fun _ _ => False

abbrev c := computed M mono keep (some .t)

/-- Definition 3, computed: `T_v = {t, m}`, `T_a = {fire}`. -/
theorem sets : (c.1 .t, c.1 .m, c.1 .s, c.2 .fire, c.2 .flip) = (true, true, false, true, false) := by
  decide

theorem local_ : Local M where
  conj_rd := by
    intro a g hg v hv
    cases a <;> simp [M] at hg <;> subst hg <;> simp at hv <;> rcases hv with rfl | rfl <;>
      simp [M]
  conj_local := by
    intro a g hg b σ τ hag h
    cases a <;> simp [M] at hg <;> subst hg
    · simp only [Agree, List.mem_cons, List.mem_nil_iff, or_false, forall_eq_or_imp,
        forall_eq] at hag
      exact ⟨hag.1 ▸ h.1, hag.2 ▸ h.2⟩
    · simp only [Agree, List.mem_cons, List.mem_nil_iff, or_false, forall_eq] at hag
      show τ .m = true
      rw [← hag]; exact h
  eff_frame := by
    intro a b σ v hv
    cases a <;> cases v <;> simp [M] at hv ⊢ <;> rfl
  eff_local := by
    intro a b σ τ hag v hv
    cases a <;> cases v <;> simp [M] at hv ⊢ <;> simp [fireEff, flipEff]
    exact hag .s (by simp [M])

theorem foldData : FoldData M mono c.1 c.2 raw folded where
  raw_rd _ _ h := h
  cover _ _ _ hv _ := Or.inl hv
  ex_mono _ _ _ h := absurd h List.not_mem_nil

/-- `Split` of this cone system, from `split_computed`. -/
theorem split :
    RdraIsh.Split (cone M c.1 c.2)
      (obsMap c.1 (lemma5ab_of_closed (seeds := seeds) computed_closed).obs_s) :=
  split_computed local_

/-- Lemma 5 (a)–(e) of the computed split, with `FoldData`. -/
theorem lemma5 :
    Lemma5 M mono keep seeds c.1 c.2 (obsOf M seeds c.1 c.2) (inputsOf M c.1 c.2 raw) folded :=
  lemma5_computed foldData

/-- `m` is an input and `s` is observed, so neither set is empty. -/
theorem input_and_obs : inputsOf M c.1 c.2 raw .m ∧ obsOf M seeds c.1 c.2 .s := by
  have hm : c.1 .m = true := by decide
  have hs : c.1 .s = false := by decide
  have hfl : c.2 .flip = false := by decide
  have hfi : c.2 .fire = true := by decide
  refine ⟨⟨hm, Or.inl ⟨hm, .flip, ⟨hfl, by simp [M]⟩, by simp [raw, M]⟩⟩,
    ⟨hs, Or.inl ⟨.fire, hfi, by simp [M]⟩⟩⟩

end RdraIsh.ExampleSplit
