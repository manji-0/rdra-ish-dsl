import RdraIsh.Split
import RdraIsh.Lemma5Cert

/-!
# The check of a Lemma 5 certificate is sound

A certificate (`Lemma5Cert.lean`) names variables and actions by number. Read over
`Fin nv` and `Fin na`, its lists are the sets of `Split.lean`'s `Lemma5`: `T_v`, `T_a`,
`obs`, `inputs`, the folded reads, the monotone variables, the kept actions and the seeds.
`Fits` says a model's reads and writes are the certificate's, and nothing else of the model:
its guards, effects and initial states are free.

- `check_sound`: a certificate `check` accepts satisfies `Lemma5` for every model that fits
  it. `model` is one such model, the certificate's reads and writes with no guards and no
  effects (`fits_model`), and `check_lemma5` is `check_sound` for it;
- `sides_of_check`: its `S_v` is the complement of `T_v`, and its `S_a` is `Split.lean`'s
  `Sa`, the writers outside `T_a`;
- `split_of_check`: under `Local`, the cone system over `T_v × S_v` meets
  `Reduction.lean`'s hypothesis `Split`, the hypothesis of Theorem A8 (`a8_dbsb`).

So the sets the code's check accepts on a run, once `lemma5-check` accepts their
certificate, meet the hypothesis the Lean proof of A8 takes, given that the code's reads and
writes bound what each action looks at (`Local`, which nothing here checks).
-/

namespace RdraIsh.Cert

open RdraIsh.Act

/-- The ids of `l` below `n`. -/
def toFin (n : Nat) (l : List Nat) : List (Fin n) :=
  l.filterMap fun i => if h : i < n then some ⟨i, h⟩ else none

theorem mem_toFin {n : Nat} {l : List Nat} {v : Fin n} : v ∈ toFin n l ↔ v.val ∈ l := by
  simp only [toFin, List.mem_filterMap]
  constructor
  · rintro ⟨i, hi, he⟩
    by_cases h : i < n
    · simp only [h, dite_true, Option.some.injEq] at he
      subst he; exact hi
    · simp [h] at he
  · intro hv
    exact ⟨v.val, hv, by simp⟩

theorem mem_getD {l : List (List Nat)} {a x : Nat} (h : x ∈ l.getD a []) : ∃ r ∈ l, x ∈ r := by
  rw [List.getD_eq_getElem?_getD] at h
  cases hg : l[a]? with
  | none => simp [hg] at h
  | some r => simp [hg] at h; exact ⟨r, List.mem_of_getElem? hg, h⟩

namespace Data

variable (d : Data)

/-- The certificate's sets over `Fin nv` and `Fin na`. -/
def tvF (v : Fin d.nv) : Bool := d.t.contains v.val
def taF (a : Fin d.na) : Bool := d.ta.contains a.val
def monoF (v : Fin d.nv) : Bool := d.mono.contains v.val
def keepF (a : Fin d.na) : Bool := d.keep.contains a.val
def obsP (v : Fin d.nv) : Prop := v.val ∈ d.obs
def inputsP (v : Fin d.nv) : Prop := v.val ∈ d.inputs
def foldedP (a : Fin d.na) (v : Fin d.nv) : Prop := v.val ∈ d.foldedOf a.val
def seedsF : List (List (Fin d.nv)) := d.seeds.map (toFin d.nv)

/-- A model whose reads and writes are the certificate's. -/
structure Fits {E Val : Type} (M : Model (Fin d.nv) E (Fin d.na) Val) : Prop where
  rd : ∀ a v, v ∈ M.rd a ↔ v.val ∈ d.rdOf a.val
  wr : ∀ a v, v ∈ M.wr a ↔ v.val ∈ d.wrOf a.val

variable {d}

theorem wf_wr (h : d.wellFormed = true) {a x : Nat} (hx : x ∈ d.wrOf a) : x < d.nv := by
  obtain ⟨r, hr, hxr⟩ := mem_getD hx
  simp only [wellFormed, Bool.and_eq_true, List.all_eq_true, decide_eq_true_eq] at h
  exact h.1.1.1.1.1.1.1.1.2 r hr x hxr

