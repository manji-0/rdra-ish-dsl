import RdraIsh.Fairness
import RdraIsh.ReductionLabels
import RdraIsh.Stages

/-!
# Theorem A8, consequence 4: `Cone_P` under `Plain`

A liveness property `□(trig ⇒ ◇goal)` at one binding (D21) reads `T_v ∪ obs`, and its fair
instances are sets of `T_a` labels (`inst`, read on `C` and `R` by `onT`; Lemma 5 (e) puts every
fair action in `T_a`). Under `Plain` the fairness is `Fair_P` alone (D20). Behaviours, `Fair_P`
and `LiveHolds` are those of `Behaviour.lean`.

`a8_plain_iff`: under Theorem A8's hypotheses the property holds on `C` iff it holds on `R`.

- `R ⊨ P ⇒ C ⊨ P` (`a8_plain_sound`): the image of a fair violating behaviour of `C` under
  `~` is a fair violating behaviour of `R` (`Fairness.lean`'s `image_step`, `enabled_back`,
  `takes_fwd`). A behaviour of `C` that steps inside one block forever maps to one that stops
  there; that stop is fair, since an instance enabled there is enabled at the `C` states too,
  and `C` takes it, which moves the image.
- `C ⊨ P ⇒ R ⊨ P` (`a8_plain_complete`): a fair violating behaviour of `R` lifts to one of `C`.
  Each step of `R` lifts to a finite segment of `C` (`seg_exists`): a stutter to a stutter, a
  `T_a` step to the same `T_a` step from the current state of `C`, a class move to the path
  `class_move_lifts` gives. The segments are glued into one sequence `Nat → T × S` by
  `walk`, which counts the steps through each segment.

`WF_vars(Next)` is not part of `Plain`; Theorem A15 (`Fairness.lean`) treats it.
-/

namespace RdraIsh

open LTS

/-! ## Gluing finite segments -/

/-- The position `(segment, offset)` at time `m`, for segments of lengths `len k`. -/
def walk (len : Nat → Nat) : Nat → Nat × Nat
  | 0 => (0, 0)
  | m + 1 =>
    if (walk len m).2 + 1 < len (walk len m).1 then ((walk len m).1, (walk len m).2 + 1)
    else ((walk len m).1 + 1, 0)

section Walk

variable {len : Nat → Nat} (hlen : ∀ k, 0 < len k)
include hlen

theorem walk_lt : ∀ m, (walk len m).2 < len (walk len m).1 := by
  intro m
  induction m with
  | zero => exact hlen 0
  | succ m _ =>
    simp only [walk]
    split
    · next h => exact h
    · exact hlen _

omit hlen in
theorem walk_fst_succ (m : Nat) :
    (walk len (m + 1)).1 = (walk len m).1 ∨ (walk len (m + 1)).1 = (walk len m).1 + 1 := by
  simp only [walk]
  split
  · exact .inl rfl
  · exact .inr rfl

omit hlen in
theorem walk_fst_le : ∀ m, (walk len m).1 ≤ m := by
  intro m
  induction m with
  | zero => exact Nat.le_refl 0
  | succ m ih => rcases walk_fst_succ (len := len) m with h | h <;> omega

