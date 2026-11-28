import RdraIsh.Reduction
import RdraIsh.Behaviour

/-!
# `WF_vars(Next)` on a reduced cone (cones.md, Definition 5 and Theorem A15)

The setting is Reduction.lean's: the cone `C` on `T × S`, the extended sub-lifecycle `X`, the
partition `B` with representatives `rep`, and the reduced cone `R = reduced C X inp B rep` on
`T × Blk`. A liveness property `□(trig ⇒ ◇goal)` at one binding reads `T_v ∪ obs`
(`trig goal : T × O → Prop`), and its fair instances `f : Φ` are sets of `T_a` labels
(`inst : Φ → Ta → Prop`; fair actions are `T_a` actions by Lemma 5 (e)). `onT inst` reads the
same instances on the labels of `C` (`Ta ⊕ Sa`) and of `R` (`Ta ⊕ Unit`).

**The run.** `decide_liveness_reduced` searches `R`'s explored graph under
`StopRule::Looping`: a goal-free state may end a counterexample by stopping when no fair
instance is enabled there and either no move leaves it or its class can loop. A counterexample
of the run is a lasso of `R` (prefix and cycle), so the run reports none exactly when no lasso
of `R` that satisfies `Fair_P` and stays forever only at such stops violates the property
(`ReducedRunOk`). Under `Fair_P`, a lasso that stays at `q` forever has no fair instance
enabled at `q` (it would be enabled forever and never taken), so `LoopingStop` keeps only the
second half of the stop rule. That the search over strongly connected components finds a lasso
whenever one exists is N9 (native.md), which is not formalized here.

**Definition 5, case 1.** `SilentCycle X B c`: from a reachable state of block `c` the silent
edges of `X` between two different states of `c` contain a cycle. The hypothesis `hloop`
asks that every such block can loop. `divergence` must establish it (its depth-first search
finds a back edge iff such a cycle exists); for `obs = ∅` `s_cycles` must imply it (case 4).
Cases 2 and 3 (inner classes, an orbit self-edge of `S` up to `H`) belong to the nested case,
which Reduction.lean does not model.

**Theorem A15** (`a15`). Under Theorem A8's hypotheses (Lemma 5 (a)(b), both halves of
Lemma 6, the shape of `X`, (B1)–(B3)), finitely many reachable states of `X` and of `R`, and
`hloop`: if the run finds no counterexample then `C ⊨ P` under `WfNext`. The proof is the one
of cones.md: map a fair violating behaviour of `C` to its image under `~` (`a15_image`), which
is a behaviour of `R` with the same trigger, goal and fair steps; its three cases are

- the image changes infinitely often: nothing to show, the lasso lemma folds it;
- the image settles in `(t, c)` while `C` keeps moving: the moves are silent edges of `X`
  inside `c` that change the state, and on finitely many states they close a cycle
  (`silentCycle_of_divergence`), so `c` can loop;
- `C` stays at `(t, s)` forever: `WF_vars(Next)` says no step leaves `(t, s)`, and a move of
  `R` from `(t, c)` would lift to one (`moves_back`, from the `X ⇒ C` half of Lemma 6 and (B2)).

The converse (a counterexample of the run is one of `C`) is not claimed: a stop at a class that
can loop may be spurious, which is why a candidate prints only after it lifts and replays.
-/

namespace RdraIsh

open LTS

variable {T S I O Blk Ta Sa XSa Ra Φ : Type}

/-- Fair instances, given on `T_a` labels, read on a label set `Ta ⊕ β`. -/
def onT {β : Type} (inst : Φ → Ta → Prop) : Φ → Ta ⊕ β → Prop
  | f, .inl a => inst f a
  | _, .inr _ => False

/-- The image of a state of `C` under `~`: `(t, s) ↦ (t, B(x(t, s)))`. -/
def img (inp : T → I) (B : S × I → Blk) (p : T × S) : T × Blk := (p.1, B (p.2, inp p.1))

/-- A walk of silent edges of `X` between different states, all inside block `c`. -/
inductive SilentWalk (X : LTS (S × I) (XSa ⊕ Ra)) (B : S × I → Blk) (c : Blk) :
    S × I → S × I → Prop
  | refl (x) : SilentWalk X B c x x
  | cons {x y z} : B x = c → B y = c → y ≠ x → (silent X).Next x y → SilentWalk X B c y z →
      SilentWalk X B c x z

