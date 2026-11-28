import RdraIsh.Actions
import RdraIsh.Reduction

/-!
# Definition 3 and Lemma 5 (cones.md, "The reduction by an independent sub-lifecycle")

Over the action-level model of `Actions.lean`:

1. **Definition 3** for one anchor, as `reduce/split.rs::split_at` computes it: `T_v` and
   `T_a` are the least sets such that
   - the anchor and every counter are in `T_v`;
   - an action that writes something or is kept (`split_at` skips any other) and that is
     kept, writes a `T_v` variable, or reads a `T_v` variable that is not monotone, is in
     `T_a`;
   - the writes of a `T_a` action are in `T_v`;
   - a variable and its entity's existence (`own_existence`) are on the same side.

   `Mem` is that least set, as an inductive predicate. `Closed` is the closure condition on
   any pair of sets. `iter` is one simultaneous pass of the conditions, repeated; `computed`
   stops after `|V| + |A| + 1` passes.
2. `S_v`, `S_a`, `obs` and `inputs` as `split_with` defines them, with the folds abstract:
   `folded a` is the set of `T`-reads of the `S_a` action `a` that its fold bits cover, and
   `raw a` is the code's `raw_reads` (what `a` reads outside its fold).
3. **Lemma 5** as a predicate `Lemma5` on arbitrary sets, and three theorems:
   - `lemma5_of_closed`: every closed pair, with `obs` and `inputs` defined from it,
     satisfies Lemma 5 (a)–(e), given `FoldData`;
   - `mem_eq_of_sound`, `computed_iff`: the least fixpoint is computed by iteration, and any
     pair whose members all follow from the rules and that is closed is the least fixpoint,
     whatever order the rules were applied in (the code's in-place loop is one such order);
   - `split_of_lemma5`: Lemma 5 (a)(b) and `Local` give `Reduction.lean`'s `Split` for the
     cone system over `T_v × S_v` valuations. So the hypothesis `Split` of Theorem A8 holds
     of any sets that pass Lemma 5, computed or certified.

The monotone predicate `mono` is a parameter: `monotone_vars` with inputs on, constant
`false` without. Lemma 5 (d) needs one fact of it, `FoldData.ex_mono`.
-/

namespace RdraIsh.Act

open LTS

variable {V E A Val : Type} [DecidableEq V]

section Def3

variable (M : Model V E A Val) (mono : V → Bool) (keep : A → Bool) (anchor : Option V)

/-- The seed of `T_v`: the anchor and every counter. -/
def isInit (v : V) : Bool := decide (anchor = some v) || M.counted v

/-- `split_at` skips an action that writes nothing and is not kept. -/
def joinable (a : A) : Bool := keep a || !(M.wr a).isEmpty

/-- Why an action joins `T_a`: it is kept, writes `T_v`, or reads a non-monotone `T_v`
variable. -/
def trigger (tv : V → Bool) (a : A) : Bool :=
  keep a || (M.wr a).any tv || (M.rd a).any (fun v => tv v && !mono v)

/-- Definition 3's least sets, `.inl` for `T_v` and `.inr` for `T_a`. -/
inductive Mem : V ⊕ A → Prop
  | init {v} : isInit M anchor v = true → Mem (.inl v)
  | keep {a} : keep a = true → Mem (.inr a)
  | wrT {a v} : v ∈ M.wr a → Mem (.inl v) → Mem (.inr a)
  | rdT {a v} : joinable M keep a = true → v ∈ M.rd a → mono v = false → Mem (.inl v) →
      Mem (.inr a)
  | wr {a v} : Mem (.inr a) → v ∈ M.wr a → Mem (.inl v)
  | up {v e} : M.own v = some e → Mem (.inl v) → Mem (.inl e)
  | down {v e} : M.own v = some e → Mem (.inl e) → Mem (.inl v)

/-- The closure conditions of Definition 3 on a pair of sets. -/
structure Closed (tv : V → Bool) (ta : A → Bool) : Prop where
  init : ∀ v, isInit M anchor v = true → tv v = true
  join : ∀ a, joinable M keep a = true → trigger M mono keep tv a = true → ta a = true
  wr : ∀ a, ta a = true → ∀ v ∈ M.wr a, tv v = true
  up : ∀ v e, M.own v = some e → tv v = true → tv e = true
  down : ∀ v e, M.own v = some e → tv e = true → tv v = true

/-- The least fixpoint as a pair of predicates. -/
abbrev MemT (v : V) : Prop := Mem M mono keep anchor (.inl v)
abbrev MemA (a : A) : Prop := Mem M mono keep anchor (.inr a)

/-- The pass of `split_at`'s loop, applied to all rules at once. -/
def passA (p : (V → Bool) × (A → Bool)) (a : A) : Bool :=
  p.2 a || (joinable M keep a && trigger M mono keep p.1 a)

def passW (p : (V → Bool) × (A → Bool)) (v : V) : Bool :=
  p.1 v || isInit M anchor v || M.acts.any (fun a => passA M mono keep p a && decide (v ∈ M.wr a))

def pass (p : (V → Bool) × (A → Bool)) : (V → Bool) × (A → Bool) :=
  (fun v => passW M mono keep anchor p v ||
      (match M.own v with
        | some e => passW M mono keep anchor p e
        | none => false) ||
      M.vars.any (fun u => decide (M.own u = some v) && passW M mono keep anchor p u),
    passA M mono keep p)

def iter : Nat → (V → Bool) × (A → Bool)
  | 0 => (isInit M anchor, fun _ => false)
  | n + 1 => pass M mono keep anchor (iter n)

/-- The sets `split_at` returns: the iteration after `|V| + |A| + 1` passes. -/
def computed : (V → Bool) × (A → Bool) := iter M mono keep anchor (M.vars.length + M.acts.length + 1)

/-- Every member of a pair follows from the rules. -/
def Sound (p : (V → Bool) × (A → Bool)) : Prop :=
  (∀ v, p.1 v = true → MemT M mono keep anchor v) ∧ (∀ a, p.2 a = true → MemA M mono keep anchor a)

end Def3

section Fixpoint

variable {M : Model V E A Val} {mono : V → Bool} {keep : A → Bool} {anchor : Option V}

omit [DecidableEq V] in
theorem joinable_of_wr {a : A} {v : V} (h : v ∈ M.wr a) : joinable M keep a = true := by
  unfold joinable
  cases hw : M.wr a with
  | nil => rw [hw] at h; exact absurd h List.not_mem_nil
  | cons _ _ => simp

/-- `Mem` is closed. -/
theorem mem_closed : Closed M mono keep anchor
    (fun v => @decide (MemT M mono keep anchor v) (Classical.propDecidable _))
    (fun a => @decide (MemA M mono keep anchor a) (Classical.propDecidable _)) where
  init v h := by simpa using Mem.init h
  join a hj ht := by
    simp only [trigger, Bool.or_eq_true, List.any_eq_true, Bool.and_eq_true,
      Bool.not_eq_true'] at ht
    simp only [decide_eq_true_eq]
    rcases ht with (hk | ⟨v, hv, hm⟩) | ⟨v, hv, hm, hn⟩
    · exact Mem.keep hk
    · exact Mem.wrT hv (by simpa using hm)
    · exact Mem.rdT hj hv hn (by simpa using hm)
  wr a ha v hv := by simpa using Mem.wr (by simpa using ha) hv
  up v e ho h := by simpa using Mem.up ho (by simpa using h)
  down v e ho h := by simpa using Mem.down ho (by simpa using h)

/-- `Mem` is least: it lies inside every closed pair. -/
theorem mem_least {tv : V → Bool} {ta : A → Bool} (hc : Closed M mono keep anchor tv ta) :
    ∀ {x}, Mem M mono keep anchor x →
      (match x with | .inl v => tv v = true | .inr a => ta a = true) := by
  intro x hx
  induction hx with
  | init h => exact hc.init _ h
  | keep h => exact hc.join _ (by simp [joinable, h]) (by simp [trigger, h])
  | @wrT a v hv _ ih =>
    exact hc.join _ (joinable_of_wr hv) (by
      simp only [trigger, Bool.or_eq_true, List.any_eq_true]
      exact Or.inl (Or.inr ⟨v, hv, ih⟩))
  | @rdT a v hj hv hn _ ih =>
    exact hc.join _ hj (by
      simp only [trigger, Bool.or_eq_true, List.any_eq_true, Bool.and_eq_true]
      exact Or.inr ⟨v, hv, ih, by simp [hn]⟩)
  | wr _ hv ih => exact hc.wr _ ih _ hv
  | up ho _ ih => exact hc.up _ _ ho ih
  | down ho _ ih => exact hc.down _ _ ho ih

/-- A pair whose members follow from the rules and that is closed is the least fixpoint. This
holds whatever order the rules were applied in to build it. -/
theorem mem_eq_of_sound {p : (V → Bool) × (A → Bool)} (hs : Sound M mono keep anchor p)
    (hc : Closed M mono keep anchor p.1 p.2) :
    (∀ v, p.1 v = true ↔ MemT M mono keep anchor v) ∧
      (∀ a, p.2 a = true ↔ MemA M mono keep anchor a) :=
  ⟨fun v => ⟨hs.1 v, fun h => mem_least hc h⟩, fun a => ⟨hs.2 a, fun h => mem_least hc h⟩⟩

theorem passA_sound {p : (V → Bool) × (A → Bool)} (hs : Sound M mono keep anchor p) :
    ∀ a, passA M mono keep p a = true → MemA M mono keep anchor a := by
  intro a h
  simp only [passA, Bool.or_eq_true, Bool.and_eq_true] at h
  rcases h with h | ⟨hj, ht⟩
  · exact hs.2 a h
  · simp only [trigger, Bool.or_eq_true, List.any_eq_true, Bool.and_eq_true,
      Bool.not_eq_true'] at ht
    rcases ht with (hk | ⟨v, hv, hm⟩) | ⟨v, hv, hm, hn⟩
    · exact Mem.keep hk
    · exact Mem.wrT hv (hs.1 v hm)
    · exact Mem.rdT hj hv hn (hs.1 v hm)

theorem passW_sound {p : (V → Bool) × (A → Bool)} (hs : Sound M mono keep anchor p) :
    ∀ v, passW M mono keep anchor p v = true → MemT M mono keep anchor v := by
  intro v h
  simp only [passW, Bool.or_eq_true, List.any_eq_true, Bool.and_eq_true,
    decide_eq_true_eq] at h
  rcases h with (h | h) | ⟨a, _, ha, hv⟩
  · exact hs.1 v h
  · exact Mem.init h
  · exact Mem.wr (passA_sound hs a ha) hv

/-- One pass keeps a pair sound. -/
theorem pass_sound {p : (V → Bool) × (A → Bool)} (hs : Sound M mono keep anchor p) :
    Sound M mono keep anchor (pass M mono keep anchor p) := by
  refine ⟨fun v h => ?_, passA_sound hs⟩
  simp only [pass, Bool.or_eq_true, List.any_eq_true, Bool.and_eq_true,
    decide_eq_true_eq] at h
  rcases h with (h | h) | ⟨u, _, hu, h⟩
  · exact passW_sound hs v h
  · cases ho : M.own v with
    | none => rw [ho] at h; exact absurd h (by simp)
    | some e =>
      rw [ho] at h
      exact Mem.down ho (passW_sound hs e h)
  · exact Mem.up hu (passW_sound hs u h)

theorem iter_sound : ∀ n, Sound M mono keep anchor (iter M mono keep anchor n)
  | 0 => ⟨fun _ h => Mem.init h, fun _ h => by simp [iter] at h⟩
  | n + 1 => pass_sound (iter_sound n)

/-- A pass only adds. -/
theorem pass_grows (p : (V → Bool) × (A → Bool)) :
    (∀ v, p.1 v = true → (pass M mono keep anchor p).1 v = true) ∧
      (∀ a, p.2 a = true → (pass M mono keep anchor p).2 a = true) :=
  ⟨fun v h => by simp [pass, passW, h], fun a h => by simp [pass, passA, h]⟩

/-- A pair a pass leaves unchanged is closed. This is the exit of `split_at`'s loop. -/
theorem closed_of_fixed {p : (V → Bool) × (A → Bool)} (hf : pass M mono keep anchor p = p) :
    Closed M mono keep anchor p.1 p.2 := by
  have hA : ∀ a, passA M mono keep p a = p.2 a := fun a => by
    have := congrArg (fun q => q.2 a) hf; simpa [pass] using this
  have hT : ∀ v, (pass M mono keep anchor p).1 v = p.1 v := fun v => by rw [hf]
  have hW : ∀ v, passW M mono keep anchor p v = true → p.1 v = true := fun v h => by
    rw [← hT v]; simp [pass, h]
  refine ⟨fun v h => hW v (by simp [passW, h]), fun a hj ht => ?_, fun a ha v hv => ?_,
    fun v e ho h => ?_, fun v e ho h => ?_⟩
  · rw [← hA a]; simp [passA, hj, ht]
  · apply hW v
    simp only [passW, Bool.or_eq_true, List.any_eq_true, Bool.and_eq_true, decide_eq_true_eq]
    exact Or.inr ⟨a, M.acts_all a, by rw [hA a]; exact ha, hv⟩
  · rw [← hT e]
    simp only [pass, Bool.or_eq_true, List.any_eq_true, Bool.and_eq_true, decide_eq_true_eq]
    exact Or.inr ⟨v, M.vars_all v, ho, by simp [passW, h]⟩
  · rw [← hT v]
    simp only [pass, Bool.or_eq_true, ho]
    exact Or.inl (Or.inr (by simp [passW, h]))

/-! ### Termination: a pass that changes the pair adds a listed member -/

theorem countP_mono {α : Type} {f g : α → Bool} (hfg : ∀ x, f x = true → g x = true) :
    ∀ l : List α, l.countP f ≤ l.countP g
  | [] => Nat.le_refl _
  | x :: l => by
    simp only [List.countP_cons]
    have := countP_mono hfg l
    by_cases hf : f x = true
    · simp [hf, hfg x hf]; omega
    · by_cases hg : g x = true <;> simp [hf, hg] <;> omega

theorem countP_lt {α : Type} {f g : α → Bool} (hfg : ∀ x, f x = true → g x = true)
    {y : α} (hfy : f y = false) (hgy : g y = true) :
    ∀ l : List α, y ∈ l → l.countP f < l.countP g
  | [], h => absurd h List.not_mem_nil
  | x :: l, h => by
    simp only [List.countP_cons]
    rcases List.mem_cons.mp h with rfl | h
    · have := countP_mono hfg l
      simp [hfy, hgy]; omega
    · have := countP_lt hfg hfy hgy l h
      by_cases hf : f x = true
      · simp [hf, hfg x hf]; omega
      · by_cases hg : g x = true <;> simp [hf, hg] <;> omega

/-- The number of listed members of a pair. -/
def size (M : Model V E A Val) (p : (V → Bool) × (A → Bool)) : Nat :=
  M.vars.countP p.1 + M.acts.countP p.2

omit [DecidableEq V] in
theorem size_le (p : (V → Bool) × (A → Bool)) : size M p ≤ M.vars.length + M.acts.length := by
  unfold size
  have := @List.countP_le_length _ p.1 M.vars
  have := @List.countP_le_length _ p.2 M.acts
  omega

theorem size_lt {p : (V → Bool) × (A → Bool)} (hne : pass M mono keep anchor p ≠ p) :
    size M p < size M (pass M mono keep anchor p) := by
  have hg := pass_grows (M := M) (mono := mono) (keep := keep) (anchor := anchor) p
  have hm1 := countP_mono hg.1 M.vars
  have hm2 := countP_mono hg.2 M.acts
  unfold size
  by_cases h1 : ∀ v, (pass M mono keep anchor p).1 v = p.1 v
  · by_cases h2 : ∀ a, (pass M mono keep anchor p).2 a = p.2 a
    · exact absurd (Prod.ext (funext h1) (funext h2)) hne
    · obtain ⟨a, ha⟩ := Classical.not_forall.mp h2
      have hpa : p.2 a = false := by
        cases hq : p.2 a
        · rfl
        · exact absurd (by rw [hq]; exact hg.2 a hq) ha
      have hqa : (pass M mono keep anchor p).2 a = true := by
        cases hq : (pass M mono keep anchor p).2 a
        · exact absurd (by rw [hq, hpa]) ha
        · rfl
      have := countP_lt hg.2 hpa hqa M.acts (M.acts_all a)
      omega
  · obtain ⟨v, hv⟩ := Classical.not_forall.mp h1
    have hpv : p.1 v = false := by
      cases hq : p.1 v
      · rfl
      · exact absurd (by rw [hq]; exact hg.1 v hq) hv
    have hqv : (pass M mono keep anchor p).1 v = true := by
      cases hq : (pass M mono keep anchor p).1 v
      · exact absurd (by rw [hq, hpv]) hv
      · rfl
    have := countP_lt hg.1 hpv hqv M.vars (M.vars_all v)
    omega

theorem iter_fixed_after {k : Nat}
    (hk : pass M mono keep anchor (iter M mono keep anchor k) = iter M mono keep anchor k) :
    ∀ j, iter M mono keep anchor (k + j) = iter M mono keep anchor k
  | 0 => rfl
  | j + 1 => by
    show pass M mono keep anchor (iter M mono keep anchor (k + j)) = _
    rw [iter_fixed_after hk j, hk]

theorem iter_progress : ∀ n, (∃ k, k ≤ n ∧
    pass M mono keep anchor (iter M mono keep anchor k) = iter M mono keep anchor k) ∨
    n ≤ size M (iter M mono keep anchor n)
  | 0 => Or.inr (Nat.zero_le _)
  | n + 1 => by
    rcases iter_progress n with ⟨k, hk, hf⟩ | hn
    · exact Or.inl ⟨k, Nat.le_succ_of_le hk, hf⟩
    · by_cases hf : pass M mono keep anchor (iter M mono keep anchor n) =
          iter M mono keep anchor n
      · exact Or.inl ⟨n, Nat.le_succ n, hf⟩
      · have := size_lt hf
        exact Or.inr (by show n + 1 ≤ size M (pass M mono keep anchor _); omega)

/-- The iteration reaches a fixpoint within `|V| + |A| + 1` passes. -/
theorem computed_fixed :
    pass M mono keep anchor (computed M mono keep anchor) = computed M mono keep anchor := by
  unfold computed
  rcases iter_progress (M := M) (mono := mono) (keep := keep) (anchor := anchor)
      (M.vars.length + M.acts.length + 1) with ⟨k, hk, hf⟩ | hn
  · obtain ⟨j, hj⟩ := Nat.exists_eq_add_of_le hk
    rw [hj, iter_fixed_after hf j]
    exact hf
  · have := size_le (M := M) (iter M mono keep anchor (M.vars.length + M.acts.length + 1))
    omega

/-- The computed sets are closed. -/
theorem computed_closed :
    Closed M mono keep anchor (computed M mono keep anchor).1 (computed M mono keep anchor).2 :=
  closed_of_fixed computed_fixed

/-- The computed sets are Definition 3's least sets. -/
theorem computed_iff :
    (∀ v, (computed M mono keep anchor).1 v = true ↔ MemT M mono keep anchor v) ∧
      (∀ a, (computed M mono keep anchor).2 a = true ↔ MemA M mono keep anchor a) :=
  mem_eq_of_sound (iter_sound _) computed_closed

end Fixpoint

/-! ## `S_v`, `S_a`, `obs`, `inputs`, and Lemma 5 -/

section Lemma5

variable (M : Model V E A Val) (mono : V → Bool) (keep : A → Bool) (seeds : List (List V))
  (tv : V → Bool) (ta : A → Bool)

/-- `S_a`: the actions outside `T_a` that write something. -/
def Sa (a : A) : Prop := ta a = false ∧ M.wr a ≠ []

/-- `obs = (⋃_{a ∈ T_a} Rd(a) ∪ ⋃ seeds) ∩ S_v`. -/
def obsOf (v : V) : Prop :=
  tv v = false ∧ ((∃ a, ta a = true ∧ v ∈ M.rd a) ∨ ∃ r ∈ seeds, v ∈ r)

/-- A `T_v` variable some `S_a` action reads raw. -/
def rawIn (raw : A → List V) (v : V) : Prop := tv v = true ∧ ∃ a, Sa M ta a ∧ v ∈ raw a

/-- `inputs`: the raw `T_v` reads of the `S_a` actions, with their existences, in `T_v`. -/
def inputsOf (raw : A → List V) (v : V) : Prop :=
  tv v = true ∧ (rawIn M tv ta raw v ∨ ∃ u, rawIn M tv ta raw u ∧ v ∈ M.existences u)

/-- What Lemma 5 needs of the code's folds and `raw_reads`, for the split `(tv, ta)`:

- `raw_rd`: an action's raw reads are among its reads (`raw_reads` ⊆ `action_reads` ∪
  `copy_reads`);
- `cover`: every `T_v` read of an `S_a` action is raw, an existence of a raw `T_v` read, or
  folded (Definition 3's `folds`: the reads one bit per slot of a key covers). The existence
  case is a `Triggered` parameter's row, which `raw_reads` leaves out and `existences` of
  its mark brings back;
- `ex_mono`: an existence of a monotone variable is monotone (`monotone_vars`: only creates
  and deletes write an existence, from absent to present to deleted). -/
structure FoldData (raw : A → List V) (folded : A → V → Prop) : Prop where
  raw_rd : ∀ a, ∀ v ∈ raw a, v ∈ M.rd a
  cover : ∀ a, Sa M ta a → ∀ v ∈ M.rd a, tv v = true →
    v ∈ raw a ∨ (∃ u ∈ raw a, tv u = true ∧ v ∈ M.existences u) ∨ folded a v
  ex_mono : ∀ u v, mono u = true → v ∈ M.existences u → mono v = true

/-- Lemma 5 (a) and (b), with `obs ⊆ S_v`: the part of Lemma 5 that `Split` needs. -/
structure Lemma5ab (obs : V → Prop) : Prop where
  /-- (a) Every `S_a` action writes only `S_v`. -/
  a : ∀ a, Sa M ta a → ∀ v ∈ M.wr a, tv v = false
  /-- (b) Every `T_a` action writes only `T_v` ... -/
  b_wr : ∀ a, ta a = true → ∀ v ∈ M.wr a, tv v = true
  /-- ... and reads `S_v` only in `obs`. -/
  b_rd : ∀ a, ta a = true → ∀ v ∈ M.rd a, tv v = false → obs v
  /-- `obs ⊆ S_v`. -/
  obs_s : ∀ v, obs v → tv v = false

/-- Lemma 5 (split), for any sets `T_v`, `T_a`, `obs`, `inputs` and folded reads, with `S_v`
the complement of `T_v` and `S_a` the writers outside `T_a`. A run-time certificate of the
sets `split_with` returns meets this statement when it checks every field. -/
structure Lemma5 (obs inputs : V → Prop) (folded : A → V → Prop) : Prop
    extends Lemma5ab M tv ta obs where
  /-- (c) Every read of an `S_a` action is in `S_v`, `inputs`, or a fold. -/
  c : ∀ a, Sa M ta a → ∀ v ∈ M.rd a, tv v = false ∨ inputs v ∨ folded a v
  /-- (d) Every input is monotone. -/
  d : ∀ v, inputs v → mono v = true
  /-- (e) Every seed's read of `S_v` is in `obs` ... -/
  e_seed : ∀ r ∈ seeds, ∀ v ∈ r, tv v = false → obs v
  /-- ... and every kept action is in `T_a`. -/
  e_keep : ∀ a, keep a = true → ta a = true
  /-- `inputs ⊆ T_v`. -/
  inputs_t : ∀ v, inputs v → tv v = true

variable {M mono keep seeds tv ta}

/-- An action outside `T_a` writes nothing in `T_v`. -/
theorem s_writes_s {anchor : Option V} (hc : Closed M mono keep anchor tv ta) {a : A}
    (ha : ta a = false) : ∀ v ∈ M.wr a, tv v = false := by
  intro v hv
  cases htv : tv v
  · rfl
  · have := hc.join a (joinable_of_wr hv) (by
      simp only [trigger, Bool.or_eq_true, List.any_eq_true]
      exact Or.inl (Or.inr ⟨v, hv, htv⟩))
    rw [ha] at this; exact absurd this (by simp)

/-- Lemma 5 (a)(b) of every closed pair, with `obs` as Definition 3 defines it. It needs
nothing of the folds. -/
theorem lemma5ab_of_closed {anchor : Option V} (hc : Closed M mono keep anchor tv ta) :
    Lemma5ab M tv ta (obsOf M seeds tv ta) where
  a _ ha := s_writes_s hc ha.1
  b_wr := hc.wr
  b_rd a ha _ hv hs := ⟨hs, Or.inl ⟨a, ha, hv⟩⟩
  obs_s _ h := h.1

/-- **Lemma 5** (a)–(e) of every closed pair, with `obs` and `inputs` as Definition 3
defines them. -/
theorem lemma5_of_closed {anchor : Option V} {raw : A → List V} {folded : A → V → Prop}
    (hc : Closed M mono keep anchor tv ta) (hf : FoldData M mono tv ta raw folded) :
    Lemma5 M mono keep seeds tv ta (obsOf M seeds tv ta) (inputsOf M tv ta raw) folded where
  toLemma5ab := lemma5ab_of_closed hc
  c a ha v hv := by
    cases htv : tv v
    · exact Or.inl rfl
    · rcases hf.cover a ha v hv htv with h | ⟨u, hu, hut, hex⟩ | h
      · exact Or.inr (Or.inl ⟨htv, Or.inl ⟨htv, a, ha, h⟩⟩)
      · exact Or.inr (Or.inl ⟨htv, Or.inr ⟨u, ⟨hut, a, ha, hu⟩, hex⟩⟩)
      · exact Or.inr (Or.inr h)
  d v hv := by
    -- A raw `T_v` read of an `S_a` action is monotone, or the action would be in `T_a`.
    have raw_mono : ∀ u, rawIn M tv ta raw u → mono u = true := by
      rintro u ⟨hut, a, ⟨ha, hw⟩, hu⟩
      cases hm : mono u
      · have hj : joinable M keep a = true := by
          cases hwa : M.wr a with
          | nil => exact absurd hwa hw
          | cons x l => exact joinable_of_wr (v := x) (by rw [hwa]; exact List.mem_cons_self)
        have := hc.join a hj (by
          simp only [trigger, Bool.or_eq_true, List.any_eq_true, Bool.and_eq_true]
          exact Or.inr ⟨u, hf.raw_rd a u hu, hut, by simp [hm]⟩)
        rw [ha] at this; exact absurd this (by simp)
      · rfl
    rcases hv.2 with h | ⟨u, hu, hex⟩
    · exact raw_mono v h
    · exact hf.ex_mono u v (raw_mono u hu) hex
  e_seed r hr v hv hs := ⟨hs, Or.inr ⟨r, hr, hv⟩⟩
  e_keep a hk := hc.join a (by simp [joinable, hk]) (by simp [trigger, hk])
  inputs_t _ h := h.1

/-- Lemma 5 of the sets `split_at` computes. -/
theorem lemma5_computed {anchor : Option V} {raw : A → List V} {folded : A → V → Prop}
    (hf : FoldData M mono (computed M mono keep anchor).1 (computed M mono keep anchor).2 raw
      folded) :
    Lemma5 M mono keep seeds (computed M mono keep anchor).1 (computed M mono keep anchor).2
      (obsOf M seeds (computed M mono keep anchor).1 (computed M mono keep anchor).2)
      (inputsOf M (computed M mono keep anchor).1 (computed M mono keep anchor).2 raw) folded :=
  lemma5_of_closed computed_closed hf

omit [DecidableEq V] in
/-- Claim A12: no counter is an input, given that `monotone_vars` marks every counter
cyclic. Every counter is in `T_v` (`Closed.init`). -/
theorem no_counter_input {obs inputs : V → Prop} {folded : A → V → Prop}
    (h5 : Lemma5 M mono keep seeds tv ta obs inputs folded)
    (hcnt : ∀ v, M.counted v = true → mono v = false) {v : V} (hv : M.counted v = true) :
    ¬ inputs v := by
  intro hi
  have := h5.d v hi
  rw [hcnt v hv] at this
  exact absurd this (by simp)

end Lemma5

/-! ## The cone system over `T_v × S_v`, and `Split` -/

section Cone

variable (M : Model V E A Val) (tv : V → Bool) (ta : A → Bool)

/-- A `T_v` valuation and an `S_v` valuation. -/
abbrev TV := {v : V // tv v = true} → Val
abbrev SV := {v : V // tv v = false} → Val

/-- The valuation of a pair. -/
def join (p : TV tv (Val := Val) × SV tv (Val := Val)) (v : V) : Val :=
  if h : tv v = true then p.1 ⟨v, h⟩ else p.2 ⟨v, Bool.eq_false_iff.mpr h⟩

/-- The labels: a `T_a` step, or a step of an action outside `T_a` (an `S_a` action, or one
that writes nothing, which stutters: `Local.idle`). -/
abbrev TaL := {l : A × Binding // ta l.1 = true}
abbrev SaL := {l : A × Binding // ta l.1 = false}

/-- The cone system with its states split as Definition 3 splits them: `Reduction.lean`'s
`C`. -/
def cone : LTS (TV tv (Val := Val) × SV tv (Val := Val)) (TaL ta ⊕ SaL ta) where
  init p := M.init (join tv p)
  step p l q := match l with
    | .inl l => M.lts.step (join tv p) l.1 (join tv q)
    | .inr l => M.lts.step (join tv p) l.1 (join tv q)

/-- The observation `obs`, as a map from `S_v` valuations. -/
def obsMap {obs : V → Prop} (hobs : ∀ v, obs v → tv v = false) (s : SV tv (Val := Val)) :
    {v : V // obs v} → Val :=
  fun v => s ⟨v.1, hobs v.1 v.2⟩

variable {M tv ta}

omit [DecidableEq V] in
theorem join_t {p : TV tv (Val := Val) × SV tv (Val := Val)} {v : V} (h : tv v = true) :
    join tv p v = p.1 ⟨v, h⟩ := by
  simp [join, h]

omit [DecidableEq V] in
theorem join_s {p : TV tv (Val := Val) × SV tv (Val := Val)} {v : V} (h : tv v = false) :
    join tv p v = p.2 ⟨v, h⟩ := by
  simp [join, h]

/-- **Lemma 5 (a)(b) give `Split`.** Under `Local`, any sets that satisfy Lemma 5 make the
cone system over `T_v × S_v` meet `Reduction.lean`'s hypothesis `Split`: an `S_a` step keeps
`T`, a `T_a` step keeps `S` and reads `S` only through `obs`. -/
theorem split_of_lemma5 {obs : V → Prop} (hL : Local M) (h5 : Lemma5ab M tv ta obs) :
    RdraIsh.Split (cone M tv ta) (obsMap tv h5.obs_s) where
  s_keeps_t := by
    rintro t s ⟨⟨a, b⟩, ha⟩ t' s' ⟨_, he⟩
    funext ⟨v, hv⟩
    have hnw : v ∉ M.wr a := fun hw => by
      have : M.wr a ≠ [] := fun h => by rw [h] at hw; exact List.not_mem_nil hw
      rw [h5.a a ⟨ha, this⟩ v hw] at hv; exact absurd hv (by simp)
    have := congrFun he v
    rw [join_t hv, hL.eff_frame a b _ v hnw, join_t hv] at this
    exact this
  t_keeps_s := by
    rintro t s ⟨⟨a, b⟩, ha⟩ t' s' ⟨_, he⟩
    funext ⟨v, hv⟩
    have hnw : v ∉ M.wr a := fun hw => by
      rw [h5.b_wr a ha v hw] at hv; exact absurd hv (by simp)
    have := congrFun he v
    rw [join_s hv, hL.eff_frame a b _ v hnw, join_s hv] at this
    exact this
  t_reads_obs := by
    rintro t s₁ s₂ ⟨⟨a, b⟩, ha⟩ t' hobs ⟨hg, he⟩
    have hag : Agree (M.rd a ++ M.wr a) (join tv (t, s₁)) (join tv (t, s₂)) := by
      intro v hv
      cases htv : tv v
      · rcases List.mem_append.mp hv with hr | hw
        · rw [join_s htv, join_s htv]
          exact congrFun hobs ⟨v, h5.b_rd a ha v hr htv⟩
        · rw [h5.b_wr a ha v hw] at htv; exact absurd htv (by simp)
      · rw [join_t htv, join_t htv]
    refine ⟨hL.guard_local hag hg, ?_⟩
    funext v
    have h1 := congrFun he v
    cases htv : tv v
    · have hnw : v ∉ M.wr a := fun hw => by
        rw [h5.b_wr a ha v hw] at htv; exact absurd htv (by simp)
      rw [join_s htv, hL.eff_frame a b _ v hnw, join_s htv]
    · rw [join_t htv]
      rw [join_t htv] at h1
      by_cases hw : v ∈ M.wr a
      · rw [h1, hL.eff_local a b _ _ hag v hw]
      · rw [hL.eff_frame a b _ v hw, join_t htv]
        rw [hL.eff_frame a b _ v hw, join_t htv] at h1
        exact h1

/-- `Split` of the sets `split_at` computes, with Definition 3's `obs`: the hypothesis
`Split` of Theorem A8 (`a8_dbsb`) from `Local` alone. The folds play no part. -/
theorem split_computed {mono : V → Bool} {keep : A → Bool} {anchor : Option V}
    {seeds : List (List V)} (hL : Local M) :
    RdraIsh.Split (cone M (computed M mono keep anchor).1 (computed M mono keep anchor).2)
      (obsMap (computed M mono keep anchor).1
        (lemma5ab_of_closed (seeds := seeds) computed_closed).obs_s) :=
  split_of_lemma5 hL (lemma5ab_of_closed computed_closed)

/-- The pair of a valuation. -/
def unjoin (tv : V → Bool) (σ : V → Val) : TV tv (Val := Val) × SV tv (Val := Val) :=
  (fun v => σ v.1, fun v => σ v.1)

omit [DecidableEq V] in
theorem join_unjoin (σ : V → Val) : join tv (unjoin tv σ) = σ := by
  funext v
  cases h : tv v
  · rw [join_s h]; rfl
  · rw [join_t h]; rfl

omit [DecidableEq V] in
theorem unjoin_join (p : TV tv (Val := Val) × SV tv (Val := Val)) : unjoin tv (join tv p) = p := by
  obtain ⟨t, s⟩ := p
  refine Prod.ext (funext fun ⟨v, h⟩ => ?_) (funext fun ⟨v, h⟩ => ?_)
  · show join tv (t, s) v = t ⟨v, h⟩
    exact join_t h
  · show join tv (t, s) v = s ⟨v, h⟩
    exact join_s h

omit [DecidableEq V] in
/-- `cone` is the cone system `lts` with its states split: a pair is reachable in `cone` iff
its valuation is reachable in `lts`. -/
theorem cone_reach_iff (p : TV tv (Val := Val) × SV tv (Val := Val)) :
    (cone M tv ta).Reach p ↔ M.lts.Reach (join tv p) := by
  constructor
  · intro h
    induction h with
    | init h => exact Reach.init h
    | @step p l q _ hs ih =>
      cases l with
      | inl l => exact Reach.step ih hs
      | inr l => exact Reach.step ih hs
  · have key : ∀ σ, M.lts.Reach σ → (cone M tv ta).Reach (unjoin tv σ) := by
      intro σ h
      induction h with
      | init h =>
        apply Reach.init
        show M.init (join tv (unjoin tv _))
        rw [join_unjoin]; exact h
      | @step σ l τ _ hs ih =>
        cases hl : ta l.1
        · apply Reach.step ih (a := .inr ⟨l, hl⟩)
          show M.lts.step (join tv (unjoin tv σ)) l (join tv (unjoin tv τ))
          rw [join_unjoin, join_unjoin]; exact hs
        · apply Reach.step ih (a := .inl ⟨l, hl⟩)
          show M.lts.step (join tv (unjoin tv σ)) l (join tv (unjoin tv τ))
          rw [join_unjoin, join_unjoin]; exact hs
    intro h
    have := key _ h
    rwa [unjoin_join] at this

end Cone

end RdraIsh.Act