omit hlen in
theorem walk_mono {m m' : Nat} (h : m ≤ m') : (walk len m).1 ≤ (walk len m').1 := by
  induction m' with
  | zero =>
    have : m = 0 := by omega
    subst this; exact Nat.le_refl _
  | succ m' ih =>
    by_cases hm : m ≤ m'
    · have := ih hm
      rcases walk_fst_succ (len := len) m' with h | h <;> omega
    · have : m = m' + 1 := by omega
      subst this; exact Nat.le_refl _

omit hlen in
theorem walk_add {m k : Nat} (hm : walk len m = (k, 0)) :
    ∀ d, d < len k → walk len (m + d) = (k, d) := by
  intro d
  induction d with
  | zero => intro; exact hm
  | succ d ih =>
    intro hd
    have h := ih (by omega)
    show walk len (m + d + 1) = _
    simp only [walk, h]
    split
    · rfl
    · omega

theorem walk_next {m k : Nat} (hm : walk len m = (k, 0)) :
    walk len (m + len k) = (k + 1, 0) := by
  have h := walk_add hm (len k - 1) (by have := hlen k; omega)
  have e : m + len k = m + (len k - 1) + 1 := by have := hlen k; omega
  rw [e]
  simp only [walk, h]
  split
  · omega
  · rfl

theorem walk_hits : ∀ k, ∃ m, walk len m = (k, 0) := by
  intro k
  induction k with
  | zero => exact ⟨0, rfl⟩
  | succ k ih =>
    obtain ⟨m, hm⟩ := ih
    exact ⟨m + len k, walk_next hlen hm⟩

/-- The glued sequence steps inside the segment it is in. -/
theorem walk_seq_succ {σ : Type} (g : Nat → Nat → σ) (hg : ∀ k, g k (len k) = g (k + 1) 0)
    (m : Nat) :
    g (walk len (m + 1)).1 (walk len (m + 1)).2 = g (walk len m).1 ((walk len m).2 + 1) := by
  have hl := walk_lt hlen m
  simp only [walk]
  split
  · rfl
  · next h =>
    have : (walk len m).2 + 1 = len (walk len m).1 := by omega
    rw [this, hg]

end Walk

/-- A path, as a function on `0 … n`. -/
theorem LTS.PathWithin.toFun {σ α : Type} {M : LTS σ α} {inv : σ → Prop} {a b : σ}
    (p : M.PathWithin inv a b) :
    ∃ (n : Nat) (g : Nat → σ), g 0 = a ∧ g n = b ∧ (∀ i, i < n → M.Next (g i) (g (i + 1))) ∧
      ∀ i, i ≤ n → inv (g i) := by
  induction p with
  | @refl s h => exact ⟨0, fun _ => s, rfl, rfl, fun i hi => absurd hi (by omega),
      fun _ _ => h⟩
  | @cons s u t hs hn _ ih =>
    obtain ⟨n, g, h0, hn', hstep, hinv⟩ := ih
    refine ⟨n + 1, fun | 0 => s | i + 1 => g i, rfl, hn', fun i hi => ?_, fun i hi => ?_⟩
    · cases i with
      | zero => show M.Next s (g 0); rw [h0]; exact hn
      | succ i => exact hstep i (by omega)
    · cases i with
      | zero => exact hs
      | succ i => exact hinv i (by omega)

/-! ## The segments of `C` behind the steps of `R` -/

variable {T S I O Blk Ta Sa XSa Ra Φ : Type}
  {C : LTS (T × S) (Ta ⊕ Sa)} {X : LTS (S × I) (XSa ⊕ Ra)} {obs : S → O}
  {inp : T → I} {B : S × I → Blk} {rep : Blk → S × I}

/-- A segment of `C` from `p` behind the step of `R` from `img p` to `q'`: `n ≥ 1` steps or
stutters, inside `img p` before its last state, ending at a reachable state whose image is
`q'`, and whose last step takes every fair instance the step of `R` takes. -/
structure SegOK (C : LTS (T × S) (Ta ⊕ Sa)) (R : LTS (T × Blk) (Ta ⊕ Unit)) (inp : T → I)
    (B : S × I → Blk) (inst : Φ → Ta → Prop) (p : T × S) (q' : T × Blk) (n : Nat)
    (g : Nat → T × S) : Prop where
  pos : 0 < n
  start : g 0 = p
  stop : img inp B (g n) = q'
  reach_end : C.Reach (g n)
  steps : ∀ i, i < n → g (i + 1) = g i ∨ C.Next (g i) (g (i + 1))
  inside : ∀ i, i < n → img inp B (g i) = img inp B p
  takes : ∀ f, R.Takes (onT inst) f (img inp B p) q' → C.Takes (onT inst) f (g (n - 1)) (g n)

theorem seg_exists (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (inst : Φ → Ta → Prop) {p : T × S} {q' : T × Blk}
    (hp : C.Reach p)
    (hq : q' = img inp B p ∨ (reduced C X inp B rep).Next (img inp B p) q') :
    ∃ n g, SegOK C (reduced C X inp B rep) inp B inst p q' n g := by
  have hrel : rel C inp B p (img inp B p) := ⟨hp, rfl, rfl⟩
  rcases hq with rfl | hn
  · exact ⟨1, fun _ => p, by omega, rfl, rfl, hp, fun _ _ => .inl rfl, fun _ _ => rfl,
      fun _ ht => absurd rfl ht.1⟩
  · by_cases hT : ∃ a, (reduced C X inp B rep).step (img inp B p) (.inl a) q'
    · obtain ⟨a, ha⟩ := hT
      obtain ⟨hsC, hr'⟩ := t_step_back h5 h6 hE hX hrel ha
      refine ⟨1, fun | 0 => p | _ + 1 => (q'.1, p.2), by omega, rfl, (rel_img hr').symm,
        hr'.1, fun i hi => ?_, fun i hi => ?_, fun f ht => ?_⟩
      · have : i = 0 := by omega
        subst this; exact .inr ⟨_, hsC⟩
      · have : i = 0 := by omega
        subst this; rfl
      · obtain ⟨hne, l, hl, hs⟩ := ht
        cases l with
        | inr _ => exact hl.elim
        | inl a' =>
          obtain ⟨hsC', hr''⟩ := t_step_back h5 h6 hE hX hrel hs
          refine ⟨fun he => hne ?_, .inl a', hl, hsC'⟩
          rw [rel_img hr'']
          exact congrArg (img inp B) he
    · obtain ⟨l, hs⟩ := hn
      cases l with
      | inl a => exact absurd ⟨a, hs⟩ hT
      | inr u =>
        cases u
        obtain ⟨u, u', hpath, hmn, hr'⟩ := class_move_lifts h5 h6.toBy hE hX hrel hs
        obtain ⟨n, g, h0, hgn, hstep, hinv⟩ := hpath.toFun
        have hun : C.Next u u' := let ⟨a, _, ha, _⟩ := hmn; ⟨.inr a, ha⟩
        refine ⟨n + 1, fun i => if i ≤ n then g i else u', by omega, ?_, ?_,
          by simp only [show ¬ n + 1 ≤ n by omega, ↓reduceIte]; exact hr'.1,
          fun i hi => ?_, fun i hi => ?_, fun f ht => ?_⟩
        · simp only [Nat.zero_le, ↓reduceIte, h0]
        · simp only [show ¬ n + 1 ≤ n by omega, ↓reduceIte]; exact (rel_img hr').symm
        · refine .inr ?_
          by_cases hin : i + 1 ≤ n
          · simp only [hin, show i ≤ n by omega, ↓reduceIte]
            obtain ⟨a, _, ha, _⟩ := hstep i (by omega)
            exact ⟨.inr a, ha⟩
          · have : i = n := by omega
            subst this
            simp only [Nat.le_refl, ↓reduceIte, hin, hgn]
            exact hun
        · simp only [show i ≤ n by omega, ↓reduceIte]
          exact (rel_img (hinv i (by omega))).symm
        · obtain ⟨_, l, hl, hs'⟩ := ht
          cases l with
          | inr _ => exact hl.elim
          | inl a => exact absurd ⟨a, hs'⟩ hT

/-! ## Consequence 4 -/

/-- `R ⊨ P ⇒ C ⊨ P` under `Plain`. -/
theorem a8_plain_sound (h5 : Split C obs) (h6 : Lemma6Fwd C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (inst : Φ → Ta → Prop) (trig goal : T × O → Prop) :
    (reduced C X inp B rep).LiveHolds (fun ρ => (reduced C X inp B rep).FairP (onT inst) ρ)
        (fun q => trig (obsR obs rep q)) (fun q => goal (obsR obs rep q)) →
      C.LiveHolds (fun σ => C.FairP (onT inst) σ)
        (fun p => trig (obsC obs p)) (fun p => goal (obsC obs p)) := by
  intro hR σ hb hf hv
  have hobs : ∀ n, obsC obs (σ n) = obsR obs rep (img inp B (σ n)) := fun n =>
    rel_preserves_obs h5 h6 hX (σ n) (img inp B (σ n)) ⟨hb.reach n, rfl, rfl⟩
  refine hR (fun n => img inp B (σ n)) ⟨⟨(σ 0).2, hb.1, rfl⟩, fun n => ?_⟩ ?_ ?_
  · rcases hb.2 n with he | hn
    · exact .inl (by simp only [he])
    · exact image_step h5 h6 hX (hb.reach n) hn
  · intro f ⟨N, hN⟩ N'
    obtain ⟨m, hm, ht⟩ :=
      hf f ⟨N, fun m hm => enabled_back h5 h6 hE hX inst f (hb.reach m) (hN m hm)⟩ N'
    exact ⟨m, hm, takes_fwd h5 h6 hX inst f (hb.reach m) ht⟩
  · obtain ⟨i, hti, hgi⟩ := hv
    refine ⟨i, ?_, fun j hj => ?_⟩
    · show trig (obsR obs rep (img inp B (σ i))); rw [← hobs]; exact hti
    · show ¬ goal (obsR obs rep (img inp B (σ j))); rw [← hobs]; exact hgi j hj

/-- The chain of segment starts, from a choice of segments `F`. -/
def segChain {σ : Type} (F : Nat → σ → Nat × (Nat → σ)) (p₀ : σ) : Nat → σ
  | 0 => p₀
  | k + 1 => (F k (segChain F p₀ k)).2 (F k (segChain F p₀ k)).1

/-- `C ⊨ P ⇒ R ⊨ P` under `Plain`: a fair violating behaviour of `R` lifts to one of `C`. -/
theorem a8_plain_complete (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (inst : Φ → Ta → Prop) (trig goal : T × O → Prop) :
    C.LiveHolds (fun σ => C.FairP (onT inst) σ)
        (fun p => trig (obsC obs p)) (fun p => goal (obsC obs p)) →
      (reduced C X inp B rep).LiveHolds (fun ρ => (reduced C X inp B rep).FairP (onT inst) ρ)
        (fun q => trig (obsR obs rep q)) (fun q => goal (obsR obs rep q)) := by
  intro hC ρ hb hf hv
  let R := reduced C X inp B rep
  -- A segment for every step of `ρ` from every state whose image is where `ρ` is.
  have hseg : ∀ k p, ∃ ng : Nat × (Nat → T × S),
      C.Reach p ∧ img inp B p = ρ k → SegOK C R inp B inst p (ρ (k + 1)) ng.1 ng.2 := by
    intro k p
    by_cases hp : C.Reach p ∧ img inp B p = ρ k
    · have hq : ρ (k + 1) = img inp B p ∨ R.Next (img inp B p) (ρ (k + 1)) := by
        rw [hp.2]; exact hb.2 k
      obtain ⟨n, g, hg⟩ := seg_exists h5 h6 hE hX inst hp.1 hq
      exact ⟨(n, g), fun _ => hg⟩
    · exact ⟨(1, fun _ => p), fun h => absurd h hp⟩
  let F : Nat → T × S → Nat × (Nat → T × S) := fun k p => Classical.choose (hseg k p)
  have hF : ∀ k p, C.Reach p ∧ img inp B p = ρ k → SegOK C R inp B inst p (ρ (k + 1))
      (F k p).1 (F k p).2 := fun k p => Classical.choose_spec (hseg k p)
  obtain ⟨s₀, hs₀, hc₀⟩ := hb.1
  let p₀ : T × S := ((ρ 0).1, s₀)
  have hp₀ : img inp B p₀ = ρ 0 := by
    show ((ρ 0).1, B (s₀, inp (ρ 0).1)) = ρ 0
    rw [← hc₀]
  let ch := segChain F p₀
  have hch : ∀ k, C.Reach (ch k) ∧ img inp B (ch k) = ρ k := by
    intro k
    induction k with
    | zero => exact ⟨Reach.init hs₀, hp₀⟩
    | succ k ih =>
      have h := hF k (ch k) ih
      exact ⟨h.reach_end, h.stop⟩
  have hok : ∀ k, SegOK C R inp B inst (ch k) (ρ (k + 1)) (F k (ch k)).1 (F k (ch k)).2 :=
    fun k => hF k (ch k) (hch k)
  let len : Nat → Nat := fun k => (F k (ch k)).1
  let g : Nat → Nat → T × S := fun k => (F k (ch k)).2
  have hlen : ∀ k, 0 < len k := fun k => (hok k).pos
  have hg : ∀ k, g k (len k) = g (k + 1) 0 := fun k => ((hok (k + 1)).start).symm
  have hg0 : ∀ k, g k 0 = ch k := fun k => (hok k).start
  let σ : Nat → T × S := fun m => g (walk len m).1 (walk len m).2
  have hσs : ∀ m, σ (m + 1) = g (walk len m).1 ((walk len m).2 + 1) :=
    walk_seq_succ hlen g hg
  have himg : ∀ m, img inp B (σ m) = ρ (walk len m).1 := by
    intro m
    have h := (hok (walk len m).1).inside _ (walk_lt hlen m)
    rw [(hch _).2] at h
    exact h
  have hreach : ∀ m, C.Reach (σ m) := by
    intro m
    induction m with
    | zero => show C.Reach (g 0 0); rw [hg0]; exact (hch 0).1
    | succ m ih =>
      have hs : g (walk len m).1 ((walk len m).2 + 1) = σ m ∨
          C.Next (σ m) (g (walk len m).1 ((walk len m).2 + 1)) :=
        (hok _).steps _ (walk_lt hlen m)
      rw [hσs]
      rcases hs with he | ⟨a, ha⟩
      · rw [he]; exact ih
      · exact Reach.step ih ha
  have hobs : ∀ m, obsC obs (σ m) = obsR obs rep (ρ (walk len m).1) := by
    intro m
    rw [← himg m]
    exact rel_preserves_obs h5 h6.toLemma6Fwd hX _ _ ⟨hreach m, rfl, rfl⟩
  -- `σ` is a behaviour of `C`.
  have hbσ : C.Behaviour σ := by
    refine ⟨?_, fun m => ?_⟩
    · show C.init (g 0 0); rw [hg0]; exact hs₀
    · rw [hσs]; exact (hok _).steps _ (walk_lt hlen m)
  -- The start of every segment is visited, at a time at least its index.
  have hhit : ∀ k, ∃ m, k ≤ m ∧ walk len m = (k, 0) := by
    intro k
    obtain ⟨m, hm⟩ := walk_hits hlen k
    have := walk_fst_le (len := len) m
    rw [hm] at this
    exact ⟨m, this, hm⟩
  refine hC σ hbσ ?_ ?_
  · -- `Fair_P` on `σ`.
    intro f ⟨N, hN⟩ N'
    have henR : EvAlways (fun k => R.Enabled (onT inst) f (ρ k)) := by
      refine ⟨N, fun k hk => ?_⟩
      obtain ⟨m, hkm, hm⟩ := hhit k
      have he : C.Enabled (onT inst) f (σ m) := hN m (by omega)
      have hσm : σ m = ch k := by
        show g (walk len m).1 (walk len m).2 = ch k
        rw [hm]; exact hg0 k
      rw [hσm] at he
      obtain ⟨p', ht⟩ := he
      rw [← (hch k).2]
      exact ⟨_, takes_fwd h5 h6.toLemma6Fwd hX inst f (hch k).1 ht⟩
    obtain ⟨k, hk, ht⟩ := hf f henR N'
    obtain ⟨m, hkm, hm⟩ := hhit k
    have hl := hlen k
    have hw := walk_add hm (len k - 1) (by omega)
    refine ⟨m + (len k - 1), by omega, ?_⟩
    show C.Takes (onT inst) f (σ (m + (len k - 1))) (σ (m + (len k - 1) + 1))
    have h1 : σ (m + (len k - 1)) = g k (len k - 1) := by
      show g (walk len _).1 (walk len _).2 = _; rw [hw]
    have h2 : σ (m + (len k - 1) + 1) = g k (len k) := by
      rw [hσs, hw]; congr 1; omega
    rw [h1, h2]
    exact (hok k).takes f (by rw [(hch k).2]; exact ht)
  · -- `σ` violates the property.
    obtain ⟨i, hti, hgi⟩ := hv
    obtain ⟨m, _, hm⟩ := hhit i
    refine ⟨m, ?_, fun j hj => ?_⟩
    · show trig (obsC obs (σ m)); rw [hobs, hm]; exact hti
    · show ¬ goal (obsC obs (σ j))
      rw [hobs]
      apply hgi
      have := walk_mono (len := len) hj
      rw [hm] at this
      exact this

/-- **Theorem A8, consequence 4**: a liveness property over `T_v ∪ obs` whose fair instances are
`T_a` labels holds on `C` under `Plain` iff it holds on `R` under `Plain`. -/
theorem a8_plain_iff (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (inst : Φ → Ta → Prop) (trig goal : T × O → Prop) :
    C.LiveHolds (fun σ => C.FairP (onT inst) σ)
        (fun p => trig (obsC obs p)) (fun p => goal (obsC obs p)) ↔
      (reduced C X inp B rep).LiveHolds (fun ρ => (reduced C X inp B rep).FairP (onT inst) ρ)
        (fun q => trig (obsR obs rep q)) (fun q => goal (obsR obs rep q)) :=
  ⟨a8_plain_complete h5 h6 hE hX inst trig goal,
   a8_plain_sound h5 h6.toLemma6Fwd hE hX inst trig goal⟩

/-! ## Consequences 2 to 4 on the staged reduced cone -/

/-- Two systems with the same initial states, and the same steps at every state where `good`
holds, have the same verdicts under `Fair_P` when `good` holds on the reachable states of
both. -/
theorem liveHolds_congr {σ α Φ : Type} {M N : LTS σ α} (good : σ → Prop)
    (hinit : ∀ s, M.init s ↔ N.init s) (hstep : ∀ s a t, good s → (M.step s a t ↔ N.step s a t))
    (hgM : ∀ s, M.Reach s → good s) (hgN : ∀ s, N.Reach s → good s)
    (inst : Φ → α → Prop) (trig goal : σ → Prop) :
    M.LiveHolds (fun ρ => M.FairP inst ρ) trig goal ↔
      N.LiveHolds (fun ρ => N.FairP inst ρ) trig goal := by
  have tr : ∀ {M N : LTS σ α}, (∀ s, M.init s ↔ N.init s) →
      (∀ s a t, good s → (M.step s a t ↔ N.step s a t)) → (∀ s, M.Reach s → good s) →
      ∀ ρ, M.Behaviour ρ → M.FairP inst ρ → N.Behaviour ρ ∧ N.FairP inst ρ := by
    intro M N hinit hstep hgM ρ hb hf
    have hg : ∀ n, good (ρ n) := fun n => hgM _ (hb.reach n)
    have htk : ∀ f n, M.Takes inst f (ρ n) (ρ (n + 1)) ↔ N.Takes inst f (ρ n) (ρ (n + 1)) :=
      fun f n => ⟨fun ⟨hne, a, ha, hs⟩ => ⟨hne, a, ha, (hstep _ _ _ (hg n)).1 hs⟩,
        fun ⟨hne, a, ha, hs⟩ => ⟨hne, a, ha, (hstep _ _ _ (hg n)).2 hs⟩⟩
    refine ⟨⟨(hinit _).1 hb.1, fun n => ?_⟩, fun f ⟨N₀, hN⟩ N' => ?_⟩
    · rcases hb.2 n with he | ⟨a, ha⟩
      · exact .inl he
      · exact .inr ⟨a, (hstep _ _ _ (hg n)).1 ha⟩
    · have hen : EvAlways (fun n => M.Enabled inst f (ρ n)) := by
        refine ⟨N₀, fun m hm => ?_⟩
        obtain ⟨t, hne, a, ha, hs⟩ := hN m hm
        exact ⟨t, hne, a, ha, (hstep _ _ _ (hg m)).2 hs⟩
      obtain ⟨m, hm, ht⟩ := hf f hen N'
      exact ⟨m, hm, (htk f m).1 ht⟩
  constructor
  · intro hM ρ hb hf hv
    obtain ⟨hb', hf'⟩ := tr (fun s => (hinit s).symm) (fun s a t h => (hstep s a t h).symm) hgN
      ρ hb hf
    exact hM ρ hb' hf' hv
  · intro hN ρ hb hf hv
    obtain ⟨hb', hf'⟩ := tr hinit hstep hgM ρ hb hf
    exact hN ρ hb' hf' hv

section Staged

variable {rt : Blk → I → Option Blk} {stages : I → I → List I}

/-- Consequence 2 on the staged reduced cone. -/
theorem a8_staged_edges_iff (h5 : Split C obs) (h6 : Lemma6Staged C X inp stages)
    (hE : Extended X) (hX : Stable X obs B rep) (ht : RiseTable X B rep rt) (a : Ta)
    (o o' : T × O) :
    Edges C tLabel (obsC obs) a o o' ↔
      Edges (reducedStaged C X inp B rep rt stages) tLabel (obsR obs rep) a o o' :=
  let h := a8_staged h5 h6 hE hX ht
  edges_iff_of_branchBisim h.1 h.2 staged_init_forward staged_init_backward a o o'

/-- Consequence 3 on the staged reduced cone. -/
theorem a8_staged_stuck_iff (h5 : Split C obs) (h6 : Lemma6Staged C X inp stages)
    (hE : Extended X) (hX : Stable X obs B rep) (ht : RiseTable X B rep rt)
    {ch : T × O → T × O → Prop} (hirr : ∀ o, ¬ ch o o) (o : T × O) :
    Stuck C (obsC obs) ch o ↔ Stuck (reducedStaged C X inp B rep rt stages) (obsR obs rep) ch o :=
  let h := a8_staged h5 h6 hE hX ht
  stuck_iff_of_dbsb hirr h.1.dbsb h.2 staged_init_forward staged_init_backward o

/-- Consequence 4 on the staged reduced cone. -/
theorem a8_staged_plain_iff (h5 : Split C obs) (h6 : Lemma6Staged C X inp stages)
    (hE : Extended X) (hX : Stable X obs B rep) (ht : RiseTable X B rep rt)
    (inst : Φ → Ta → Prop) (trig goal : T × O → Prop) :
    C.LiveHolds (fun σ => C.FairP (onT inst) σ)
        (fun p => trig (obsC obs p)) (fun p => goal (obsC obs p)) ↔
      (reducedStaged C X inp B rep rt stages).LiveHolds
        (fun ρ => (reducedStaged C X inp B rep rt stages).FairP (onT inst) ρ)
        (fun q => trig (obsR obs rep q)) (fun q => goal (obsR obs rep q)) := by
  have h6' := h6.toLemma6
  refine (a8_plain_iff h5 h6' hE hX inst trig goal).trans
    (liveHolds_congr (M := reduced C X inp B rep) (N := reducedStaged C X inp B rep rt stages)
      (fun q => ∃ p, rel C inp B p q) (fun _ => Iff.rfl)
      (fun q l q' ⟨p, hr⟩ => (staged_step_iff h5 h6 hE hX ht l hr).symm) ?_ ?_ _ _ _)
  · intro q hq
    obtain ⟨p, _, hr⟩ := reach_of_stutterSim (branch_backward h5 h6' hE hX).stutterSim
      init_backward q hq
    exact ⟨p, hr⟩
  · intro q hq
    obtain ⟨p, _, hr⟩ := reach_of_stutterSim (a8_staged h5 h6 hE hX ht).1.2.stutterSim
      staged_init_backward q hq
    exact ⟨p, hr⟩

end Staged

end RdraIsh