theorem SilentWalk.trans {X : LTS (S × I) (XSa ⊕ Ra)} {B : S × I → Blk} {c : Blk}
    {x y z : S × I} (h₁ : SilentWalk X B c x y) (h₂ : SilentWalk X B c y z) :
    SilentWalk X B c x z := by
  induction h₁ with
  | refl => exact h₂
  | cons hx hy hne hn _ ih => exact .cons hx hy hne hn (ih h₂)

/-- Definition 5, case 1: a reachable state of block `c` starts a cycle of silent edges
between different states of `c`. -/
def SilentCycle (X : LTS (S × I) (XSa ⊕ Ra)) (B : S × I → Blk) (c : Blk) : Prop :=
  ∃ x y, X.Reach x ∧ B x = c ∧ B y = c ∧ y ≠ x ∧ (silent X).Next x y ∧ SilentWalk X B c y x

/-- `StopRule::Looping`, the part `Fair_P` does not already give: no move leaves `q`, or its
class can loop. -/
def LoopingStop (R : LTS (T × Blk) (Ta ⊕ Unit)) (canLoop : Blk → Prop) (q : T × Blk) : Prop :=
  ¬ R.Moves q ∨ canLoop q.2

/-- The run on `R` finds no counterexample: no lasso of `R` that satisfies `Fair_P` and stays
forever only at a `LoopingStop` violates the property. -/
def ReducedRunOk (R : LTS (T × Blk) (Ta ⊕ Unit)) (inst : Φ → Ta → Prop)
    (canLoop : Blk → Prop) (trig goal : T × Blk → Prop) : Prop :=
  ∀ ρ, Lasso ρ → R.Behaviour ρ → R.FairP (onT inst) ρ → StopsOnly (LoopingStop R canLoop) ρ →
    ¬ Violates trig goal ρ

