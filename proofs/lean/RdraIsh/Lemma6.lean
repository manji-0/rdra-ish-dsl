import RdraIsh.Restrict

/-!
# Lemma 6 over actions (cones.md, "The reduction by an independent sub-lifecycle")

For the extended sub-lifecycle `X = D.ext` of `Restrict.lean` and the cone system `coneW`
(the cone without the steps of actions that write nothing and are not kept, which stutter),
under `Lemma6Hyp`:

- `s_fwd`: every `S_a` step of the cone is a silent edge of `X` from `x(t, s)`, by the same
  action and binding (the cells of "Parameters" for folded and unfolded `S_a` actions, the
  `C ⇒ X` direction);
- `s_back`: every silent edge of `X` from `x(t, s)` is an `S_a` step of the cone, by the same
  action at a binding that agrees with the edge's outside the fold's free `T` parameters
  (`X ⇒ C`);
- `rise_fwd`: every `T_a` step is matched by one rise per stage of `stages`: the restricted
  action for the raw inputs, then one flip per bit that changes;
- `lemma6_by`, `lemma6_staged`, `lemma6`, `lemma6_fwd`: the hypotheses `Lemma6By`,
  `Lemma6Staged`, `Lemma6` and `Lemma6Fwd` of `Reduction.lean`, `ReductionLabels.lean` and
  `Stages.lean`, as theorems;
- `a8_actions`, `a8_actions_holds_iff`: Theorem A8 with the code's stages for this cone and
  `X`, from `Lemma6Hyp`, (B1)–(B3) and the rise table alone; the invariant form reads the cone
  `cone` itself, with the actions that write nothing.

The "cannot occur" cells for an unfolded `S_a` action (a parameter on a stub or a dropped
entity whose role reads its row) are derived from Lemma 5 (a)(c) and `Local.conj_rd`
(`unfolded_side`), and so is the folded table's last row (a `new` `T` parameter of an `S_a`
action) from `new_writes` and Lemma 5 (a).
-/

namespace RdraIsh.Act

open LTS

variable {V E A Val K : Type} [DecidableEq K] {M : Model V E A Val} {D : Ext M K}
  {obs : V → Prop}

omit [DecidableEq K] in
theorem mem_roleConjs_inv (R : Rows Val) (ex : E → V) :
    ∀ (n : Nat) (ps : List (Param E)) (g : Conj V Val), g ∈ roleConjs R ex n ps →
      ∃ j p, ps[j]? = some p ∧ g = roleConj R ex (n + j) p
  | _, [], g, h => by simp [roleConjs] at h
  | n, q :: ps, g, h => by
    simp only [roleConjs, List.mem_cons] at h
    rcases h with rfl | h
    · exact ⟨0, q, rfl, by simp⟩
    · obtain ⟨j, p, hp, rfl⟩ := mem_roleConjs_inv R ex (n + 1) ps g h
      exact ⟨j + 1, p, by simpa using hp, by rw [show n + (j + 1) = n + 1 + j by omega]⟩

omit [DecidableEq K] in
theorem Rows.cand_of_not_reads (R : Rows Val) {r : Role} (h : r.readsRow = false) (v : Val)
    (c : Nat) : R.cand r v c := by
  cases r <;> simp_all [Role.readsRow, Rows.cand]

namespace Ext

variable (D)

omit [DecidableEq K] in
/-- On `S_v` and the inputs, the valuation of `x(t, s)` is that of `(t, s)`. -/
theorem xval_inp (t : TV D.tv (Val := Val)) (s : SV D.tv (Val := Val)) {v : V}
    (h : D.tv v = false ∨ D.inputs v = true) : D.xval (s, D.inp t) v = join D.tv (t, s) v := by
  unfold xval
  by_cases htv : D.tv v = false
  · rw [dite_eq_left htv, join_s htv]
  · have htt : D.tv v = true := by simpa using htv
    have hi : D.inputs v = true := h.resolve_left htv
    rw [dite_eq_right htv, dite_eq_left hi, join_t htt]
    rfl

omit [DecidableEq K] in
theorem agree_inp (t : TV D.tv (Val := Val)) (s : SV D.tv (Val := Val)) :
    AgreeOn (fun v => D.tv v = false ∨ D.inputs v = true) (D.xval (s, D.inp t))
      (join D.tv (t, s)) :=
  fun _ h => D.xval_inp t s h

variable {D}

omit [DecidableEq K] in
theorem side_s {e : E} (h : D.side e = .s) : D.tv (D.ex e) = false := by
  unfold side at h
  by_cases h1 : D.tv (D.ex e) = false
  · exact h1
  · rw [ite_eq_right h1] at h
    by_cases h2 : D.inputs (D.ex e) = true
    · rw [ite_eq_left h2] at h; cases h
    · rw [ite_eq_right h2] at h
      by_cases h3 : D.stub e = true
      · rw [ite_eq_left h3] at h; cases h
      · rw [ite_eq_right h3] at h; cases h

