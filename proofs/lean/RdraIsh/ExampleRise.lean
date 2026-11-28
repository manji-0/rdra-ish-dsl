import RdraIsh.Stages
import RdraIsh.Nested
import RdraIsh.Plain

/-!
# An instance of Theorem A8 with a merged block and a rise

A guard against vacuity that exercises what `Example.lean` does not: a block of two states with
a silent edge inside it and a silent exit, a `T_a` step that changes the input bytes and so is
matched by a rise, and an `S_a` action that reads the input.

- `T = Bool`, a flag; its input byte is the flag itself (`inp = id`).
- `S = St`, three values `a`, `b`, `c`; `obs s` says whether `s = c`.
- `T_a`: `t := true`, from `t = false`; it reads nothing of `S`.
- `S_a` `false`: `a → b`, always. `S_a` `true`: `b → c`, only once the flag is set: it reads
  the input.
- `X` on `St × Bool` has the same silent edges and a rise `(s, false) → (s, true)`.
- The partition is `B x = (obs x.1, x.2)`. At either input, `a` and `b` share a block: the edge
  `a → b` is a silent edge inside it, and at input `true` both leave it for the block of `c`,
  `a` through `b`. The rise moves each block at input `false` to its block at input `true`.

Every hypothesis of `a8_branching`, `a8_staged`, `a8_nested` (at depth zero) and
`a8_plain_iff` is discharged, and `merged_block`, `rise_changes_input` and `exit_through_block`
show that the instance exercises a block of two reachable states, a rise, and a silent exit
whose path crosses its own block.
-/

namespace RdraIsh.ExampleRise

open LTS

inductive St | a | b | c
  deriving DecidableEq

def C : LTS (Bool × St) (Unit ⊕ Bool) where
  init p := p = (false, .a)
  step p l q := match l with
    | .inl () => p.1 = false ∧ q = (true, p.2)
    | .inr false => p.2 = .a ∧ q = (p.1, .b)
    | .inr true => p.2 = .b ∧ p.1 = true ∧ q = (p.1, .c)

def X : LTS (St × Bool) (Bool ⊕ Unit) where
  init x := x = (.a, false)
  step x l y := match l with
    | .inl false => x.1 = .a ∧ y = (.b, x.2)
    | .inl true => x.1 = .b ∧ x.2 = true ∧ y = (.c, x.2)
    | .inr () => x.2 = false ∧ y = (x.1, true)

def obs : St → Bool
  | .c => true
  | _ => false

def inp : Bool → Bool := id

def B (x : St × Bool) : Bool × Bool := (obs x.1, x.2)

def rep (k : Bool × Bool) : St × Bool := (if k.1 then .c else .a, k.2)