/-- A path whose last step leaves its first state starts with a move. -/
theorem LTS.PathWithin.moves {σ α : Type} {M : LTS σ α} {inv : σ → Prop} {s y y' : σ}
    (hp : M.PathWithin inv s y) (hn : M.Next y y') (hne : y' ≠ s) : M.Moves s := by
  induction hp with
  | refl _ => exact ⟨y', hne, hn⟩
  | @cons s u t _ hsu _ ih =>
    by_cases hus : u = s
    · subst hus; exact ih hn hne
    · exact ⟨u, hus, hsu⟩

section Proofs

variable {C : LTS (T × S) (Ta ⊕ Sa)} {X : LTS (S × I) (XSa ⊕ Ra)} {obs : S → O}
  {inp : T → I} {B : S × I → Blk} {rep : Blk → S × I}

theorem rel_img {p : T × S} {q : T × Blk} (h : rel C inp B p q) : q = img inp B p := by
  obtain ⟨q1, q2⟩ := q
  obtain ⟨_, h1, h2⟩ := h
  simp only at h1 h2
  rw [h1, h2]; rfl

/-- Every step of `C` from a reachable state is a step of `R` between the images, or leaves the
image unchanged (Theorem A8, points 2, 3, 5: the matching path of `sim_forward` is empty). -/
theorem image_step (h5 : Split C obs) (h6 : Lemma6Fwd C X inp) (hX : Stable X obs B rep)
    {p p' : T × S} (hp : C.Reach p) (hn : C.Next p p') :
    img inp B p' = img inp B p ∨ (reduced C X inp B rep).Next (img inp B p) (img inp B p') := by
  rcases sim_forward h5 h6 hX p (img inp B p) p' ⟨hp, rfl, rfl⟩ hn with
    h | ⟨y, y', hpath, hnext, hr'⟩
  · exact .inl (rel_img h).symm
  · rw [rel_img hpath.last] at hnext
    rw [rel_img hr'] at hnext
    exact .inr hnext

/-- A `T_a` step of `C` is the same `T_a` step of `R` between the images. -/
theorem t_step_img (h5 : Split C obs) (h6 : Lemma6Fwd C X inp) (hX : Stable X obs B rep)
    {t t' : T} {s s' : S} {a : Ta} (hr : C.Reach (t, s)) (hs : C.step (t, s) (.inl a) (t', s')) :
    (reduced C X inp B rep).step (img inp B (t, s)) (.inl a) (img inp B (t', s')) := by
  have hs' : s' = s := h5.t_keeps_s hs
  subst hs'
  have hx := x_reach h5 h6 (t, s') hr
  obtain ⟨hrr, hrb, hro, _⟩ := rep_facts hX hx
  have hrise := h6.rise hr hs
  obtain ⟨y, hy, hby⟩ := b3_star hX hx hrr hrb.symm hrise
  have hyr : X.Reach y := hy.reach hrr
  have hx' : X.Reach (s', inp t') := hrise.reach hx
  obtain ⟨_, hyi⟩ := hX.b1 hyr hx' hby
  exact ⟨h5.t_reads_obs hro.symm hs, y, hy, hyi, hby⟩

/-- A fair instance taken by a step of `C` is taken by the image step. -/
theorem takes_fwd (h5 : Split C obs) (h6 : Lemma6Fwd C X inp) (hX : Stable X obs B rep)
    (inst : Φ → Ta → Prop) (f : Φ) {p p' : T × S} (hp : C.Reach p)
    (ht : C.Takes (onT inst) f p p') :
    (reduced C X inp B rep).Takes (onT inst) f (img inp B p) (img inp B p') := by
  obtain ⟨t, s⟩ := p
  obtain ⟨t', s'⟩ := p'
  obtain ⟨hne, l, hl, hs⟩ := ht
  cases l with
  | inr _ => exact hl.elim
  | inl a =>
    have hs' : s' = s := h5.t_keeps_s hs
    refine ⟨fun he => hne ?_, .inl a, hl, t_step_img h5 h6 hX hp hs⟩
    have : t' = t := congrArg Prod.fst he
    rw [this, hs']

/-- A fair instance enabled at the image of a reachable state of `C` is enabled there. -/
theorem enabled_back (h5 : Split C obs) (h6 : Lemma6Fwd C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (inst : Φ → Ta → Prop) (f : Φ) {p : T × S} (hp : C.Reach p)
    (he : (reduced C X inp B rep).Enabled (onT inst) f (img inp B p)) :
    C.Enabled (onT inst) f p := by
  obtain ⟨t, s⟩ := p
  obtain ⟨⟨t', c'⟩, hne, l, hl, hs⟩ := he
  cases l with
  | inr _ => exact hl.elim
  | inl a =>
    obtain ⟨hstep, y, hy, hyi, hyb⟩ := hs
    have hx := x_reach h5 h6 (t, s) hp
    obtain ⟨_, hrb, hro, hri⟩ := rep_facts hX hx
    simp only [img] at hstep hy hyi hyb hne hrb hro hri
    have hsC : C.step (t, s) (.inl a) (t', s) := h5.t_reads_obs hro hstep
    refine ⟨(t', s), fun heq => hne ?_, .inl a, hl, hsC⟩
    have htt : t' = t := congrArg Prod.fst heq
    subst htt
    have hy1 := hy.keeps_s hE
    have hyr : y = rep (B (s, inp t')) := Prod.ext hy1 (hyi.trans hri.symm)
    rw [← hyb, hyr, hrb]

/-- A move of `R` from the image of a reachable state of `C` lifts to a move of `C` from it
(the `X ⇒ C` half of Lemma 6, through `sim_backward`). -/
theorem moves_back (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) {p : T × S} (hp : C.Reach p)
    (hm : (reduced C X inp B rep).Moves (img inp B p)) : C.Moves p := by
  obtain ⟨q', hne, hn⟩ := hm
  rcases sim_backward h5 h6 hE hX (img inp B p) p q' ⟨hp, rfl, rfl⟩ hn with
    h | ⟨y, y', hpath, hnext, hr'⟩
  · exact absurd (rel_img h) hne
  · refine hpath.moves hnext fun hy => hne ?_
    rw [rel_img hr', hy]

/-- A step of `C` that keeps `T` and changes `S` is a silent edge of `X` between different
states (Lemma 5 (b) and the `S_a` conjunct of Lemma 6). -/
theorem silent_of_step (h5 : Split C obs) (h6 : Lemma6Fwd C X inp) {t : T} {s s' : S}
    (hr : C.Reach (t, s)) (hn : C.Next (t, s) (t, s')) (hne : s' ≠ s) :
    (silent X).Next (s, inp t) (s', inp t) := by
  obtain ⟨l, hl⟩ := hn
  cases l with
  | inl a => exact absurd (h5.t_keeps_s hl) hne
  | inr a => exact h6.s_step hr hl

/-- **Divergence inside one image** (cones.md, A15, the second case, (i)): if a behaviour of
`C` keeps its image at `q` from `n` on and still changes state infinitely often, the silent
edges of `X` inside `q.2` contain a cycle. Needs the reachable states of `X` in a list. -/
theorem silentCycle_of_divergence (h5 : Split C obs) (h6 : Lemma6Fwd C X inp)
    (LX : List (S × I)) (hLX : ∀ x, X.Reach x → x ∈ LX) {σ : Nat → T × S}
    (hb : C.Behaviour σ) (n : Nat) (q : T × Blk)
    (hc : ∀ m, n ≤ m → img inp B (σ m) = q) (hinf : InfOften fun m => σ (m + 1) ≠ σ m) :
    SilentCycle X B q.2 := by
  let xs : Nat → S × I := fun k => ((σ (n + k)).2, inp q.1)
  have ht : ∀ k, (σ (n + k)).1 = q.1 := fun k => congrArg Prod.fst (hc (n + k) (by omega))
  have heta : ∀ k, σ (n + k) = (q.1, (σ (n + k)).2) := fun k => Prod.ext (ht k) rfl
  have hblk : ∀ k, B (xs k) = q.2 := by
    intro k
    have := congrArg Prod.snd (hc (n + k) (by omega))
    simp only [img, ht k] at this
    exact this
  have hreach : ∀ k, X.Reach (xs k) := by
    intro k
    have := x_reach h5 h6 _ (hb.reach (n + k))
    simp only [ht k] at this
    exact this
  have hstep : ∀ k, xs (k + 1) = xs k ∨
      (xs (k + 1) ≠ xs k ∧ (silent X).Next (xs k) (xs (k + 1))) := by
    intro k
    by_cases heq : (σ (n + (k + 1))).2 = (σ (n + k)).2
    · exact .inl (by simp only [xs, heq])
    · have hne : xs (k + 1) ≠ xs k := fun he => heq (congrArg Prod.fst he)
      refine .inr ⟨hne, ?_⟩
      rcases hb.2 (n + k) with he | hn
      · exact absurd (by rw [show n + (k + 1) = n + k + 1 from rfl, he]) heq
      · have hr := hb.reach (n + k)
        rw [show n + k + 1 = n + (k + 1) from rfl, heta (k + 1), heta k] at hn
        rw [heta k] at hr
        exact silent_of_step h5 h6 hr hn heq
  have hinfx : InfOften fun k => xs (k + 1) ≠ xs k := by
    intro N
    obtain ⟨m, hm, hne⟩ := hinf (n + N)
    refine ⟨m - n, by omega, fun he => hne ?_⟩
    have e1 : n + (m - n) = m := by omega
    have e2 : n + (m - n + 1) = m + 1 := by omega
    have h2 : (σ (m + 1)).2 = (σ m).2 := by
      have := congrArg Prod.fst he
      simp only [xs, e1, e2] at this
      exact this
    have h1 : (σ (m + 1)).1 = (σ m).1 := by
      have a := ht (m - n + 1)
      have b := ht (m - n)
      rw [e2] at a; rw [e1] at b
      rw [a, b]
    exact Prod.ext h1 h2
  have hwalk : ∀ d a, SilentWalk X B q.2 (xs a) (xs (a + d)) := by
    intro d
    induction d with
    | zero => intro a; exact .refl _
    | succ d ih =>
      intro a
      have e : a + (d + 1) = a + 1 + d := by omega
      rw [e]
      rcases hstep a with he | ⟨hne, hn⟩
      · rw [← he]; exact ih (a + 1)
      · exact .cons (hblk a) (hblk (a + 1)) hne hn (ih (a + 1))
  obtain ⟨n₀, m, _, hnm, hmo, hcov⟩ := recurrent_segment xs LX (fun k => hLX _ (hreach k)) 0
  obtain ⟨k, hk, hkne⟩ := hinfx n₀
  obtain ⟨j, hj1, hj2, hj3, hj4⟩ := hcov k hk
  have hjne : xs (j + 1) ≠ xs j := by rw [hj3, hj4]; exact hkne
  have hjn : (silent X).Next (xs j) (xs (j + 1)) := by
    rcases hstep j with he | ⟨_, hn⟩
    · exact absurd he hjne
    · exact hn
  have w1 : SilentWalk X B q.2 (xs (j + 1)) (xs m) := by
    have := hwalk (m - (j + 1)) (j + 1)
    rwa [show j + 1 + (m - (j + 1)) = m by omega] at this
  have w2 : SilentWalk X B q.2 (xs n₀) (xs j) := by
    have := hwalk (j - n₀) n₀
    rwa [show n₀ + (j - n₀) = j by omega] at this
  rw [hmo] at w1
  exact ⟨xs j, xs (j + 1), hreach j, hblk j, hblk (j + 1), hjne, hjn, w1.trans w2⟩

/-- **The image of a fair violating behaviour** (the body of Theorem A15's proof). Under
Theorem A8's hypotheses, finitely many reachable states of `X` and `hloop`, the image under
`~` of a behaviour of `C` that satisfies `Fair_P ∧ WF_vars(Next)` and violates the property is
a behaviour of `R` that satisfies `Fair_P`, stays forever only at a `LoopingStop`, and violates
the property. -/
theorem a15_image (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (LX : List (S × I)) (hLX : ∀ x, X.Reach x → x ∈ LX)
    (canLoop : Blk → Prop) (hloop : ∀ c, SilentCycle X B c → canLoop c)
    (inst : Φ → Ta → Prop) (trig goal : T × O → Prop) {σ : Nat → T × S}
    (hb : C.Behaviour σ) (hf : C.FairP (onT inst) σ) (hwf : C.WFNext σ)
    (hv : Violates (fun p => trig (obsC obs p)) (fun p => goal (obsC obs p)) σ) :
    (reduced C X inp B rep).Behaviour (fun n => img inp B (σ n)) ∧
      (reduced C X inp B rep).FairP (onT inst) (fun n => img inp B (σ n)) ∧
      StopsOnly (LoopingStop (reduced C X inp B rep) canLoop) (fun n => img inp B (σ n)) ∧
      Violates (fun q => trig (obsR obs rep q)) (fun q => goal (obsR obs rep q))
        (fun n => img inp B (σ n)) := by
  have h6f := h6.toLemma6Fwd
  have hobs : ∀ n, obsC obs (σ n) = obsR obs rep (img inp B (σ n)) := fun n =>
    rel_preserves_obs h5 h6f hX (σ n) (img inp B (σ n)) ⟨hb.reach n, rfl, rfl⟩
  refine ⟨⟨⟨(σ 0).2, hb.1, rfl⟩, fun n => ?_⟩, ?_, ?_, ?_⟩
  · rcases hb.2 n with he | hn
    · exact .inl (by simp only [he])
    · exact image_step h5 h6f hX (hb.reach n) hn
  · intro f ⟨N, hN⟩ N'
    obtain ⟨m, hm, ht⟩ :=
      hf f ⟨N, fun m hm => enabled_back h5 h6f hE hX inst f (hb.reach m) (hN m hm)⟩ N'
    exact ⟨m, hm, takes_fwd h5 h6f hX inst f (hb.reach m) ht⟩
  · intro N q hc
    by_cases hst : ∃ n', ∀ m, n' ≤ m → σ (m + 1) = σ m
    · obtain ⟨n', hn'⟩ := hst
      have hconst := const_of_stutter (n := max N n') fun m hm => hn' m (by omega)
      have hstop := (C.wfNext_iff σ).mp hwf (max N n') _ hconst
      have hq : img inp B (σ (max N n')) = q := hc _ (by omega)
      refine .inl fun hm => hstop (moves_back h5 h6 hE hX (hb.reach _) ?_)
      rw [hq]; exact hm
    · have hinf : InfOften fun m => σ (m + 1) ≠ σ m := Classical.byContradiction fun h =>
        hst (let ⟨n', hn'⟩ := not_infOften h
             ⟨n', fun m hm => Classical.byContradiction (hn' m hm)⟩)
      exact .inr (hloop _ (silentCycle_of_divergence h5 h6f LX hLX hb N q hc hinf))
  · obtain ⟨i, hti, hgi⟩ := hv
    refine ⟨i, ?_, fun j hj => ?_⟩
    · show trig (obsR obs rep (img inp B (σ i))); rw [← hobs]; exact hti
    · show ¬ goal (obsR obs rep (img inp B (σ j))); rw [← hobs]; exact hgi j hj

/-- **Theorem A15, over behaviours.** Under the hypotheses of `a15_image`: if no behaviour of
`R` that satisfies `Fair_P` and stays forever only at a `LoopingStop` violates the property,
then `C ⊨ P` under `WfNext`. -/
theorem a15_behaviour (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (LX : List (S × I)) (hLX : ∀ x, X.Reach x → x ∈ LX)
    (canLoop : Blk → Prop) (hloop : ∀ c, SilentCycle X B c → canLoop c)
    (inst : Φ → Ta → Prop) (trig goal : T × O → Prop)
    (hrun : ∀ ρ, (reduced C X inp B rep).Behaviour ρ →
      (reduced C X inp B rep).FairP (onT inst) ρ →
      StopsOnly (LoopingStop (reduced C X inp B rep) canLoop) ρ →
      ¬ Violates (fun q => trig (obsR obs rep q)) (fun q => goal (obsR obs rep q)) ρ) :
    C.LiveHolds (fun σ => C.FairP (onT inst) σ ∧ C.WFNext σ)
      (fun p => trig (obsC obs p)) (fun p => goal (obsC obs p)) := by
  intro σ hb ⟨hf, hwf⟩ hv
  obtain ⟨h1, h2, h3, h4⟩ := a15_image h5 h6 hE hX LX hLX canLoop hloop inst trig goal hb hf hwf hv
  exact hrun _ h1 h2 h3 h4

/-- **Theorem A15** (cones.md): assume Theorem A8's hypotheses (Lemma 5 (a)(b), Lemma 6 both
ways, the shape of `X`, (B1)–(B3)), that the reachable states of `X` and of `R` are finitely
many (the lists `LX`, `LR`), and that every block with a silent cycle can loop (`hloop`,
Definition 5 case 1). If the run on `R` under `StopRule::Looping` finds no counterexample
(`ReducedRunOk`: no fair lasso of `R` that stops only at a `LoopingStop` violates `P`), then
`C ⊨ P` under `WfNext` (`Fair_P ∧ WF_vars(Next)`). -/
theorem a15 (h5 : Split C obs) (h6 : Lemma6 C X inp) (hE : Extended X)
    (hX : Stable X obs B rep) (LX : List (S × I)) (hLX : ∀ x, X.Reach x → x ∈ LX)
    (LR : List (T × Blk)) (hLR : ∀ q, (reduced C X inp B rep).Reach q → q ∈ LR)
    (canLoop : Blk → Prop) (hloop : ∀ c, SilentCycle X B c → canLoop c)
    (inst : Φ → Ta → Prop) (trig goal : T × O → Prop)
    (hrun : ReducedRunOk (reduced C X inp B rep) inst canLoop
      (fun q => trig (obsR obs rep q)) (fun q => goal (obsR obs rep q))) :
    C.LiveHolds (fun σ => C.FairP (onT inst) σ ∧ C.WFNext σ)
      (fun p => trig (obsC obs p)) (fun p => goal (obsC obs p)) := by
  refine a15_behaviour h5 h6 hE hX LX hLX canLoop hloop inst trig goal fun ρ hb hf hs hv => ?_
  obtain ⟨ρ', hl, hb', hf', hs', hv'⟩ :=
    (reduced C X inp B rep).lasso_of_behaviour LR hLR (onT inst) _ _ _ ρ hb hf hs hv
  exact hrun ρ' hl hb' hf' hs' hv'

end Proofs

end RdraIsh