omit [DecidableEq K] in
theorem side_input {e : E} (h : D.side e = .input) : D.inputs (D.ex e) = true := by
  unfold side at h
  by_cases h1 : D.tv (D.ex e) = false
  · rw [ite_eq_left h1] at h; cases h
  · rw [ite_eq_right h1] at h
    by_cases h2 : D.inputs (D.ex e) = true
    · exact h2
    · rw [ite_eq_right h2] at h
      by_cases h3 : D.stub e = true
      · rw [ite_eq_left h3] at h; cases h
      · rw [ite_eq_right h3] at h; cases h

omit [DecidableEq K] in
theorem side_stub {e : E} (h : D.side e = .stub) :
    D.tv (D.ex e) = true ∧ D.inputs (D.ex e) = false := by
  unfold side at h
  by_cases h1 : D.tv (D.ex e) = false
  · rw [ite_eq_left h1] at h; cases h
  · rw [ite_eq_right h1] at h
    by_cases h2 : D.inputs (D.ex e) = true
    · rw [ite_eq_left h2] at h; cases h
    · exact ⟨by simpa using h1, by simpa using h2⟩

/-! ## Guards: roles and the other conjuncts -/

theorem role_of_guard (h : Lemma6Hyp D obs) {a : A} {b : Binding} {σ : V → Val}
    (hg : M.guard a b σ) {j : Nat} {p : Param E} (hp : (M.params a)[j]? = some p) :
    D.rows.cand p.role (σ (D.ex p.entity)) (b.slot j) := by
  have hm : roleConj D.rows D.ex (0 + j) p ∈ M.guards a := by
    rw [h.shape a]
    exact List.mem_append_left _ (mem_roleConjs _ _ 0 _ j p hp)
  have := hg _ hm
  simpa [roleConj] using this

theorem og_of_guard (h : Lemma6Hyp D obs) {a : A} {b : Binding} {σ : V → Val}
    (hg : M.guard a b σ) {g : Conj V Val} (hm : g ∈ D.og a) : g.holds b σ :=
  hg g (by rw [h.shape a]; exact List.mem_append_right _ hm)

theorem guard_of (h : Lemma6Hyp D obs) {a : A} {b : Binding} {σ : V → Val}
    (hr : ∀ j p, (M.params a)[j]? = some p → D.rows.cand p.role (σ (D.ex p.entity)) (b.slot j))
    (ho : ∀ g ∈ D.og a, g.holds b σ) : M.guard a b σ := by
  intro g hg
  rw [h.shape a] at hg
  rcases List.mem_append.mp hg with hg | hg
  · obtain ⟨j, p, hp, rfl⟩ := mem_roleConjs_inv _ _ 0 _ g hg
    simpa [roleConj] using hr j p hp
  · exact ho g hg

/-- A kept conjunct reads the same at `x(t, s)` as at `(t, s)`. -/
theorem kept_iff (h : Lemma6Hyp D obs) {a : A} {g : Conj V Val} (hm : g ∈ D.og a)
    (hk : D.kept a g) (b : Binding) (t : TV D.tv (Val := Val)) (s : SV D.tv (Val := Val)) :
    g.holds b (join D.tv (t, s)) ↔ g.holds b (D.xval (s, D.inp t)) := by
  have hgm : g ∈ M.guards a := by rw [h.shape a]; exact List.mem_append_right _ hm
  constructor
  · exact h.local_.conj_local hgm b _ _ (fun v hv => (D.xval_inp t s (h.kept_rd a g hk v hv)).symm)
  · exact h.local_.conj_local hgm b _ _ (fun v hv => D.xval_inp t s (h.kept_rd a g hk v hv))

/-- An unfolded `S_a` action has no parameter on a stub or a dropped entity whose role reads
its row: the role conjunct's read is in `Rd ∪ Wr`, so in `S_v ∪ inputs` (Lemma 5 (a)(c)). -/
theorem unfolded_side (h : Lemma6Hyp D obs) {a : A} (ha : Sa M D.ta a) (hf : D.fold a = none)
    {j : Nat} {p : Param E} (hp : (M.params a)[j]? = some p) (hr : p.role.readsRow = true) :
    D.tv (D.ex p.entity) = false ∨ D.inputs (D.ex p.entity) = true := by
  have hm : roleConj D.rows D.ex (0 + j) p ∈ M.guards a := by
    rw [h.shape a]
    exact List.mem_append_left _ (mem_roleConjs _ _ 0 _ j p hp)
  have hv := h.local_.conj_rd hm (D.ex p.entity) (by simp [roleConj, hr])
  rcases List.mem_append.mp hv with hrd | hwr
  · rcases h.reads a ha _ hrd with h1 | h2 | ⟨F, hF, _⟩
    · exact .inl h1
    · exact .inr h2
    · rw [hf] at hF; cases hF
  · exact .inl (h.lemma5.a a ha _ hwr)

