import RdraIsh.Sweep
import RdraIsh.PartitionCert

/-!
# The check of a partition certificate is sound

A record (`PartitionCert.lean`) names the states of a run of `X` by number. Given the states
`st i` the numbers stand for and the numbers `idx x` of the states, `Data.run` is the run as
`Sweep.ExplRun` reads it, labelled by the `obs` value and the input bytes.

What is read from the dump code, not checked (`Graph`, `Faithful`):

- `Graph`: the numbering is a bijection between `0 … n-1` and a set of states that holds
  every reachable state, the silent edges and the rises are exactly `X`'s from reachable
  states, and the listed initial states are initial. That every numbered state is reachable
  is not assumed: `checkReach` gives it (`reach_of_check`);
- `Faithful`: equal numbers in `ov`, `sv`, `iv` mean equal `obs`, `S` and input bytes.

What is proved:

- `complete_of_check`: an accepted record's run is `Sweep.Complete`;
- `cert_of_validate`: for any witness `validate` accepts, the partition passes
  `Sweep.Cert`; (B2) goes through `Sweep.b2_of_marking`, with the marking of a splitter
  `sp` the states whose class lists `sp` as an exit;
- `stable_of_check`: an accepted record's partition is `Stable`, with the dumped
  representatives;
- `extended_of_check`: the reachable part of `X` (`onReach`, the steps from reachable
  states, which has the same reachable states, `reach_onReach`) has Definition 4's shape,
  `Extended`.
-/

namespace RdraIsh.PCert

open LTS Sweep

theorem all_at {n : Nat} {f : (i : Nat) → i < n → Bool} (h : Nat.all n f = true) :
    ∀ i (hi : i < n), f i hi = true := by
  induction n with
  | zero => intro i hi; exact absurd hi (Nat.not_lt_zero _)
  | succ n ih =>
    simp only [Nat.all, Bool.and_eq_true] at h
    intro i hi
    rcases Nat.lt_succ_iff_lt_or_eq.mp hi with h' | rfl
    · exact ih h.1 i h'
    · exact h.2

theorem mem_silentList {d : Data} {p t : Nat} :
    (p, t) ∈ d.silentList ↔ ∃ e, e < d.ne ∧ g d.ssrc e = p ∧ g d.sdst e = t := by
  simp [Data.silentList]

theorem mem_riseList {d : Data} {p t : Nat} :
    (p, t) ∈ d.riseList ↔ ∃ e, e < d.nr ∧ g d.rsrc e = p ∧ g d.rdst e = t := by
  simp [Data.riseList]

/-- The facts of `wellFormed` the proofs use. -/
structure WF (d : Data) : Prop where
  blk : ∀ {i}, i < d.n → d.blkOf i < d.nb
  rep : ∀ {c}, c < d.nb → d.repOf c < d.n
  init : ∀ {k}, k < d.init.size → g d.init k < d.n
  silent : ∀ {e}, e < d.ne → g d.ssrc e < d.n ∧ g d.sdst e < d.n
  rise : ∀ {e}, e < d.nr → g d.rsrc e < d.n ∧ g d.rdst e < d.n

theorem wf_of {d : Data} (h : d.wellFormed = true) : WF d := by
  simp only [Data.wellFormed, Bool.and_eq_true] at h
  obtain ⟨⟨⟨⟨⟨_, hb⟩, hr⟩, hi⟩, hs⟩, hu⟩ := h
  refine ⟨fun hi' => ?_, fun hc => ?_, fun hk => ?_, fun he => ?_, fun he => ?_⟩
  · simpa using all_at hb _ hi'
  · simpa using all_at hr _ hc
  · simpa using all_at hi _ hk
  · simpa using all_at hs _ he
  · simpa using all_at hu _ he

/-- The parts of an accepted record, on its own witness. -/
theorem parts {d : Data} (h : d.check = true) :
    WF d ∧ d.checkRep = true ∧ d.checkB1 = true ∧ d.checkB2 d.witness = true ∧
      d.checkB3 d.witness = true ∧ d.checkSilentInp = true ∧ d.checkRiseS = true ∧
      d.checkReach d.witness = true := by
  simp only [Data.check, Data.validate, Bool.and_eq_true] at h
  obtain ⟨hw, ⟨⟨⟨⟨⟨⟨h1, h2⟩, h3⟩, h4⟩, h5⟩, h6⟩, h7⟩⟩ := h
  exact ⟨wf_of hw, h1, h2, h3, h4, h5, h6, h7⟩

