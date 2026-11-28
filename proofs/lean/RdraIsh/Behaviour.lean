import RdraIsh.LTS

/-!
# Infinite behaviours, weak fairness (D20), liveness verdicts (D21), and lassos

A *behaviour* of a system is a sequence `ρ : Nat → σ` that starts in an initial state and whose
every step is a step of the system or a stutter (`ρ (n+1) = ρ n`), as D14 and TLA+ read it.

Fairness follows docs/theory/semantics.md, D20, and assumption τ of docs/theory/native.md:

- A *fair instance* `f : Φ` stands for a set of action labels (`inst f a`: the label `a`, an
  action with a binding, is a binding of the instance's action that extends its slots `κ`).
- `f` is *taken* by a step `s → s'` when `s' ≠ s` and some label of `f` moves `s` to `s'`.
  A step that changes nothing takes nothing (τ).
- `f` is *enabled* at `s` when some step takes it from `s`.
- `Fair_P` (`FairP`) is weak fairness per instance: an instance enabled at every state from
  some point on is taken infinitely often.
- `WF_vars(Next)` (`WFNext`): if from some point on a step to another state is enabled at every
  state, infinitely many steps change the state.

`StopsOnly stop ρ` says a behaviour that stays at `q` forever does so only where `stop q`
holds. `wfNext_iff` shows `WF_vars(Next)` is `StopsOnly` with `stop q := ¬ Moves q`; the
reduced run of Theorem A15 widens that stop to the classes that can loop.

A *lasso* is an ultimately periodic behaviour: from index `k` on it repeats with period
`p > 0`, so a finite prefix and a finite cycle determine it. `lasso_of_behaviour` is the
pigeonhole step: on a system whose reachable states are finitely many (a `List` covering
them), every behaviour that is fair and violates a liveness property can be folded into a
lasso that is fair and violates it too. Nothing here uses Mathlib's `Finset` or `Fintype`; the
finiteness hypothesis is a list of states.
-/

namespace RdraIsh

universe u v w

/-! ## Infinitely often, eventually always -/

/-- `P` holds at infinitely many indices. -/
def InfOften (P : Nat → Prop) : Prop := ∀ n, ∃ m, n ≤ m ∧ P m

/-- `P` holds at every index from some point on. -/
def EvAlways (P : Nat → Prop) : Prop := ∃ n, ∀ m, n ≤ m → P m

/-- A predicate that does not hold infinitely often is false from some point on. -/
theorem not_infOften {P : Nat → Prop} (h : ¬ InfOften P) : ∃ n, ∀ m, n ≤ m → ¬ P m := by
  refine Classical.byContradiction fun hc => h fun n => ?_
  refine Classical.byContradiction fun hn => hc ⟨n, fun m hm hp => hn ⟨m, hm, hp⟩⟩

/-- A sequence that stutters at every index from `n` on is constant from `n` on. -/
theorem const_of_stutter {σ : Type u} {ρ : Nat → σ} {n : Nat}
    (h : ∀ m, n ≤ m → ρ (m + 1) = ρ m) : ∀ m, n ≤ m → ρ m = ρ n := by
  intro m hm
  induction m with
  | zero =>
    have : n = 0 := by omega
    subst this; rfl
  | succ m ih =>
    by_cases hnm : n ≤ m
    · rw [h m hnm, ih hnm]
    · have : n = m + 1 := by omega
      subst this; rfl

/-! ## Behaviours and fairness -/

namespace LTS

variable {σ : Type u} {α : Type v} {Φ : Type w}

/-- A behaviour (D14 with stuttering): an initial state, then steps or stutters. -/
def Behaviour (M : LTS σ α) (ρ : Nat → σ) : Prop :=
  M.init (ρ 0) ∧ ∀ n, ρ (n + 1) = ρ n ∨ M.Next (ρ n) (ρ (n + 1))

theorem Behaviour.reach {M : LTS σ α} {ρ : Nat → σ} (h : M.Behaviour ρ) :
    ∀ n, M.Reach (ρ n) := by
  intro n
  induction n with
  | zero => exact Reach.init h.1
  | succ n ih =>
    rcases h.2 n with he | ⟨a, ha⟩
    · rw [he]; exact ih
    · exact Reach.step ih ha

/-- A fair instance `f` is taken by the step `s → s'` (D20 with τ). -/
def Takes (M : LTS σ α) (inst : Φ → α → Prop) (f : Φ) (s s' : σ) : Prop :=
  s' ≠ s ∧ ∃ a, inst f a ∧ M.step s a s'

/-- `f` is enabled at `s`: some step takes it. -/
def Enabled (M : LTS σ α) (inst : Φ → α → Prop) (f : Φ) (s : σ) : Prop :=
  ∃ s', M.Takes inst f s s'

/-- Some step leaves `s` for another state. -/
def Moves (M : LTS σ α) (s : σ) : Prop := ∃ s', s' ≠ s ∧ M.Next s s'

/-- `Fair_P`: weak fairness per fair instance. -/
def FairP (M : LTS σ α) (inst : Φ → α → Prop) (ρ : Nat → σ) : Prop :=
  ∀ f, EvAlways (fun n => M.Enabled inst f (ρ n)) →
    InfOften (fun n => M.Takes inst f (ρ n) (ρ (n + 1)))

/-- `WF_vars(Next)`. -/
def WFNext (M : LTS σ α) (ρ : Nat → σ) : Prop :=
  EvAlways (fun n => M.Moves (ρ n)) → InfOften (fun n => ρ (n + 1) ≠ ρ n)

/-- A behaviour stays at one state forever only at a state where `stop` holds. -/
def StopsOnly (stop : σ → Prop) (ρ : Nat → σ) : Prop :=
  ∀ n q, (∀ m, n ≤ m → ρ m = q) → stop q

/-- `WF_vars(Next)` says exactly that a behaviour stays forever only where no step leaves. -/
theorem wfNext_iff (M : LTS σ α) (ρ : Nat → σ) :
    M.WFNext ρ ↔ StopsOnly (fun q => ¬ M.Moves q) ρ := by
  constructor
  · intro hwf n q hc hm
    obtain ⟨m, hm', hne⟩ := hwf ⟨n, fun m hnm => by show M.Moves (ρ m); rw [hc m hnm]; exact hm⟩ n
    exact hne (by rw [hc (m + 1) (by omega), hc m hm'])
  · intro hs ⟨N, hN⟩
    refine Classical.byContradiction fun hinf => ?_
    obtain ⟨n, hn⟩ := not_infOften hinf
    have hst : ∀ m, n ≤ m → ρ (m + 1) = ρ m := fun m hm =>
      Classical.byContradiction (hn m hm)
    have hc := const_of_stutter hst
    have hstop := hs n (ρ n) hc
    apply hstop
    have : M.Moves (ρ (max N n)) := hN (max N n) (by omega)
    rwa [hc (max N n) (by omega)] at this

end LTS

/-! ## The liveness verdict (D21) -/

/-- `ρ` violates `□(trig ⇒ ◇goal)`: some state meets the trigger and no state from there meets
the goal. `trig` stands for `trig_b ∧ cause_b` of D21 at a fixed binding `b`. -/
def Violates {σ : Type u} (trig goal : σ → Prop) (ρ : Nat → σ) : Prop :=
  ∃ i, trig (ρ i) ∧ ∀ j, i ≤ j → ¬ goal (ρ j)

/-- D21: the property holds under the fairness `fair` when no behaviour that satisfies `fair`
violates it. -/
def LTS.LiveHolds {σ : Type u} {α : Type v} (M : LTS σ α) (fair : (Nat → σ) → Prop)
    (trig goal : σ → Prop) : Prop :=
  ∀ ρ, M.Behaviour ρ → fair ρ → ¬ Violates trig goal ρ

/-- An ultimately periodic sequence: a finite prefix of length `k`, then a cycle of length
`p > 0` repeated forever. -/
def Lasso {σ : Type u} (ρ : Nat → σ) : Prop :=
  ∃ k p, 0 < p ∧ ∀ n, k ≤ n → ρ (n + p) = ρ n

/-! ## Pigeonhole over a list -/

section Pigeonhole

variable {β : Type u}

/-- For finitely many events, a point after which every event either still happens infinitely
often or never happens again. -/
theorem eventually_split (L : List β) (occ : β → Nat → Prop) :
    ∃ n₀, ∀ b, b ∈ L → InfOften (occ b) ∨ ∀ m, n₀ ≤ m → ¬ occ b m := by
  induction L with
  | nil => exact ⟨0, fun b hb => nomatch hb⟩
  | cons b t ih =>
    obtain ⟨n₁, h₁⟩ := ih
    by_cases hb : InfOften (occ b)
    · refine ⟨n₁, fun b' hb' => ?_⟩
      rcases List.mem_cons.mp hb' with rfl | ht
      · exact .inl hb
      · exact h₁ b' ht
    · obtain ⟨n₂, h₂⟩ := not_infOften hb
      refine ⟨max n₁ n₂, fun b' hb' => ?_⟩
      rcases List.mem_cons.mp hb' with rfl | ht
      · exact .inr fun m hm => h₂ m (by omega)
      · rcases h₁ b' ht with h | h
        · exact .inl h
        · exact .inr fun m hm => h m (by omega)

/-- A finite window after `N` in which every infinitely recurring event of the list happens. -/
theorem window_covers (L : List β) (occ : β → Nat → Prop) (N : Nat) :
    ∃ M, ∀ b, b ∈ L → InfOften (occ b) → ∃ j, N ≤ j ∧ j < M ∧ occ b j := by
  induction L with
  | nil => exact ⟨0, fun b hb => nomatch hb⟩
  | cons b t ih =>
    obtain ⟨M₁, h₁⟩ := ih
    by_cases hb : InfOften (occ b)
    · obtain ⟨j, hj, hoc⟩ := hb N
      refine ⟨max M₁ (j + 1), fun b' hb' hinf => ?_⟩
      rcases List.mem_cons.mp hb' with rfl | ht
      · exact ⟨j, hj, by omega, hoc⟩
      · obtain ⟨j', h1, h2, h3⟩ := h₁ b' ht hinf
        exact ⟨j', h1, by omega, h3⟩
    · refine ⟨M₁, fun b' hb' hinf => ?_⟩
      rcases List.mem_cons.mp hb' with rfl | ht
      · exact absurd hinf hb
      · exact h₁ b' ht hinf

end Pigeonhole

/-- All pairs of a list. -/
def pairs {σ : Type u} (L : List σ) : List (σ × σ) := L.flatMap fun a => L.map fun b => (a, b)

theorem mem_pairs {σ : Type u} {L : List σ} {a b : σ} (ha : a ∈ L) (hb : b ∈ L) :
    (a, b) ∈ pairs L :=
  List.mem_flatMap.mpr ⟨a, ha, List.mem_map.mpr ⟨b, hb, rfl⟩⟩

/-- **The recurrent segment.** A sequence over finitely many states has, after any index `i`,
a segment `[n₀, m)` with `ρ m = ρ n₀` in which every step `(ρ k, ρ (k+1))` with `k ≥ n₀`
occurs. -/
theorem recurrent_segment {σ : Type u} (ρ : Nat → σ) (L : List σ) (hL : ∀ n, ρ n ∈ L)
    (i : Nat) :
    ∃ n₀ m, i ≤ n₀ ∧ n₀ < m ∧ ρ m = ρ n₀ ∧
      ∀ k, n₀ ≤ k → ∃ j, n₀ ≤ j ∧ j < m ∧ ρ j = ρ k ∧ ρ (j + 1) = ρ (k + 1) := by
  let occ : σ × σ → Nat → Prop := fun ab k => ρ k = ab.1 ∧ ρ (k + 1) = ab.2
  obtain ⟨n₁, h₁⟩ := eventually_split (pairs L) occ
  let n₀ := max n₁ i
  have hinf : ∀ k, n₀ ≤ k → InfOften (occ (ρ k, ρ (k + 1))) := by
    intro k hk
    rcases h₁ _ (mem_pairs (hL k) (hL (k + 1))) with h | h
    · exact h
    · exact absurd ⟨rfl, rfl⟩ (h k (by omega))
  obtain ⟨M, hM⟩ := window_covers (pairs L) occ n₀
  obtain ⟨m, hm, hmo, _⟩ := hinf n₀ (Nat.le_refl _) (max M (n₀ + 1))
  refine ⟨n₀, m, by omega, by omega, hmo, fun k hk => ?_⟩
  obtain ⟨j, hj1, hj2, hj3, hj4⟩ :=
    hM _ (mem_pairs (hL k) (hL (k + 1))) (hinf k hk)
  exact ⟨j, hj1, by omega, hj3, hj4⟩

/-! ## Folding a segment into a lasso -/

section Fold

variable {σ : Type u} (ρ : Nat → σ) (n₀ m : Nat)

/-- The index the lasso reads at time `n`: `n` until `m`, then round `[n₀, m)` again. -/
def foldIdx : Nat → Nat
  | 0 => 0
  | n + 1 => if foldIdx n + 1 = m then n₀ else foldIdx n + 1

variable {n₀ m}

theorem foldIdx_lt (h : n₀ < m) : ∀ n, foldIdx n₀ m n < m := by
  intro n
  induction n with
  | zero => simp only [foldIdx]; omega
  | succ n ih =>
    simp only [foldIdx]
    split <;> omega

theorem foldIdx_eq : ∀ n, n < m → foldIdx n₀ m n = n := by
  intro n
  induction n with
  | zero => intro; rfl
  | succ n ih =>
    intro hn
    simp only [foldIdx, ih (by omega)]
    split <;> omega

theorem foldIdx_ge (h : n₀ < m) : ∀ n, n₀ ≤ n → n₀ ≤ foldIdx n₀ m n := by
  intro n
  induction n with
  | zero => intro hn; rw [foldIdx_eq 0 (by omega)]; exact hn
  | succ n ih =>
    intro hn
    by_cases hn' : n₀ ≤ n
    · have := ih hn'
      simp only [foldIdx]
      split <;> omega
    · rw [foldIdx_eq (n + 1) (by omega)]; exact hn

/-- Without a wrap, the index advances by one per step. -/
theorem foldIdx_add (n : Nat) :
    ∀ d, foldIdx n₀ m n + d < m → foldIdx n₀ m (n + d) = foldIdx n₀ m n + d := by
  intro d
  induction d with
  | zero => intro; rfl
  | succ d ih =>
    intro hd
    show foldIdx n₀ m (n + d + 1) = _
    simp only [foldIdx, ih (by omega)]
    split <;> omega

/-- From any time at or after `n₀`, the index comes back to every `j ∈ [n₀, m)` within one
period, and it is periodic with period `m - n₀`. -/
theorem foldIdx_reaches (h : n₀ < m) (n : Nat) (hn : n₀ ≤ n) (j : Nat) (hj1 : n₀ ≤ j)
    (hj2 : j < m) : ∃ d, foldIdx n₀ m (n + d) = j ∧
      (j = foldIdx n₀ m n → d = m - n₀) := by
  have hi := foldIdx_lt h n
  have hi0 := foldIdx_ge h n hn
  let i := foldIdx n₀ m n
  -- Run to `m - 1`, wrap to `n₀`, then run to `j`.
  have h1 : foldIdx n₀ m (n + (m - 1 - i)) = m - 1 := by
    rw [foldIdx_add n (m - 1 - i) (by omega)]; omega
  have h2 : foldIdx n₀ m (n + (m - 1 - i) + 1) = n₀ := by
    simp only [foldIdx, h1]
    split <;> omega
  have h3 : foldIdx n₀ m (n + (m - 1 - i) + 1 + (j - n₀)) = j := by
    rw [foldIdx_add _ (j - n₀) (by omega), h2]; omega
  refine ⟨m - 1 - i + 1 + (j - n₀), ?_, fun hji => by omega⟩
  rw [← h3]; congr 1; omega

/-- The lasso of the segment `[n₀, m)`. -/
def foldSeq (n : Nat) : σ := ρ (foldIdx n₀ m n)

theorem foldSeq_succ (hm : ρ m = ρ n₀) (n : Nat) :
    foldSeq ρ (n₀ := n₀) (m := m) (n + 1) = ρ (foldIdx n₀ m n + 1) := by
  simp only [foldSeq, foldIdx]
  split
  · next he => rw [he, hm]
  · rfl

theorem foldSeq_lasso (h : n₀ < m) : Lasso (foldSeq ρ (n₀ := n₀) (m := m)) := by
  refine ⟨n₀, m - n₀, by omega, fun n hn => ?_⟩
  obtain ⟨d, hd, hdp⟩ := foldIdx_reaches h n hn (foldIdx n₀ m n) (foldIdx_ge h n hn)
    (foldIdx_lt h n)
  rw [hdp rfl] at hd
  simp only [foldSeq, hd]

end Fold

/-! ## Lassos on a finite system -/

namespace LTS

variable {σ : Type u} {α : Type v} {Φ : Type w}

/-- **Lasso existence.** On a system whose reachable states lie in the list `L`, a behaviour
that satisfies `Fair_P`, stays forever only where `stop` holds, and violates
`□(trig ⇒ ◇goal)` folds into a lasso with all four properties. The same holds with
`WF_vars(Next)` in place of `StopsOnly` (`wfNext_iff`). -/
theorem lasso_of_behaviour (M : LTS σ α) (L : List σ) (hL : ∀ s, M.Reach s → s ∈ L)
    (inst : Φ → α → Prop) (stop trig goal : σ → Prop) (ρ : Nat → σ)
    (hb : M.Behaviour ρ) (hf : M.FairP inst ρ) (hs : StopsOnly stop ρ)
    (hv : Violates trig goal ρ) :
    ∃ ρ', Lasso ρ' ∧ M.Behaviour ρ' ∧ M.FairP inst ρ' ∧ StopsOnly stop ρ' ∧
      Violates trig goal ρ' := by
  obtain ⟨i, hti, hgi⟩ := hv
  obtain ⟨n₀, m, hin, hnm, hmo, hcov⟩ :=
    recurrent_segment ρ L (fun n => hL _ (hb.reach n)) i
  let ρ' := foldSeq ρ (n₀ := n₀) (m := m)
  -- (A) every step of `ρ'` is a step of `ρ`, from the tail once `ρ'` is in its cycle.
  have hA : ∀ n, ∃ j, ρ' n = ρ j ∧ ρ' (n + 1) = ρ (j + 1) ∧ (n < n₀ → j = n) ∧
      (n₀ ≤ n → n₀ ≤ j) :=
    fun n => ⟨foldIdx n₀ m n, rfl, foldSeq_succ ρ hmo n,
      fun hn => foldIdx_eq n (by omega), foldIdx_ge hnm n⟩
  -- (B) every step of the tail of `ρ` recurs in `ρ'` after any time.
  have hB : ∀ k, n₀ ≤ k → ∀ N, ∃ n, N ≤ n ∧ ρ' n = ρ k ∧ ρ' (n + 1) = ρ (k + 1) := by
    intro k hk N
    obtain ⟨j, hj1, hj2, hj3, hj4⟩ := hcov k hk
    obtain ⟨d, hd, _⟩ := foldIdx_reaches hnm (max N n₀) (by omega) j hj1 hj2
    refine ⟨max N n₀ + d, by omega, ?_, ?_⟩
    · show ρ (foldIdx n₀ m _) = _; rw [hd, hj3]
    · show foldSeq ρ _ = _; rw [foldSeq_succ ρ hmo, hd, hj4]
  refine ⟨ρ', foldSeq_lasso ρ hnm, ⟨?_, fun n => ?_⟩, ?_, ?_, ?_⟩
  · show M.init (ρ (foldIdx n₀ m 0)); exact hb.1
  · obtain ⟨j, h1, h2, _⟩ := hA n
    rw [h1, h2]; exact hb.2 j
  · intro f ⟨N, hN⟩ N'
    have hen : EvAlways (fun n => M.Enabled inst f (ρ n)) := by
      refine ⟨n₀, fun k hk => ?_⟩
      obtain ⟨n, hn, h1, _⟩ := hB k hk N
      rw [← h1]; exact hN n hn
    obtain ⟨k, hk, htk⟩ := hf f hen n₀
    obtain ⟨n, hn, h1, h2⟩ := hB k hk N'
    exact ⟨n, hn, by show M.Takes inst f (ρ' n) (ρ' (n + 1)); rw [h1, h2]; exact htk⟩
  · intro N q hc
    refine hs n₀ q fun k hk => ?_
    obtain ⟨n, hn, h1, _⟩ := hB k hk N
    rw [← h1]; exact hc n hn
  · refine ⟨i, ?_, fun j hj => ?_⟩
    · show trig (ρ (foldIdx n₀ m i))
      rw [foldIdx_eq i (by omega)]; exact hti
    · obtain ⟨j', h1, _, h3, h4⟩ := hA j
      rw [h1]
      apply hgi
      by_cases hj' : j < n₀
      · rw [h3 hj']; exact hj
      · have := h4 (by omega); omega

end LTS

end RdraIsh