omit [DecidableEq K] in
/-- The role `X` gives a parameter admits the slot the cone binds it to, given that a stub
parameter whose role needs a present row is rebound. -/
theorem xrole_cand {a : A} {b : Binding} {t : TV D.tv (Val := Val)} {s : SV D.tv (Val := Val)}
    {j : Nat} {p : Param E} {r : Role} (hr : D.xrole a j p = some r)
    (hc : D.rows.cand p.role (join D.tv (t, s) (D.ex p.entity)) (b.slot j))
    (hstub : D.side p.entity = .stub → p.role.needsPresent = true → D.isKey a j = false →
      D.bindStub a = true) :
    D.rows.cand r (D.xval (s, D.inp t) (D.ex p.entity)) (b.slot j) := by
  unfold xrole at hr
  cases hs : D.side p.entity with
  | s =>
    rw [hs] at hr
    cases hr
    rw [D.xval_inp t s (.inl (side_s hs))]; exact hc
  | input =>
    rw [hs] at hr
    cases hk : D.isKey a j
    · rw [hk] at hr; cases hr
      rw [D.xval_inp t s (.inr (side_input hs))]; exact hc
    · rw [hk] at hr; cases hr; trivial
  | stub =>
    rw [hs] at hr
    cases hk : D.isKey a j
    · rw [hk] at hr
      cases hrole : p.role with
      | new => rw [hrole] at hr; cases hr
      | earlier n =>
        rw [hrole] at hr
        simp [Role.needsPresent] at hr
        subst hr; trivial
      | existing =>
        rw [hrole] at hr
        have := hstub hs (by rw [hrole]; rfl) hk
        simp [Role.needsPresent, this] at hr
        subst hr; trivial
      | parent =>
        rw [hrole] at hr
        have := hstub hs (by rw [hrole]; rfl) hk
        simp [Role.needsPresent, this] at hr
        subst hr; trivial
      | triggered =>
        rw [hrole] at hr
        have := hstub hs (by rw [hrole]; rfl) hk
        simp [Role.needsPresent, this] at hr
        subst hr; trivial
    · rw [hk] at hr; cases hr; trivial
  | dropped => rw [hs] at hr; cases hr