/-! ## (B2): the exits of a class -/

section B2

variable {d : Data} {w : Witness}

/-- The marking of the splitter `sp`: the states whose class lists `sp` as an exit. -/
def Exit (d : Data) (w : Witness) (sp p : Nat) : Prop :=
  p < d.n ∧ ∃ x, g w.eoff (d.blkOf p) ≤ x ∧ x < g w.eoff (d.blkOf p + 1) ∧ g w.elist x = sp

/-- A pair that holds is a marked state, by induction on its rank. -/
theorem marked_of_pair (hp : d.checkPairs w = true) :
    ∀ r p j, p < d.n → j < Data.segLen w p → g w.prank (g w.poff p + j) < r →
      Marked d.silentList d.blkOf (d.exitAt w p j) p := by
  intro r
  induction r with
  | zero => intro _ _ _ _ h; exact absurd h (Nat.not_lt_zero _)
  | succ r ih =>
    intro p j hpn hj hr
    have hpq := all_at hp p hpn
    simp only [Bool.and_eq_true] at hpq
    have hok := all_at hpq.2 j hj
    simp only [Data.pairOk, Bool.and_eq_true, Bool.or_eq_true, beq_iff_eq, bne_iff_ne, ne_eq,
      decide_eq_true_eq] at hok
    obtain ⟨⟨⟨he, hsrc⟩, hne⟩, hcase⟩ := hok
    have hmem : (p, g d.sdst (g w.pvia (g w.poff p + j))) ∈ d.silentList :=
      mem_silentList.mpr ⟨_, he, hsrc, rfl⟩
    rcases hcase with ⟨ht, hp'⟩ | ⟨⟨⟨hb, htn⟩, hjt⟩, hlt⟩
    · exact .front hmem (Ne.symm hne) ht hp'
    · have hm := ih _ j htn hjt (by omega)
      have hx : d.exitAt w (g d.sdst (g w.pvia (g w.poff p + j))) j = d.exitAt w p j := by
        simp only [Data.exitAt, hb]
      rw [hx] at hm
      exact .back hmem (Ne.symm hne) hb.symm hm

theorem marked_of_exit (hp : d.checkPairs w = true) {sp p : Nat} (h : Exit d w sp p) :
    Marked d.silentList d.blkOf sp p := by
  obtain ⟨hpn, x, hlo, hhi, hx⟩ := h
  have hpq := all_at hp p hpn
  simp only [Bool.and_eq_true, beq_iff_eq] at hpq
  have hj : x - g w.eoff (d.blkOf p) < Data.segLen w p := by rw [hpq.1]; omega
  have hm := marked_of_pair hp _ p _ hpn hj (Nat.lt_succ_self _)
  have hea : d.exitAt w p (x - g w.eoff (d.blkOf p)) = sp := by
    simp only [Data.exitAt]; rw [Nat.add_sub_of_le hlo]; exact hx
  rwa [hea] at hm

theorem exit_of_edge (hw : WF d) (hc : d.checkClosed w = true) {sp p t : Nat}
    (he : (p, t) ∈ d.silentList) (hne : p ≠ t) (ht : d.blkOf t = sp) (hp : d.blkOf p ≠ sp) :
    Exit d w sp p := by
  obtain ⟨e, hen, hs, hd⟩ := mem_silentList.mp he
  have := all_at hc e hen
  simp only [hs, hd, Bool.or_eq_true, beq_iff_eq, Bool.and_eq_true, decide_eq_true_eq] at this
  refine ⟨hs ▸ (hw.silent hen).1, g w.eix e, ?_⟩
  rcases this with (h | h) | ⟨⟨h1, h2⟩, h3⟩
  · exact absurd h hne
  · exact absurd (h.trans ht) hp
  · exact ⟨h1, h2, h3.trans ht⟩

/-- (B2) of the certificate. -/
theorem b2_of_check (hw : WF d) (hb2 : d.checkB2 w = true) :
    ∀ {sp i j}, i < d.n → j < d.n → d.blkOf i = d.blkOf j →
      Marked d.silentList d.blkOf sp i → Marked d.silentList d.blkOf sp j := by
  simp only [Data.checkB2, Bool.and_eq_true] at hb2
  obtain ⟨hc, hp⟩ := hb2
  refine b2_of_marking (R := ⟨d.n, fun _ => ((), ()), fun _ => 0, d.silentList, [],
    fun _ => (0 : Nat)⟩) (Exit d w) (fun h => marked_of_exit hp h)
    (fun he hne ht hp' => exit_of_edge hw hc he hne ht hp') ?_ ?_
  · intro sp p x he _ hb hx
    obtain ⟨e, hen, hs, _⟩ := mem_silentList.mp he
    refine ⟨hs ▸ (hw.silent hen).1, ?_⟩
    rw [hb]; exact hx.2
  · intro sp i j _ hj hb hi
    exact ⟨hj, by rw [← hb]; exact hi.2⟩