/-- An action of the model is in `Sa` iff the certificate's `inSa` holds of it. -/
theorem sa_iff {E Val : Type} {M : Model (Fin d.nv) E (Fin d.na) Val} (hf : d.Fits M)
    (hw : d.wellFormed = true) (a : Fin d.na) : Sa M d.taF a ↔ d.inSa a.val = true := by
  have hne : M.wr a ≠ [] ↔ (d.wrOf a.val).isEmpty = false := by
    constructor
    · intro h
      obtain ⟨v, hv⟩ := List.exists_mem_of_ne_nil _ h
      have := (hf.wr a v).1 hv
      cases hl : d.wrOf a.val with
      | nil => rw [hl] at this; exact absurd this List.not_mem_nil
      | cons _ _ => rfl
    · intro h hm
      cases hl : d.wrOf a.val with
      | nil => rw [hl] at h; exact absurd h (by simp)
      | cons x l =>
        have hx : x ∈ d.wrOf a.val := by rw [hl]; exact List.mem_cons_self
        have := (hf.wr a ⟨x, wf_wr hw hx⟩).2 hx
        rw [hm] at this; exact List.not_mem_nil this
  simp only [Sa, inSa, taF, hne, Bool.and_eq_true, Bool.not_eq_true']

/-- The parts of an accepted certificate. -/
theorem parts (h : d.check = true) :
    d.wellFormed = true ∧ d.checkSides = true ∧ d.checkA = true ∧ d.checkBWr = true ∧
      d.checkBRd = true ∧ d.checkObsS = true ∧ d.checkC = true ∧ d.checkD = true ∧
      d.checkESeed = true ∧ d.checkEKeep = true ∧ d.checkInputsT = true := by
  simp only [check, Bool.and_eq_true] at h
  obtain ⟨⟨⟨⟨⟨⟨⟨⟨⟨⟨h1, h2⟩, h3⟩, h4⟩, h5⟩, h6⟩, h7⟩, h8⟩, h9⟩, h10⟩, h11⟩ := h
  exact ⟨h1, h2, h3, h4, h5, h6, h7, h8, h9, h10, h11⟩

/-- **The check is sound**: a certificate it accepts satisfies Lemma 5 (a)–(e) for every
model whose reads and writes are the certificate's. -/
theorem check_sound (h : d.check = true) {E Val : Type}
    (M : Model (Fin d.nv) E (Fin d.na) Val) (hf : d.Fits M) :
    Lemma5 M d.monoF d.keepF d.seedsF d.tvF d.taF d.obsP d.inputsP d.foldedP := by
  obtain ⟨hw, -, hA, hBW, hBR, hOS, hC, hD, hES, hEK, hIT⟩ := parts h
  simp only [checkA, checkBWr, checkBRd, checkObsS, checkC, checkD, checkESeed, checkEKeep,
    checkInputsT, List.all_eq_true, List.mem_range, Bool.or_eq_true, Bool.not_eq_true',
    List.contains_iff_mem] at hA hBW hBR hOS hC hD hES hEK hIT
  refine
    { a := ?_, b_wr := ?_, b_rd := ?_, obs_s := ?_, c := ?_, d := ?_, e_seed := ?_,
      e_keep := ?_, inputs_t := ?_ }
  · intro a ha v hv
    have hs := (sa_iff hf hw a).1 ha
    rcases hA a.val a.isLt with h | h
    · rw [hs] at h; exact absurd h (by simp)
    · have := h v.val ((hf.wr a v).1 hv)
      simpa [tvF] using this
  · intro a ha v hv
    have := hBW a.val (by simpa [taF] using ha) v.val ((hf.wr a v).1 hv)
    simpa [tvF] using this
  · intro a ha v hv hs
    rcases hBR a.val (by simpa [taF] using ha) v.val ((hf.rd a v).1 hv) with h | h
    · simp [tvF, h] at hs
    · exact h
  · intro v hv
    simpa [tvF] using hOS v.val hv
  · intro a ha v hv
    have hs := (sa_iff hf hw a).1 ha
    rcases hC a.val a.isLt with h | h
    · rw [hs] at h; exact absurd h (by simp)
    · rcases h v.val ((hf.rd a v).1 hv) with (h | h) | h
      · exact Or.inl (by simpa [tvF] using h)
      · exact Or.inr (Or.inl h)
      · exact Or.inr (Or.inr h)
  · intro v hv
    simpa [monoF] using hD v.val hv
  · intro r hr v hv hs
    simp only [seedsF, List.mem_map] at hr
    obtain ⟨r0, hr0, rfl⟩ := hr
    rcases hES r0 hr0 v.val (mem_toFin.1 hv) with h | h
    · simp [tvF, h] at hs
    · exact h
  · intro a ha
    simpa [taF] using hEK a.val (by simpa [keepF] using ha)
  · intro v hv
    simpa [tvF] using hIT v.val hv

/-- The model of a certificate: its reads and writes, and nothing else (no entities, no
guards, effects that change nothing, every state initial). -/
def model (d : Data) : Model (Fin d.nv) Unit (Fin d.na) Unit where
  vars := List.finRange d.nv
  acts := List.finRange d.na
  vars_all := List.mem_finRange
  acts_all := List.mem_finRange
  own _ := none
  existences _ := []
  counted _ := false
  params _ := []
  guards _ := []
  rd a := toFin d.nv (d.rdOf a.val)
  wr a := toFin d.nv (d.wrOf a.val)
  eff _ _ σ := σ
  init _ := True

/-- The model of a certificate fits it, so `Fits` is never empty. -/
theorem fits_model (d : Data) : d.Fits d.model where
  rd _ _ := mem_toFin
  wr _ _ := mem_toFin

/-- An accepted certificate satisfies Lemma 5 over its own model. -/
theorem check_lemma5 (h : d.check = true) :
    Lemma5 d.model d.monoF d.keepF d.seedsF d.tvF d.taF d.obsP d.inputsP d.foldedP :=
  check_sound h d.model (fits_model d)

/-- An accepted certificate's `S_v` and `S_a` are the complement of `T_v` and the writers
outside `T_a`. -/
theorem sides_of_check (h : d.check = true) {E Val : Type}
    {M : Model (Fin d.nv) E (Fin d.na) Val} (hf : d.Fits M) :
    (∀ v : Fin d.nv, v.val ∈ d.s ↔ d.tvF v = false) ∧
      (∀ a : Fin d.na, a.val ∈ d.sa ↔ Sa M d.taF a) := by
  obtain ⟨hw, hS, -⟩ := parts h
  simp only [checkSides, Bool.and_eq_true, List.all_eq_true, List.mem_range] at hS
  refine ⟨fun v => ?_, fun a => ?_⟩
  · have hv := hS.1 v.val v.isLt
    rw [tvF, Bool.eq_false_iff, ne_eq, List.contains_iff_mem]
    by_cases hs : v.val ∈ d.s <;> by_cases ht : v.val ∈ d.t
    · simp [hs, ht] at hv
    · exact iff_of_true hs ht
    · exact iff_of_false hs (fun h => h ht)
    · simp [hs, ht] at hv
  · rw [sa_iff hf hw a]
    have := hS.2 a.val a.isLt
    rw [beq_iff_eq] at this
    rw [← this, List.contains_iff_mem]

/-- Under `Local`, an accepted certificate's split meets `Split` of `Reduction.lean`, the
hypothesis of Theorem A8. -/
theorem split_of_check (h : d.check = true) {E Val : Type}
    {M : Model (Fin d.nv) E (Fin d.na) Val} (hf : d.Fits M) (hL : Local M) :
    RdraIsh.Split (cone M d.tvF d.taF) (obsMap d.tvF (check_sound h M hf).obs_s) :=
  split_of_lemma5 hL (check_sound h M hf).toLemma5ab

end Data

end RdraIsh.Cert