/-- A guard of the cone at `(t, s)` gives the guard of `X` at `x(t, s)`, for the same binding. -/
theorem xguard_of_guard (h : Lemma6Hyp D obs) {a : A} {b : Binding} {t : TV D.tv (Val := Val)}
    {s : SV D.tv (Val := Val)} (hg : M.guard a b (join D.tv (t, s)))
    (hstub : ∀ j p, (M.params a)[j]? = some p → D.side p.entity = .stub →
      p.role.needsPresent = true → D.isKey a j = false → D.bindStub a = true) :
    D.xguard a b (s, D.inp t) := by
  refine ⟨fun j p r hp hr => xrole_cand hr (role_of_guard h hg hp) (hstub j p hp),
    fun g hm hk => (kept_iff h hm hk b t s).mp (og_of_guard h hg hm), fun F hF => ?_⟩
  show D.bitOf (F.bit (b.slot F.key)) t = true
  refine (h.exact a F hF t s _).mpr ⟨b, rfl, fun j p hp _ hn => ?_, fun g hg' => ?_⟩
  · exact (D.rows.cand_present hn _ _).mp (role_of_guard h hg hp)
  · exact og_of_guard h hg (h.folded_og a F hF g hg')

/-- The stub condition of `xguard_of_guard` for an `S_a` action. -/
theorem hstub_sa (h : Lemma6Hyp D obs) {a : A} (ha : Sa M D.ta a) :
    ∀ j p, (M.params a)[j]? = some p → D.side p.entity = .stub →
      p.role.needsPresent = true → D.isKey a j = false → D.bindStub a = true := by
  intro j p hp hs hn _
  cases hf : D.fold a with
  | some F => simp [bindStub, hf, h.bind]
  | none =>
    obtain ⟨h1, h2⟩ := side_stub hs
    rcases unfolded_side h ha hf hp (Role.readsRow_of_needsPresent hn) with h3 | h3
    · rw [h1] at h3; cases h3
    · rw [h2] at h3; cases h3

/-- The stub condition of `xguard_of_guard` for a `T_a` action. -/
theorem hstub_ta (h : Lemma6Hyp D obs) {a : A} (ha : D.ta a = true) :
    ∀ j p, (M.params a)[j]? = some p → D.side p.entity = .stub →
      p.role.needsPresent = true → D.isKey a j = false → D.bindStub a = true := by
  intro _ _ _ _ _ _
  simp [bindStub, ha, h.bind]

/-! ## Effects -/

/-- An `S_a` action writes the same `S_v` values at `x(t, s)` as at `(t, s)`. -/
theorem eff_x (h : Lemma6Hyp D obs) {a : A} (ha : Sa M D.ta a) (b : Binding)
    (t : TV D.tv (Val := Val)) (s : SV D.tv (Val := Val)) {v : V} (hv : D.tv v = false) :
    M.eff a b (D.xval (s, D.inp t)) v = M.eff a b (join D.tv (t, s)) v := by
  by_cases hw : v ∈ M.wr a
  · cases hf : D.fold a with
    | some F => exact h.eff_s a F hf b _ _ (D.agree_inp t s) v hw
    | none =>
      refine h.local_.eff_local a b _ _ (fun u hu => ?_) v hw
      apply D.xval_inp t s
      rcases List.mem_append.mp hu with hrd | hwr
      · rcases h.reads a ha _ hrd with h1 | h2 | ⟨F, hF, _⟩
        · exact .inl h1
        · exact .inr h2
        · rw [hf] at hF; cases hF
      · exact .inl (h.lemma5.a a ha _ hwr)
  · rw [h.local_.eff_frame a b _ v hw, h.local_.eff_frame a b _ v hw, D.xval_inp t s (.inl hv)]

/-- An `S_a` action's effect does not read its fold's free `T` parameters. -/
theorem eff_agree (h : Lemma6Hyp D obs) {a : A} {b b' : Binding}
    (hb : BindAgree (D.free a) b b') (σ : V → Val) (v : V) :
    M.eff a b σ v = M.eff a b' σ v := by
  by_cases hw : v ∈ M.wr a
  · cases hf : D.fold a with
    | some F => exact h.eff_names a F hf b b' σ hb v hw
    | none =>
      have : b = b' := by
        obtain ⟨bs, bc⟩ := b
        obtain ⟨bs', bc'⟩ := b'
        obtain ⟨h1, h2⟩ := hb
        have hs : bs = bs' := funext fun j => h1 j (fun ⟨F, hF, _⟩ => by rw [hf] at hF; cases hF)
        simp only at h2
        rw [hs, h2]
      rw [this]
  · rw [h.local_.eff_frame a b σ v hw, h.local_.eff_frame a b' σ v hw]

/-! ## The `S_a` half of Lemma 6 -/

/-- **`C ⇒ X`**: an `S_a` step of the cone is a silent edge of `X` from `x(t, s)` to
`x(t, s')`, by the same action and binding. -/
theorem s_fwd (h : Lemma6Hyp D obs) {t : TV D.tv (Val := Val)} {s s' : SV D.tv (Val := Val)}
    {l : SaL D.ta} (hs : (coneW M D.tv D.ta).step (t, s) (.inr l) (t, s')) :
    D.ext.step (s, D.inp t) (.inl l.1) (s', D.inp t) := by
  obtain ⟨⟨a, b⟩, hta⟩ := l
  obtain ⟨hw, hg, he⟩ := hs
  have ha : Sa M D.ta a := ⟨hta, hw⟩
  refine ⟨ha, xguard_of_guard h hg (hstub_sa h ha), ?_⟩
  refine Prod.ext (funext fun ⟨v, hv⟩ => ?_) rfl
  have := congrFun he v
  rw [join_s hv] at this
  show s' ⟨v, hv⟩ = M.eff a b (D.xval (s, D.inp t)) v
  rw [eff_x h ha b t s hv]
  exact this

/-- **`X ⇒ C`**: a silent edge of `X` from `x(t, s)` is an `S_a` step of the cone from
`(t, s)`, by the same action at a binding that agrees with the edge's outside the fold's free
`T` parameters. The free ones come from step (a): the bit holds at the key's slot, so some
binding of them meets every folded guard. -/
theorem s_back (h : Lemma6Hyp D obs) {t : TV D.tv (Val := Val)} {s s' : SV D.tv (Val := Val)}
    {l' : A × Binding} (hx : D.ext.step (s, D.inp t) (.inl l') (s', D.inp t)) :
    ∃ l, D.stands l l' ∧ (coneW M D.tv D.ta).step (t, s) (.inr l) (t, s') := by
  obtain ⟨a, b'⟩ := l'
  obtain ⟨ha, hg, hy⟩ := hx
  have hs' : s' = fun v => M.eff a b' (D.xval (s, D.inp t)) v.1 := (Prod.mk.inj hy).1
  -- A binding of the cone that agrees with `b'` outside the free `T` parameters and meets
  -- the guard at `(t, s)`.
  have key : ∃ bs : Binding, BindAgree (D.free a) bs b' ∧
      (∀ j p, (M.params a)[j]? = some p →
        D.rows.cand p.role (join D.tv (t, s) (D.ex p.entity)) (bs.slot j)) ∧
      (∀ g ∈ D.og a, g.holds bs (join D.tv (t, s))) := by
    cases hf : D.fold a with
    | none =>
      refine ⟨b', ⟨fun _ _ => rfl, rfl⟩, fun j p hp => ?_, fun g hm => ?_⟩
      · cases hrr : p.role.readsRow
        · exact D.rows.cand_of_not_reads hrr _ _
        · have hside := unfolded_side h ha hf hp hrr
          have hxr : D.xrole a j p = some p.role := by
            have hk : D.isKey a j = false := by simp [isKey, hf]
            unfold xrole
            rcases hside with h1 | h1
            · have : D.side p.entity = .s := by simp [side, h1]
              rw [this]
            · have : D.side p.entity = .input := by
                by_cases h0 : D.tv (D.ex p.entity) = false
                · simp [side, h0]
                  -- unreachable: an input is in `T_v`
                  have := D.inputs_t _ h1; rw [h0] at this; cases this
                · simp [side, h0, h1]
              rw [this, hk]; rfl
          have := hg.1 j p p.role hp hxr
          rwa [D.xval_inp t s hside] at this
      · rcases h.cover a ha g hm with hk | ⟨F, hF, _⟩
        · exact (kept_iff h hm hk b' t s).mpr (hg.2.1 g hm hk)
        · rw [hf] at hF; cases hF
    | some F =>
      have hbit : D.bitOf (F.bit (b'.slot F.key)) t = true := hg.2.2 F hf
      obtain ⟨b'', hkey, hfc⟩ := (h.exact a F hf t s _).mp hbit
      classical
      let bs : Binding := ⟨fun j => if D.free a j then b''.slot j else b'.slot j, b'.choice⟩
      have hbs : BindAgree (D.free a) bs b' :=
        ⟨fun j hj => by simp only [bs, ite_eq_right hj], rfl⟩
      have hkey' : bs.slot F.key = b''.slot F.key := by
        have : ¬ D.free a F.key := fun ⟨F', hF', _, hne⟩ => by
          rw [hf] at hF'; cases hF'; exact hne rfl
        simp only [bs, ite_eq_right this]; exact hkey.symm
      have hT : ∀ j, D.tParam a j → bs.slot j = b''.slot j := by
        intro j hj
        by_cases hfr : D.free a j
        · simp only [bs, ite_eq_left hfr]
        · have : j = F.key := Classical.byContradiction fun hne => hfr ⟨F, hf, hj, hne⟩
          subst this; exact hkey'
      refine ⟨bs, hbs, fun j p hp => ?_, fun g hm => ?_⟩
      · by_cases htv : D.tv (D.ex p.entity) = false
        · have hnf : ¬ D.free a j := fun ⟨_, _, ⟨p', hp', ht'⟩, _⟩ => by
            rw [hp] at hp'; cases hp'; rw [htv] at ht'; cases ht'
          have hside : D.side p.entity = .s := by simp [side, htv]
          have hxr : D.xrole a j p = some p.role := by unfold xrole; rw [hside]
          have := hg.1 j p p.role hp hxr
          rw [D.xval_inp t s (.inl htv)] at this
          rw [hbs.1 j hnf]; exact this
        · have htt : D.tv (D.ex p.entity) = true := by simpa using htv
          rw [hT j ⟨p, hp, htt⟩]
          cases hrole : p.role with
          | earlier n => trivial
          | new =>
            have := h.lemma5.a a ha _ (h.new_writes a j p hp hrole)
            rw [this] at htt; cases htt
          | existing =>
            have := hfc.1 j p hp htt (by rw [hrole]; rfl)
            simpa [Rows.cand] using this
          | parent =>
            have := hfc.1 j p hp htt (by rw [hrole]; rfl)
            simpa [Rows.cand] using this
          | triggered =>
            have := hfc.1 j p hp htt (by rw [hrole]; rfl)
            simpa [Rows.cand] using this
      · by_cases hgf : g ∈ F.folded
        · refine h.folded_names a F hf g hgf b'' bs _ (fun j hj => ?_) (hfc.2 g hgf)
          rcases hj with hj | hj
          · exact (hT j hj).symm
          · subst hj; exact hkey'.symm
        · have hk : D.kept a g := by
            rcases h.cover a ha g hm with hk | ⟨F', hF', hgF'⟩
            · exact hk
            · rw [hf] at hF'; cases hF'; exact absurd hgF' hgf
          have hj := (kept_iff h hm hk b' t s).mpr (hg.2.1 g hm hk)
          exact h.other_names a F hf g hm hgf b' bs _
            ⟨fun j hj => (hbs.1 j hj).symm, hbs.2.symm⟩ hj
  obtain ⟨bs, hbs, hr, ho⟩ := key
  refine ⟨⟨(a, bs), ha.1⟩, ⟨rfl, hbs⟩, ha.2, guard_of h hr ho, ?_⟩
  funext v
  by_cases htv : D.tv v = false
  · rw [join_s htv, hs']
    show M.eff a b' (D.xval (s, D.inp t)) v = M.eff a bs (join D.tv (t, s)) v
    rw [eff_x h ha b' t s htv, eff_agree h hbs]
  · have htt : D.tv v = true := by simpa using htv
    have hnw : v ∉ M.wr a := fun hw => by rw [h.lemma5.a a ha v hw] at htt; cases htt
    show join D.tv (t, s') v = M.eff a bs (join D.tv (t, s)) v
    rw [h.local_.eff_frame a bs _ v hnw, join_t htt, join_t htt]

/-! ## The rises -/

theorem riseThrough_append {S I XSa Ra : Type} {X : LTS (S × I) (XSa ⊕ Ra)} {x y z : S × I}
    {l₁ l₂ : List I} (h₁ : RiseThrough X x l₁ y) (h₂ : RiseThrough X y l₂ z) :
    RiseThrough X x (l₁ ++ l₂) z := by
  induction h₁ with
  | nil => exact h₂
  | cons r hs hi _ ih => exact .cons r hs hi (ih h₂)

/-- The flips over `ks` from bits `β` end at bits that are `β'` on `ks` and `β` elsewhere. -/
theorem flips_through (s : SV D.tv (Val := Val)) (r : D.Raw) (β' : K → Bool) :
    ∀ (ks : List K) (β : K → Bool), ∃ β₂, RiseThrough D.ext (s, (r, β)) (D.flips r β β' ks)
      (s, (r, β₂)) ∧ (∀ k ∈ ks, β₂ k = β' k) ∧ (∀ k, k ∉ ks → β₂ k = β k)
  | [], β => ⟨β, .nil _, by simp, fun _ _ => rfl⟩
  | k :: ks, β => by
    by_cases hk : β k = β' k
    · obtain ⟨β₂, hr, h1, h2⟩ := flips_through s r β' ks β
      refine ⟨β₂, by simp only [flips, ite_eq_left hk]; exact hr, fun k' hk' => ?_, fun k' hk' => ?_⟩
      · rcases List.mem_cons.mp hk' with rfl | hk'
        · by_cases hm : k' ∈ ks
          · exact h1 _ hm
          · rw [h2 _ hm, hk]
        · exact h1 _ hk'
      · exact h2 k' (fun hm => hk' (List.mem_cons_of_mem _ hm))
    · obtain ⟨β₂, hr, h1, h2⟩ := flips_through s r β' ks (Ext.upd β k (β' k))
      refine ⟨β₂, ?_, fun k' hk' => ?_, fun k' hk' => ?_⟩
      · simp only [flips, ite_eq_right hk]
        refine .cons (.inr (k, β' k)) ⟨?_, rfl⟩ rfl hr
        show β k = !β' k
        revert hk; cases β k <;> cases β' k <;> simp
      · rcases List.mem_cons.mp hk' with rfl | hk'
        · by_cases hm : k' ∈ ks
          · exact h1 _ hm
          · rw [h2 _ hm]; simp [Ext.upd]
        · exact h1 _ hk'
      · have hne : k' ≠ k := fun he => hk' (he ▸ List.mem_cons_self)
        rw [h2 k' (fun hm => hk' (List.mem_cons_of_mem _ hm))]
        simp [Ext.upd, hne]

/-- The flips over every key end at `β'`. -/
theorem flips_end (s : SV D.tv (Val := Val)) (r : D.Raw) (β β' : K → Bool) :
    RiseThrough D.ext (s, (r, β)) (D.flips r β β' D.keys) (s, (r, β')) := by
  obtain ⟨β₂, hr, h1, _⟩ := flips_through s r β' D.keys β
  have : β₂ = β' := funext fun k => h1 k (D.keys_all k)
  rw [this] at hr; exact hr

/-- **The rise half of Lemma 6** (step (c)): a `T_a` step of the cone is matched by one rise
of `X` per stage of `stages`: the restricted action for the raw inputs when they change, then
one flip per bit that changes. -/
theorem rise_fwd (h : Lemma6Hyp D obs) {t t' : TV D.tv (Val := Val)} {s : SV D.tv (Val := Val)}
    {l : TaL D.ta} (hs : (coneW M D.tv D.ta).step (t, s) (.inl l) (t', s)) :
    RiseThrough D.ext (s, D.inp t) (D.stages (D.inp t) (D.inp t')) (s, D.inp t') := by
  obtain ⟨⟨a, b⟩, hta⟩ := l
  obtain ⟨hg, he⟩ := hs
  unfold stages
  refine riseThrough_append (y := (s, ((D.inp t').1, (D.inp t).2))) ?_
    (flips_end s _ _ _)
  by_cases hr : (D.inp t).1 = (D.inp t').1
  · rw [ite_eq_left hr, ← hr]; exact .nil _
  · rw [ite_eq_right hr]
    refine .cons (.inl (a, b)) ⟨⟨hta, ?_⟩, xguard_of_guard h hg (hstub_ta h hta), ?_⟩
      rfl (.nil _)
    · refine Classical.byContradiction fun hno => hr (funext fun v => ?_)
      have hnw : v.1 ∉ M.wr a := fun hw => hno ⟨v.1, hw, v.2⟩
      have := congrFun he v.1
      have htt := D.inputs_t v.1 v.2
      rw [h.local_.eff_frame a b _ _ hnw, join_t htt, join_t htt] at this
      exact this.symm
    · refine Prod.ext rfl (Prod.ext (funext fun v => ?_) rfl)
      show (D.inp t').1 v = M.eff a b (D.xval (s, D.inp t)) v.1
      rw [h.rise_eff a b _ _ hta (D.agree_inp t s) v.1 v.2]
      have := congrFun he v.1
      rw [join_t (D.inputs_t v.1 v.2)] at this
      exact this

/-! ## Lemma 6 and Theorem A8 -/

/-- Definition 4's shape of `X`: a silent edge keeps the input bytes, a rise keeps `S`. -/
theorem extended : RdraIsh.Extended D.ext where
  silent_keeps_inp := by
    rintro s i ⟨a, b⟩ s' i' ⟨_, _, hy⟩
    exact (Prod.mk.inj hy).2
  rise_keeps_s := by
    rintro s i r s' i' hs
    rcases r with ⟨a, b⟩ | ⟨k, v⟩
    · exact (Prod.mk.inj hs.2.2).1
    · exact (Prod.mk.inj hs.2).1

omit [DecidableEq K] in
theorem stands_refl (l : SaL D.ta) : D.stands l l.1 := ⟨rfl, fun _ _ => rfl, rfl⟩

/-- **Lemma 6 with the binding relation** (`ReductionLabels.lean`'s `Lemma6By`), for the cone
of the writing actions and `X`, from `Lemma6Hyp`. -/
theorem lemma6_by (h : Lemma6Hyp D obs) :
    Lemma6By (coneW M D.tv D.ta) D.ext D.inp D.stands where
  init hi := ⟨_, hi, rfl⟩
  s_by {_t _s a _s'} _ hs := ⟨a.1, stands_refl a, s_fwd h hs⟩
  rise _ hs := (rise_fwd h hs).riseStar
  s_back_by _ hx := s_back h hx

/-- **Lemma 6 with the code's stages** (`Stages.lean`'s `Lemma6Staged`). -/
theorem lemma6_staged (h : Lemma6Hyp D obs) :
    Lemma6Staged (coneW M D.tv D.ta) D.ext D.inp D.stages where
  init hi := ⟨_, hi, rfl⟩
  s_step {_t _s a _s'} _ hs := ⟨a.1, s_fwd h hs⟩
  rise_staged _ hs := rise_fwd h hs
  s_step_back _ hx := let ⟨l, _, hl⟩ := s_back h hx; ⟨l, hl⟩

/-- **Lemma 6** (`Reduction.lean`'s `Lemma6`). -/
theorem lemma6 (h : Lemma6Hyp D obs) : Lemma6 (coneW M D.tv D.ta) D.ext D.inp :=
  (lemma6_by h).toLemma6

/-- **The `C ⇒ X` half of Lemma 6** (`Lemma6Fwd`), which the soundness of an `ok` reads. -/
theorem lemma6_fwd (h : Lemma6Hyp D obs) : Lemma6Fwd (coneW M D.tv D.ta) D.ext D.inp :=
  (lemma6 h).toLemma6Fwd

end Ext

/-! ## The cone of the writing actions -/

/-- `Split` of `coneW`, from Lemma 5 (a)(b) and `Local`. -/
theorem split_coneW [DecidableEq V] {tv : V → Bool} {ta : A → Bool} (hL : Local M)
    (h5 : Lemma5ab M tv ta obs) : RdraIsh.Split (coneW M tv ta) (obsMap tv h5.obs_s) :=
  let hc := split_of_lemma5 hL h5
  { s_keeps_t := fun hs => hc.s_keeps_t hs.2
    t_keeps_s := fun hs => hc.t_keeps_s hs
    t_reads_obs := fun ho hs => hc.t_reads_obs ho hs }

/-- `coneW` reaches what the cone reaches: a step of an action outside `T_a` that writes
nothing stutters (`Local.idle`). -/
theorem coneW_reach_iff {tv : V → Bool} {ta : A → Bool} (hL : Local M)
    (p : TV tv (Val := Val) × SV tv (Val := Val)) :
    (coneW M tv ta).Reach p ↔ (cone M tv ta).Reach p := by
  constructor
  · intro h
    induction h with
    | init h => exact Reach.init h
    | @step p l q _ hs ih =>
      cases l with
      | inl l => exact Reach.step ih (show (cone M tv ta).step p (.inl l) q from hs)
      | inr l => exact Reach.step ih (show (cone M tv ta).step p (.inr l) q from hs.2)
  · intro h
    induction h with
    | init h => exact Reach.init h
    | @step p l q _ hs ih =>
      cases l with
      | inl l => exact Reach.step ih (show (coneW M tv ta).step p (.inl l) q from hs)
      | inr l =>
        by_cases hw : M.wr l.1.1 = []
        · have hq : join tv q = join tv p := hL.idle hw hs
          have : q = p := by rw [← unjoin_join (tv := tv) q, hq, unjoin_join]
          rw [this]; exact ih
        · exact Reach.step ih (show (coneW M tv ta).step p (.inr l) q from ⟨hw, hs⟩)

/-- **Theorem A8 over actions, with the code's stages**: under `Lemma6Hyp`, (B1)–(B3) of a
partition of `X` and a complete rise table, the relation of A8 is a divergence-blind
branching bisimulation between the cone of the writing actions and the reduced cone whose `T`
move is `recanon` over `stages`, and it preserves `T_v` and `obs`. No hypothesis is `Split`,
`Lemma6*` or `Extended`. -/
theorem a8_actions [DecidableEq V] (h : Lemma6Hyp D obs) {Blk : Type} {B : D.XS → Blk} {rep : Blk → D.XS}
    {rt : Blk → D.Inp → Option Blk} (hX : Stable D.ext (obsMap D.tv h.lemma5.obs_s) B rep)
    (ht : RiseTable D.ext B rep rt) :
    BranchBisim (coneW M D.tv D.ta)
        (reducedStaged (coneW M D.tv D.ta) D.ext D.inp B rep rt D.stages) tLabel tLabel
        (rel (coneW M D.tv D.ta) D.inp B) ∧
      PreservesObs (rel (coneW M D.tv D.ta) D.inp B) (obsC (obsMap D.tv h.lemma5.obs_s))
        (obsR (obsMap D.tv h.lemma5.obs_s) rep) :=
  a8_staged (split_coneW h.local_ h.lemma5) (Ext.lemma6_staged h) Ext.extended hX ht

/-- **Theorem A8, consequence 1, over actions**: an invariant over `T_v ∪ obs` holds on the
cone system `cone` (every action, those that write nothing included) iff it holds on the
reduced cone. -/
theorem a8_actions_holds_iff [DecidableEq V] (h : Lemma6Hyp D obs) {Blk : Type} {B : D.XS → Blk}
    {rep : Blk → D.XS} {rt : Blk → D.Inp → Option Blk}
    (hX : Stable D.ext (obsMap D.tv h.lemma5.obs_s) B rep) (ht : RiseTable D.ext B rep rt)
    (P : TV D.tv (Val := Val) × ({v : V // obs v} → Val) → Prop) :
    (cone M D.tv D.ta).Holds (fun p => P (obsC (obsMap D.tv h.lemma5.obs_s) p)) ↔
      (reducedStaged (coneW M D.tv D.ta) D.ext D.inp B rep rt D.stages).Holds
        (fun q => P (obsR (obsMap D.tv h.lemma5.obs_s) rep q)) := by
  rw [← a8_staged_holds_iff (split_coneW h.local_ h.lemma5) (Ext.lemma6_staged h) Ext.extended
    hX ht P]
  exact ⟨fun hh p hp => hh p ((coneW_reach_iff h.local_ p).mp hp),
    fun hh p hp => hh p ((coneW_reach_iff h.local_ p).mpr hp)⟩

end RdraIsh.Act