end B2

/-! ## (B3): the classes a state's rises enter -/

section B3

variable {d : Data} {w : Witness}

theorem enters_iff (hb3 : d.checkB3 w = true) {p c : Nat} (hp : p < d.n) :
    (∃ y, (p, y) ∈ d.riseList ∧ d.blkOf y = c) ↔
      ∃ j, j < g w.roff (p + 1) - g w.roff p ∧ g w.rlist (g w.roff p + j) = c := by
  simp only [Data.checkB3, Bool.and_eq_true] at hb3
  obtain ⟨⟨hen, hwit⟩, _⟩ := hb3
  constructor
  · rintro ⟨y, hy, rfl⟩
    obtain ⟨e, he, hs, hd⟩ := mem_riseList.mp hy
    have := all_at hen e he
    simp only [hs, hd, Bool.and_eq_true, beq_iff_eq, decide_eq_true_eq] at this
    obtain ⟨⟨h1, h2⟩, h3⟩ := this
    refine ⟨g w.rix e - g w.roff p, by omega, ?_⟩
    rw [Nat.add_sub_of_le h1]; exact h3
  · rintro ⟨j, hj, hc⟩
    have := all_at (all_at hwit p hp) j hj
    simp only [Bool.and_eq_true, beq_iff_eq, decide_eq_true_eq] at this
    obtain ⟨⟨h1, h2⟩, h3⟩ := this
    exact ⟨_, mem_riseList.mpr ⟨_, h1, h2, rfl⟩, h3.trans hc⟩

theorem enters_rep (hw : WF d) (hb3 : d.checkB3 w = true) {p c : Nat} (hp : p < d.n) :
    (∃ y, (p, y) ∈ d.riseList ∧ d.blkOf y = c) ↔
      ∃ y, (d.repOf (d.blkOf p), y) ∈ d.riseList ∧ d.blkOf y = c := by
  have hq := hw.rep (hw.blk hp)
  rw [enters_iff hb3 hp, enters_iff hb3 hq]
  have hs := hb3
  simp only [Data.checkB3, Bool.and_eq_true] at hs
  have := all_at hs.2 p hp
  simp only [Bool.and_eq_true, beq_iff_eq] at this
  obtain ⟨hlen, hall⟩ := this
  constructor
  · rintro ⟨j, hj, hc⟩
    have := all_at hall j hj
    simp only [beq_iff_eq] at this
    exact ⟨j, hlen ▸ hj, this ▸ hc⟩
  · rintro ⟨j, hj, hc⟩
    have hj' : j < g w.roff (p + 1) - g w.roff p := hlen ▸ hj
    have := all_at hall j hj'
    simp only [beq_iff_eq] at this
    exact ⟨j, hj', this.trans hc⟩

end B3

/-! ## The run, its completeness, and the certificate -/

variable {S I O XSa Ra : Type}

/-- The run of a record, with `st i` the state numbered `i` and `idx x` the number of `x`,
labelled by the `obs` value and the input bytes. -/
def Data.run (d : Data) (st : Nat → S × I) (idx : S × I → Nat) : ExplRun S I (Nat × Nat) :=
  ⟨d.n, st, idx, d.silentList, d.riseList, fun i => (g d.ov i, g d.iv i)⟩

/-- Read from the dump code: the numbers stand for the states of a set that holds every
reachable state of `X`, the edges are `X`'s from reachable states, and the listed initial
states are initial. -/
structure Graph (X : LTS (S × I) (XSa ⊕ Ra)) (d : Data) (st : Nat → S × I)
    (idx : S × I → Nat) : Prop where
  idx_lt : ∀ {x}, X.Reach x → idx x < d.n
  st_idx : ∀ {x}, X.Reach x → st (idx x) = x
  idx_st : ∀ {i}, i < d.n → idx (st i) = i
  silent_sound : ∀ {i j}, (i, j) ∈ d.silentList → (silent X).Next (st i) (st j)
  silent_complete : ∀ {x y}, X.Reach x → (silent X).Next x y → (idx x, idx y) ∈ d.silentList
  rise_sound : ∀ {i j}, (i, j) ∈ d.riseList → ∃ r, X.step (st i) (.inr r) (st j)
  rise_complete : ∀ {x y r}, X.Reach x → X.step x (.inr r) y → (idx x, idx y) ∈ d.riseList
  init : ∀ {k}, k < d.init.size → X.init (st (g d.init k))