theorem split : Split C obs where
  s_keeps_t {t s a t' s'} h := by
    cases a <;> simp only [C, Prod.mk.injEq] at h <;> simp only [h]
  t_keeps_s {t s a t' s'} h := by
    simp only [C, Prod.mk.injEq] at h
    exact h.2.2
  t_reads_obs {t s₁ s₂ a t'} _ h := by
    simp only [C, Prod.mk.injEq] at h ⊢
    exact ⟨h.1, h.2.1, trivial⟩

theorem lemma6 : Lemma6By C X inp (fun a b => a = b) where
  init h := by
    simp_all [C, X, inp]
  s_by {t s a s'} _ h := by
    refine ⟨a, rfl, ?_⟩
    cases a <;> simp_all [C, X, inp]
  rise {t s a t'} _ h := by
    simp only [C, Prod.mk.injEq] at h
    obtain ⟨rfl, rfl, -⟩ := h
    exact .cons () ⟨rfl, rfl⟩ (.refl _)
  s_back_by {t s b s'} _ h := by
    refine ⟨b, rfl, ?_⟩
    cases b <;> simp_all [C, X, inp]

theorem extended : Extended X where
  silent_keeps_inp {s i b s' i'} h := by
    cases b <;> simp only [X, Prod.mk.injEq] at h <;> simp only [h]
  rise_keeps_s {s i r s' i'} h := by
    simp only [X, Prod.mk.injEq] at h
    exact h.2.1

/-- The reachable states of `X`: `c` is reached only at input `true`. -/
theorem reach_c {x : St × Bool} (h : X.Reach x) : x.1 = .c → x.2 = true := by
  induction h with
  | init hi => simp only [X] at hi; subst hi; intro h; cases h
  | @step x l y _ hs ih =>
    obtain ⟨s, i⟩ := x
    match l with
    | .inl false =>
      simp only [X] at hs; rw [hs.2]; intro h; cases h
    | .inl true =>
      simp only [X] at hs; rw [hs.2.2]; intro; exact hs.2.1
    | .inr () =>
      simp only [X] at hs; rw [hs.2]; intro; rfl

theorem reach_at : X.Reach (.a, true) :=
  Reach.step (Reach.init rfl) (show X.step (.a, false) (.inr ()) (.a, true) from ⟨rfl, rfl⟩)
theorem reach_bt : X.Reach (.b, true) :=
  Reach.step reach_at (show X.step (.a, true) (.inl false) (.b, true) from ⟨rfl, rfl⟩)
theorem reach_ct : X.Reach (.c, true) :=
  Reach.step reach_bt (show X.step (.b, true) (.inl true) (.c, true) from ⟨rfl, rfl, rfl⟩)

theorem obs_rep (k : Bool × Bool) : B (rep k) = k := by
  obtain ⟨k1, k2⟩ := k
  cases k1 <;> rfl

/-- A silent edge of `X` that changes the block goes from `b` to `c` at input `true`. -/
theorem exit_edge {z z' : St × Bool} (hn : (silent X).Next z z') (hne : B z' ≠ B z) :
    z.1 = .b ∧ z.2 = true ∧ z' = (.c, true) := by
  obtain ⟨l, hl⟩ := hn
  cases l
  · simp only [silent, X] at hl
    obtain ⟨h1, rfl⟩ := hl
    exact absurd (by simp only [B, h1, obs]) hne
  · simp only [silent, X] at hl
    obtain ⟨h1, h2, rfl⟩ := hl
    exact ⟨h1, h2, by rw [h2]⟩

/-- Every state of the block `(false, true)` leaves it for the block of `c`. -/
theorem exit_of {y : St × Bool} (hy : B y = (false, true)) : SilentExit X B y (true, true) := by
  obtain ⟨s, i⟩ := y
  simp only [B, Prod.mk.injEq] at hy
  obtain ⟨hs, rfl⟩ := hy
  refine ⟨by simp only [B, hs, ne_eq, Prod.mk.injEq, Bool.true_eq_false, false_and,
    not_false_eq_true], ?_⟩
  cases s with
  | a =>
    exact ⟨(.b, true), (.c, true), .cons rfl ⟨false, rfl, rfl⟩ (.refl rfl),
      ⟨true, rfl, rfl, rfl⟩, rfl⟩
  | b => exact ⟨(.b, true), (.c, true), .refl rfl, ⟨true, rfl, rfl, rfl⟩, rfl⟩
  | c => cases hs

theorem stable : Stable X obs B rep where
  b1 _ _ h := by
    simp only [B, Prod.mk.injEq] at h
    exact h
  b2 {x y c'} _ _ hb hexit := by
    obtain ⟨hne, z, z', hp, hn, hz'⟩ := hexit
    have hz : B z = B x := hp.last
    obtain ⟨hz1, hz2, rfl⟩ := exit_edge hn (by rw [hz', hz]; exact hne)
    have hbx : B x = (false, true) := by rw [← hz]; simp only [B, hz1, hz2, obs]
    have hc' : c' = (true, true) := hz'.symm
    subst hc'
    exact exit_of (hb ▸ hbx)
  b3 {x y r x'} _ _ hb hs := by
    simp only [X] at hs
    obtain ⟨hx2, rfl⟩ := hs
    have hy : B y = B x := hb.symm
    simp only [B, Prod.mk.injEq] at hy
    refine ⟨(), (y.1, true), ⟨by rw [hy.2, hx2], rfl⟩, ?_⟩
    simp only [B, hy.1]
  rep_block _ := obs_rep _
  rep_reach {x} hx := by
    obtain ⟨s, i⟩ := x
    cases s with
    | c =>
      have hi := reach_c hx rfl
      simp only at hi
      subst hi
      exact reach_ct
    | a | b =>
      cases i
      · exact Reach.init rfl
      · exact reach_at

/-! ## The instance is not degenerate -/

/-- A block of two reachable states, joined by a silent edge. -/
theorem merged_block : X.Reach (.a, true) ∧ X.Reach (.b, true) ∧ (St.a, true) ≠ (St.b, true) ∧
    B (.a, true) = B (.b, true) ∧ (silent X).Next (.a, true) (.b, true) :=
  ⟨reach_at, reach_bt, by simp, rfl, ⟨false, rfl, rfl⟩⟩

/-- A reachable `T_a` step that changes the input bytes, matched by a rise of `X`. -/
theorem rise_changes_input : C.Reach (false, .a) ∧ C.step (false, .a) (.inl ()) (true, .a) ∧
    inp false ≠ inp true ∧ X.step (.a, inp false) (.inr ()) (.a, inp true) :=
  ⟨Reach.init rfl, ⟨rfl, rfl⟩, by decide, ⟨rfl, rfl⟩⟩

/-- The silent exit from `a` crosses its own block through `b`: (B2) in its path form. -/
theorem exit_through_block : SilentExit X B (.a, true) (true, true) ∧
    ∀ b, ¬ X.step (.a, true) (.inl b) (.c, true) := by
  refine ⟨exit_of rfl, fun b h => ?_⟩
  cases b <;> simp [X] at h

/-! ## The theorems apply -/

theorem bisim :
    BranchBisim C (reduced C X inp B rep) tLabel tLabel (rel C inp B) ∧
      PreservesObs (rel C inp B) (obsC obs) (obsR obs rep) :=
  a8_branching split lemma6.toLemma6 extended stable

/-- Consequence 1: "the flag is set whenever `c` is" holds on `C` iff on `R`. -/
theorem c_after_flag :
    C.Holds (fun p => (obsC obs p).2 = true → (obsC obs p).1 = true) ↔
      (reduced C X inp B rep).Holds (fun q => (obsR obs rep q).2 = true →
        (obsR obs rep q).1 = true) :=
  a8_holds_iff split lemma6.toLemma6 extended stable (fun o => o.2 = true → o.1 = true)

/-- Consequence 2: the `T_a` edges by their values before and after. -/
theorem edges (o o' : Bool × Bool) :
    Edges C tLabel (obsC obs) () o o' ↔ Edges (reduced C X inp B rep) tLabel (obsR obs rep) () o o' :=
  a8_edges_iff split lemma6.toLemma6 extended stable () o o'

/-- Consequence 3: a state stuck at `obs` (no path reaches `c`) exists on `C` iff on `R`. -/
theorem stuck (o : Bool × Bool) :
    Stuck C (obsC obs) (fun p q => p.2 ≠ q.2) o ↔
      Stuck (reduced C X inp B rep) (obsR obs rep) (fun p q => p.2 ≠ q.2) o :=
  a8_stuck_iff split lemma6.toLemma6 extended stable (fun _ h => h rfl) o

/-- Consequence 4: "once the flag is set, `c` follows", with `T_a` fair, under `Plain`. -/
theorem flag_leads_to_c :
    C.LiveHolds (fun σ => C.FairP (onT fun (_ : Unit) _ => True) σ)
        (fun p => (obsC obs p).1 = true) (fun p => (obsC obs p).2 = true) ↔
      (reduced C X inp B rep).LiveHolds
        (fun ρ => (reduced C X inp B rep).FairP (onT fun (_ : Unit) _ => True) ρ)
        (fun q => (obsR obs rep q).1 = true) (fun q => (obsR obs rep q).2 = true) :=
  a8_plain_iff split lemma6.toLemma6 extended stable _ (fun o => o.1 = true) (fun o => o.2 = true)

/-- `Reduction::stages` here: one stage, the new input byte, when it changes. -/
def stages (i i' : Bool) : List Bool := if i = i' then [] else [i']

/-- The rise table: a block at input `false` rises to the same `obs` at input `true`. -/
def rt (k : Bool × Bool) (i : Bool) : Option (Bool × Bool) :=
  if k.2 = false ∧ i = true then some (k.1, true) else none

theorem lemma6_staged : Lemma6Staged C X inp stages where
  init := lemma6.init
  s_step hr hs := lemma6.toLemma6.s_step hr hs
  rise_staged {t s a t'} _ h := by
    simp only [C, Prod.mk.injEq] at h
    obtain ⟨rfl, rfl, -⟩ := h
    exact .cons () ⟨rfl, rfl⟩ rfl (.nil _)
  s_step_back hr hs := lemma6.toLemma6.s_step_back hr hs

theorem rise_table : RiseTable X B rep rt where
  complete {k r y} _ h := by
    simp only [X] at h
    obtain ⟨h2, rfl⟩ := h
    obtain ⟨k1, k2⟩ := k
    simp only [rep] at h2 ⊢
    subst h2
    cases k1 <;> rfl

theorem bisim_staged :
    BranchBisim C (reducedStaged C X inp B rep rt stages) tLabel tLabel (rel C inp B) ∧
      PreservesObs (rel C inp B) (obsC obs) (obsR obs rep) :=
  a8_staged split lemma6_staged extended stable rise_table

/-- The flat case as a tower of depth zero. -/
theorem nested_flat : ∃ (Blk : Type) (B' : St × Bool → Blk) (rep' : Blk → St × Bool),
    BranchBisim C (reduced C X inp B' rep') tLabel tLabel (rel C inp B') ∧
      PreservesObs (rel C inp B') (obsC obs) (obsR obs rep') :=
  a8_nested split lemma6.toLemma6 extended (.flat B (stableV_of_stable stable)) (.a, false)

end RdraIsh.ExampleRise