/-- Read from the dump code: equal value numbers mean equal values. -/
structure Faithful (obs : S → O) (d : Data) (st : Nat → S × I) : Prop where
  obs : ∀ {i j}, i < d.n → j < d.n → g d.ov i = g d.ov j → obs (st i).1 = obs (st j).1
  s : ∀ {i j}, i < d.n → j < d.n → g d.sv i = g d.sv j → (st i).1 = (st j).1
  inp : ∀ {i j}, i < d.n → j < d.n → g d.iv i = g d.iv j → (st i).2 = (st j).2

section Run

variable {X : LTS (S × I) (XSa ⊕ Ra)} {obs : S → O} {d : Data} {w : Witness}
  {st : Nat → S × I} {idx : S × I → Nat}

/-- **Every numbered state is reachable**, by induction on the rank of its parent. -/
theorem reach_of_check (hG : Graph X d st idx) (h : d.checkReach w = true) :
    ∀ {i}, i < d.n → X.Reach (st i) := by
  have key : ∀ r i, i < d.n → g w.prk i < r → X.Reach (st i) := by
    intro r
    induction r with
    | zero => intro _ _ h; exact absurd h (Nat.not_lt_zero _)
    | succ r ih =>
      intro i hi hr
      have := all_at h i hi
      simp only [Bool.or_eq_true, Bool.and_eq_true, beq_iff_eq, decide_eq_true_eq] at this
      rcases this with (⟨⟨_, hk⟩, he⟩ | ⟨⟨⟨⟨_, hk⟩, he⟩, hs⟩, hlt⟩) | ⟨⟨⟨⟨_, hk⟩, he⟩, hs⟩, hlt⟩
      · rw [← he]; exact .init (hG.init hk)
      · obtain ⟨b, hb⟩ := hG.silent_sound (mem_silentList.mpr ⟨_, hk, rfl, rfl⟩)
        rw [he] at hb
        exact .step (ih _ hs (by omega)) hb
      · obtain ⟨r', hb⟩ := hG.rise_sound (mem_riseList.mpr ⟨_, hk, rfl, rfl⟩)
        rw [he] at hb
        exact .step (ih _ hs (by omega)) hb
  intro i hi
  exact key _ i hi (Nat.lt_succ_self _)

/-- **An accepted record's run is complete.** -/
theorem complete_of_check (hG : Graph X d st idx) (hF : Faithful obs d st)
    (h : d.check = true) : Complete X obs (d.run st idx) := by
  obtain ⟨hw, -, -, -, -, -, -, hR⟩ := parts h
  refine
    { idx_lt := hG.idx_lt, st_idx := hG.st_idx, st_reach := fun hi => reach_of_check hG hR hi, idx_st := hG.idx_st,
      silent_sound := ?_, silent_complete := hG.silent_complete, rise_sound := ?_,
      rise_complete := hG.rise_complete, label := ?_ }
  · intro i j he
    obtain ⟨e, hen, hs, hd⟩ := mem_silentList.mp he
    exact ⟨hs ▸ (hw.silent hen).1, hd ▸ (hw.silent hen).2, hG.silent_sound he⟩
  · intro i j he
    obtain ⟨e, hen, hs, hd⟩ := mem_riseList.mp he
    exact ⟨hs ▸ (hw.rise hen).1, hd ▸ (hw.rise hen).2, hG.rise_sound he⟩
  · intro x y hx hy hl
    simp only [Data.run, Prod.mk.injEq] at hl
    have hxn := hG.idx_lt hx
    have hyn := hG.idx_lt hy
    have ho := hF.obs hxn hyn hl.1
    have hi := hF.inp hxn hyn hl.2
    rw [hG.st_idx hx, hG.st_idx hy] at ho hi
    exact ⟨ho, hi⟩

/-- **The certificate.** For any witness `validate` accepts, the partition passes
`Sweep.Cert`. -/
theorem cert_of_validate (hw : WF d) (hb1 : d.checkB1 = true) (hb2 : d.checkB2 w = true)
    (hb3 : d.checkB3 w = true) : Cert (d.run st idx) d.blkOf where
  b1 := by
    intro i j hi hj hb
    have h1 := all_at hb1 i hi
    have h2 := all_at hb1 j hj
    simp only [Bool.and_eq_true, beq_iff_eq] at h1 h2
    show (g d.ov i, g d.iv i) = (g d.ov j, g d.iv j)
    rw [h1.1, h1.2, h2.1, h2.2, hb]
  b2 := fun hi hj hb hm => b2_of_check hw hb2 hi hj hb hm
  b3 := by
    intro i c hi
    obtain ⟨hf, hfb⟩ := first_spec (blk := d.blkOf) (n := d.n) hi
    show (∃ y, (i, y) ∈ d.riseList ∧ d.blkOf y = c) ↔
      ∃ y, (first d.blkOf d.n (d.blkOf i), y) ∈ d.riseList ∧ d.blkOf y = c
    rw [enters_rep hw hb3 hi, enters_rep hw hb3 hf, hfb]

/-- **An accepted partition is stable**, with the dumped representatives. -/
theorem stable_of_check (hG : Graph X d st idx) (hF : Faithful obs d st)
    (h : d.check = true) :
    Stable X obs (blockOf (d.run st idx) d.blkOf) (fun c => st (d.repOf c)) := by
  obtain ⟨hw, hrep, hb1, hb2, hb3, -, -, -⟩ := parts h
  have hC := complete_of_check hG hF h
  have hS := stable_of_cert hC (cert_of_validate (st := st) (idx := idx) hw hb1 hb2 hb3)
  have rep_of : ∀ {x}, X.Reach x →
      d.repOf (blockOf (d.run st idx) d.blkOf x) < d.n ∧
        d.blkOf (d.repOf (blockOf (d.run st idx) d.blkOf x)) =
          blockOf (d.run st idx) d.blkOf x := by
    intro x hx
    have hc := hw.blk (hG.idx_lt hx)
    have := all_at hrep _ hc
    simp only [Bool.and_eq_true, beq_iff_eq, decide_eq_true_eq] at this
    exact this
  refine ⟨hS.b1, hS.b2, hS.b3, ?_, ?_⟩
  · intro x hx
    obtain ⟨hn, hb⟩ := rep_of hx
    show d.blkOf (idx (st _)) = _
    rw [hG.idx_st hn]; exact hb
  · intro x hx
    exact hC.st_reach (rep_of hx).1

/-- The steps of `X` from its reachable states. -/
def onReach {σ α : Type} (X : LTS σ α) : LTS σ α where
  init := X.init
  step x a y := X.Reach x ∧ X.step x a y

theorem reach_onReach {σ α : Type} {X : LTS σ α} {x : σ} : (onReach X).Reach x ↔ X.Reach x := by
  constructor
  · intro h
    induction h with
    | init h => exact .init h
    | step _ hs _ => exact .step hs.1 hs.2
  · intro h
    induction h with
    | init h => exact .init h
    | step hx hs ih => exact .step ih ⟨hx, hs⟩

/-- **The shape of `X`.** On an accepted record, the reachable part of `X` has Definition 4's
shape: a silent edge keeps the input bytes, a rise keeps `S`. -/
theorem extended_of_check (hG : Graph X d st idx) (hF : Faithful obs d st)
    (h : d.check = true) : Extended (onReach X) := by
  obtain ⟨hw, -, -, -, -, hsi, hrs, -⟩ := parts h
  refine ⟨fun {s i b s' i'} hst => ?_, fun {s i r s' i'} hst => ?_⟩
  · obtain ⟨hx, hs⟩ := hst
    have hy : X.Reach (s', i') := .step hx hs
    obtain ⟨e, hen, hsrc, hdst⟩ := mem_silentList.mp (hG.silent_complete hx ⟨b, hs⟩)
    have := all_at hsi e hen
    simp only [hsrc, hdst, beq_iff_eq] at this
    have := hF.inp (hG.idx_lt hx) (hG.idx_lt hy) this
    rw [hG.st_idx hx, hG.st_idx hy] at this
    exact this.symm
  · obtain ⟨hx, hs⟩ := hst
    have hy : X.Reach (s', i') := .step hx hs
    obtain ⟨e, hen, hsrc, hdst⟩ := mem_riseList.mp (hG.rise_complete hx hs)
    have := all_at hrs e hen
    simp only [hsrc, hdst, beq_iff_eq] at this
    have := hF.s (hG.idx_lt hx) (hG.idx_lt hy) this
    rw [hG.st_idx hx, hG.st_idx hy] at this
    exact this.symm

end Run

end RdraIsh.PCert
