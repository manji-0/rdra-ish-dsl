import RdraIsh.Search
import RdraIsh.SymmetryLiveness

/-!
# The liveness search up to symmetry (cones.md, A23; `OrbitRun::decide_liveness`)

`explore/orbit/liveness.rs` decides `□(trig ⇒ ◇goal)` on the stored graph of a run up to
symmetry. It searches *pairs* `(r, b)` of a stored state and a binding: a real edge from `r`
to the concrete successor `t ≠ r`, stored as `can t = π(t)(t)`, takes `(r, b)` to
`(can t, π(t)(b))` (`PStep`). On those pairs it computes the goal-free pairs, their edges
(`PEdge`) and their strongly connected components (`SameP`), as N9's search does on states.

Fairness of a component is read on its *threads*: a node `((u, x))` is a pair `u` of the
component and a fair instance `x`, and a pair edge along `t` takes it to `(v, π(t)(x))`
(`TEdge`). A *piece* is a connected component of that graph with the direction forgotten
(`Conn`). The code (`Product::threads_covered`) accepts a component when it has a cycle
(`size ≥ 2` or a pair self-edge, `Candidate`) and every piece is covered: some node of it is
a pair where the instance is not enabled, or takes it on an edge inside the component
(`Covered`). The code builds the pieces from the nodes of enabled instances only, with
union-find, and marks a node covered when an edge joins it to a node whose instance is not
enabled (`ThreadsCovered`); `threadsCovered_iff` shows that this is the same criterion.

The theorems:

- `TEdge.reverse`: every thread edge inside a component can be walked back, so a piece is
  strongly connected. Round a closed walk the composite permutation lies in the group of
  its first state and has finite order (`order_exists`).
- `oAccepts_iff`: under A23's hypotheses (`Sym`, `SymLive`, `LiveEqui`, the canonical form
  `can = π(·)(·)`), with the reachable states in a list, every `G_s` finite, and the fair
  instances enabled at each state in a list, the orbit search accepts iff some binding has a
  behaviour of `M` that satisfies `Fair_P`, stays forever only where `stop` holds and
  violates the property. With N9 (`accepts_iff`) and Theorem A23 (`a23`), the orbit search's
  verdict is the plain search's and the annotated quotient's.
-/

namespace RdraIsh

open LTS

/-! ## Sequences driven by the edges of a walk -/

/-- The states a walk visits from `a` when each edge, given by its concrete successor `t j`,
moves the current state by `nx (t j)`. -/
def walkSeq {σ γ : Type} (nx : σ → γ → γ) (t : Nat → σ) (a : γ) : Nat → γ
  | 0 => a
  | j + 1 => nx (t j) (walkSeq nx t a j)

/-- The successors of the walk `t₁` for `n₁` edges, then those of `t₂`. -/
def catT {σ : Type} (t₁ : Nat → σ) (n₁ : Nat) (t₂ : Nat → σ) (k : Nat) : σ :=
  if k < n₁ then t₁ k else t₂ (k - n₁)

section WalkSeq

variable {σ γ : Type} (nx : σ → γ → γ)

theorem walkSeq_congr {t t' : Nat → σ} (a : γ) :
    ∀ j, (∀ i, i < j → t i = t' i) → walkSeq nx t a j = walkSeq nx t' a j
  | 0, _ => rfl
  | j + 1, h => by
    show nx (t j) (walkSeq nx t a j) = nx (t' j) (walkSeq nx t' a j)
    rw [h j (by omega), walkSeq_congr a j fun i hi => h i (by omega)]

theorem walkSeq_shift (t : Nat → σ) (a : γ) (c : Nat) :
    ∀ j, walkSeq nx t a (c + j) = walkSeq nx (fun i => t (c + i)) (walkSeq nx t a c) j
  | 0 => rfl
  | j + 1 => by
    show nx (t (c + j)) (walkSeq nx t a (c + j)) = _
    rw [walkSeq_shift t a c j]; rfl

theorem walkSeq_catT_le {t₁ t₂ : Nat → σ} {n₁ : Nat} (a : γ) {j : Nat} (hj : j ≤ n₁) :
    walkSeq nx (catT t₁ n₁ t₂) a j = walkSeq nx t₁ a j :=
  walkSeq_congr nx a j fun i hi => ite_eq_left (by omega)

theorem walkSeq_catT_add (t₁ t₂ : Nat → σ) (n₁ : Nat) (a : γ) (j : Nat) :
    walkSeq nx (catT t₁ n₁ t₂) a (n₁ + j) = walkSeq nx t₂ (walkSeq nx t₁ a n₁) j := by
  rw [walkSeq_shift, walkSeq_catT_le nx a (Nat.le_refl _)]
  exact walkSeq_congr nx _ j fun i _ => by
    show (if n₁ + i < n₁ then _ else t₂ (n₁ + i - n₁)) = t₂ i
    rw [ite_eq_right (by omega), show n₁ + i - n₁ = i by omega]

theorem catT_add (t₁ t₂ : Nat → σ) (n₁ j : Nat) : catT t₁ n₁ t₂ (n₁ + j) = t₂ j := by
  show (if n₁ + j < n₁ then _ else _) = _
  rw [ite_eq_right (by omega), show n₁ + j - n₁ = j by omega]

theorem catT_self (t₁ t₂ : Nat → σ) (n₁ : Nat) : catT t₁ n₁ t₂ n₁ = t₂ 0 := by
  show (if n₁ < n₁ then _ else _) = _
  rw [ite_eq_right (Nat.lt_irrefl _), Nat.sub_self]

theorem catT_lt {t₁ t₂ : Nat → σ} {n₁ j : Nat} (h : j < n₁) : catT t₁ n₁ t₂ j = t₁ j :=
  ite_eq_left h

/-- A walk whose every edge is allowed (`ok`) is a path of any relation that holds of an
allowed edge, between any two of its states. -/
theorem walkSeq_star {E : γ → γ → Prop} {ok : γ → σ → Prop} (hE : ∀ a t, ok a t → E a (nx t a))
    {t : Nat → σ} {a : γ} {n : Nat} (hw : ∀ j, j < n → ok (walkSeq nx t a j) (t j)) :
    ∀ i j, i ≤ j → j ≤ n → Star E (walkSeq nx t a i) (walkSeq nx t a j) := by
  intro i j hij hjn
  induction j with
  | zero => have : i = 0 := by omega
            subst this; exact .refl _
  | succ j ih =>
    by_cases hi : i = j + 1
    · subst hi; exact .refl _
    · exact .tail (ih (by omega) (by omega)) (hE _ _ (hw j (by omega)))

/-- A path of a relation whose every step is an allowed edge is a walk (`d` fills the
successors past its end). -/
theorem star_walkSeq {E : γ → γ → Prop} {ok : γ → σ → Prop}
    (hE : ∀ a b, E a b → ∃ t, ok a t ∧ b = nx t a) (d : σ) {a b : γ} (h : Star E a b) :
    ∃ t n, (∀ j, j < n → ok (walkSeq nx t a j) (t j)) ∧ walkSeq nx t a n = b := by
  induction h with
  | refl => exact ⟨fun _ => d, 0, fun j hj => absurd hj (Nat.not_lt_zero _), rfl⟩
  | @tail b c _ he ih =>
    obtain ⟨t, n, hw, hn⟩ := ih
    obtain ⟨t₀, hok, rfl⟩ := hE b c he
    refine ⟨catT t n (fun _ => t₀), n + 1, fun j hj => ?_, ?_⟩
    · by_cases hjn : j < n
      · rw [walkSeq_catT_le nx a (by omega), catT_lt hjn]; exact hw j hjn
      · have : j = n := by omega
        subst this
        rw [walkSeq_catT_le nx a (Nat.le_refl _), hn, catT_self]; exact hok
    · show nx (catT t n (fun _ => t₀) n) (walkSeq nx (catT t n (fun _ => t₀)) a n) = _
      rw [walkSeq_catT_le nx a (Nat.le_refl _), hn, catT_self]

end WalkSeq

/-! ## Undirected connectivity -/

/-- `a` and `c` are joined by a chain of `R` edges, each read in either direction. -/
inductive Conn {γ : Type} (R : γ → γ → Prop) : γ → γ → Prop
  | refl (a : γ) : Conn R a a
  | fwd {a b c : γ} : R a b → Conn R b c → Conn R a c
  | bwd {a b c : γ} : R b a → Conn R b c → Conn R a c

theorem Star.conn {γ : Type} {R : γ → γ → Prop} {a b : γ} (h : Star R a b) : Conn R a b := by
  have key : ∀ c, Conn R b c → Conn R a c := by
    induction h with
    | refl => exact fun _ h => h
    | tail _ he ih => exact fun c hc => ih c (.fwd he hc)
  exact key b (.refl b)

theorem Conn.trans {γ : Type} {R : γ → γ → Prop} {a b c : γ} (h₁ : Conn R a b)
    (h₂ : Conn R b c) : Conn R a c := by
  induction h₁ with
  | refl => exact h₂
  | fwd he _ ih => exact .fwd he (ih h₂)
  | bwd he _ ih => exact .bwd he (ih h₂)

/-- `f` applied to `f x` `k` times is `f` applied `k + 1` times to `x`. -/
theorem iter_succ' {γ : Type} (f : γ → γ) (k : Nat) (x : γ) :
    iter f k (f x) = iter f (k + 1) x := by
  rw [iter_add f k 1 x]; rfl

/-! ## The pairs and threads of the stored graph -/

/-- The canonical form of a run up to symmetry: `can s = π(s)(s)` with `π(s) ∈ G_s`. -/
structure Canon {σ α G : Type} {M : LTS σ α} (h : Sym M G) where
  can : σ → σ
  π : σ → G
  hcan : ∀ s, can s = h.act (π s) s
  hπ : ∀ s, h.mem s (π s)

variable {σ α G Φ β : Type} {M : LTS σ α} {h : Sym M G}

/-- A real edge of the stored graph (`OrbitRun::is_real`): a step from the stored state `r`
to a concrete successor `t ≠ r`. A concrete self-loop is a stutter. -/
def REdge (M : LTS σ α) (r t : σ) : Prop := t ≠ r ∧ M.Next r t

section Pairs

variable (L : SymLive h Φ β) (K : Canon h) (goal : β → σ → Prop)

/-- The pair an edge to `t` takes `u` to (`Product::target`): `(can t, π(t)(b))`. -/
def pn (t : σ) (u : σ × β) : σ × β := (K.can t, L.actB (K.π t) u.2)

/-- The thread node an edge to `t` takes `a` to: its pair, and its instance moved by `π(t)`
(`Product::moved`). -/
def tn (t : σ) (a : (σ × β) × Φ) : (σ × β) × Φ := (pn L K t a.1, L.actF (K.π t) a.2)

/-- The pair is goal-free (`in_n`). -/
def PN (u : σ × β) : Prop := ¬ goal u.2 u.1

/-- An edge to `t` from the pair `u` is an edge of `N`: real, with both pairs goal-free. -/
def POk (u : σ × β) (t : σ) : Prop := PN goal u ∧ PN goal (pn L K t u) ∧ REdge M u.1 t

/-- An edge of the pairs inside `N` (`search`'s `adjacency`). -/
def PEdge (u v : σ × β) : Prop := ∃ t, POk L K goal u t ∧ v = pn L K t u

/-- `e` and `u` lie in one strongly connected component of the pairs in `N`. -/
def SameP (e u : σ × β) : Prop := Star (PEdge L K goal) e u ∧ Star (PEdge L K goal) u e

/-- An edge to `t` from `u` inside the component of `e`. -/
def ZOk (e u : σ × β) (t : σ) : Prop :=
  SameP L K goal e u ∧ SameP L K goal e (pn L K t u) ∧ POk L K goal u t

/-- A thread edge inside the component of `e`: the pair moves along an edge of the
component, and the instance with it. -/
def TEdge (e : σ × β) (a b : (σ × β) × Φ) : Prop :=
  ∃ t, ZOk L K goal e a.1 t ∧ b = tn L K t a

/-- A thread node is covered: its instance is not enabled at its stored state, or an edge of
the component takes it (D20, by the concrete step from the stored state to `t`). -/
def Covered (e : σ × β) (a : (σ × β) × Φ) : Prop :=
  ¬ M.Enabled L.inst a.2 a.1.1 ∨ ∃ t, ZOk L K goal e a.1 t ∧ M.Takes L.inst a.2 a.1.1 t

/-- The component has a cycle (`candidate`): a second pair, or a pair self-edge
(`self_loop`, an orbit self-edge that keeps the binding). -/
def Candidate (e : σ × β) : Prop :=
  (∃ v, v ≠ e ∧ SameP L K goal e v) ∨ PEdge L K goal e e

/-- The component of `e` admits a fair cycle (`fair_cycle`): `e` is a stored pair in `N`,
the component has a cycle, and every piece of its threads holds a covered node. -/
def FairPComp (e : σ × β) : Prop :=
  (quotient M K.can).Reach e.1 ∧ PN goal e ∧ Candidate L K goal e ∧
    ∀ a : (σ × β) × Φ, SameP L K goal e a.1 →
      ∃ z, Conn (TEdge L K goal e) a z ∧ Covered L K goal e z

/-- A stop pair (`stops`). -/
def PStop (stop : σ → Prop) (e : σ × β) : Prop :=
  PN goal e ∧ (∀ x, ¬ M.Enabled L.inst x e.1) ∧ stop e.1

/-- The orbit search accepts (`search.start` is found): a stored pair where the trigger
holds and the goal fails reaches a stop pair or a pair of a fair component inside `N`. -/
def OAccepts (stop : σ → Prop) (trig : β → σ → Prop) : Prop :=
  ∃ s e, (quotient M K.can).Reach s.1 ∧ trig s.2 s.1 ∧ PN goal s ∧
    Star (PEdge L K goal) s e ∧ (PStop L goal stop e ∨ FairPComp L K goal e)

variable {L K goal}

theorem fst_walkSeq_tn (t : Nat → σ) (a : (σ × β) × Φ) :
    ∀ j, (walkSeq (tn L K) t a j).1 = walkSeq (pn L K) t a.1 j
  | 0 => rfl
  | j + 1 => by
    show pn L K (t j) (walkSeq (tn L K) t a j).1 = _
    rw [fst_walkSeq_tn t a j]; rfl

theorem snd_walkSeq_tn (t : Nat → σ) (a : (σ × β) × Φ) :
    ∀ j, (walkSeq (tn L K) t a j).2 = orbitSeq L.actF (fun i => K.π (t i)) a.2 j
  | 0 => rfl
  | j + 1 => by
    show L.actF (K.π (t j)) (walkSeq (tn L K) t a j).2 = _
    rw [snd_walkSeq_tn t a j]; rfl

theorem walkSeq_tn (t : Nat → σ) (u : σ × β) (x : Φ) (j : Nat) :
    walkSeq (tn L K) t (u, x) j =
      (walkSeq (pn L K) t u j, orbitSeq L.actF (fun i => K.π (t i)) x j) :=
  Prod.ext (fst_walkSeq_tn t _ j) (snd_walkSeq_tn t _ j)

/-- The group of the stored state can only grow along an edge (a slot leaves `absent`
only). -/
theorem mem_pn {r t : σ} {g : G} (hr : REdge M r t) (hg : h.mem r g) : h.mem (K.can t) g := by
  obtain ⟨a, ha⟩ := hr.2
  rw [K.hcan]; exact (h.mem_act (K.hπ t)).2 (h.mem_step ha hg)

/-- `π(t)` lies in the group of `can t`. -/
theorem perm_mem (t : σ) : h.mem (K.can t) (K.π t) := by
  rw [K.hcan]; exact (h.mem_act (K.hπ t)).2 (K.hπ t)

theorem walk_mem {t : Nat → σ} {u : σ × β} {n : Nat}
    (hw : ∀ j, j < n → REdge M (walkSeq (pn L K) t u j).1 (t j)) {g : G} {i : Nat}
    (hg : h.mem (walkSeq (pn L K) t u i).1 g) : ∀ j, i ≤ j → j ≤ n →
      h.mem (walkSeq (pn L K) t u j).1 g := by
  intro j hij hjn
  induction j with
  | zero =>
    have : i = 0 := by omega
    subst this; exact hg
  | succ j ih =>
    by_cases hi : i = j + 1
    · subst hi; exact hg
    · exact mem_pn (hw j (by omega)) (ih (by omega) (by omega))

theorem SameP.extend {e u v : σ × β} (hu : SameP L K goal e u)
    (h₁ : Star (PEdge L K goal) u v) (h₂ : Star (PEdge L K goal) v u) : SameP L K goal e v :=
  ⟨hu.1.trans h₁, h₂.trans hu.2⟩

theorem SameP.goalFree {e u : σ × β} (hu : SameP L K goal e u) (he : PN goal e) :
    PN goal u :=
  hu.1.keep (P := PN goal) (fun _ _ ⟨_, hok, hv⟩ => hv ▸ hok.2.1) he

/-- **A thread edge can be walked back.** Inside a component, the edge from `a` to `b` and a
path of pairs back from `b` to `a` close a walk whose composite permutation `P` lies in the
group of `a`'s stored state. `P` has a power `d > 0` that acts as the identity on fair
instances, and `d` rounds of the walk bring the thread back to `a` from `b`. So a piece of
the thread graph is strongly connected. -/
theorem TEdge.reverse (hG : FiniteGroups h) {e : σ × β} {a b : (σ × β) × Φ}
    (hab : TEdge L K goal e a b) : Star (TEdge L K goal e) b a := by
  obtain ⟨t₀, ⟨hsa, hsb, hok⟩, rfl⟩ := hab
  -- A path of pairs from `b`'s pair back to `a`'s.
  obtain ⟨t₁, n₁, hw₁, hn₁⟩ := star_walkSeq (pn L K) (ok := POk L K goal)
    (fun _ _ h => h) a.1.1 (hsb.2.trans hsa.1)
  let tZ := catT (fun _ => t₀) 1 t₁
  let nZ := 1 + n₁
  let W := walkSeq (pn L K) tZ a.1
  have hW1 : ∀ j, W (1 + j) = walkSeq (pn L K) t₁ (pn L K t₀ a.1) j := fun j => by
    show walkSeq (pn L K) (catT (fun _ => t₀) 1 t₁) a.1 (1 + j) = _
    rw [walkSeq_catT_add]; rfl
  have hWn : walkSeq (pn L K) tZ a.1 nZ = a.1 := (hW1 n₁).trans hn₁
  have hst := walkSeq_star (pn L K) (E := PEdge L K goal) (fun _ _ h => ⟨_, h, rfl⟩) hw₁
  have hin : ∀ k, k ≤ n₁ → SameP L K goal e (walkSeq (pn L K) t₁ (pn L K t₀ a.1) k) := by
    intro k hk
    refine hsb.extend (hst 0 k (Nat.zero_le _) hk) ?_
    have := hst k n₁ hk (Nat.le_refl _)
    rw [hn₁] at this
    exact this.trans (hsa.2.trans hsb.1)
  -- Every edge of the closed walk lies inside the component.
  have hZ : ∀ j, j < nZ → ZOk L K goal e (W j) (tZ j) := by
    intro j hj
    by_cases hj0 : j = 0
    · subst hj0
      show ZOk L K goal e a.1 (catT (fun _ => t₀) 1 t₁ 0)
      rw [catT_lt (by omega)]; exact ⟨hsa, hsb, hok⟩
    · obtain ⟨i, rfl⟩ : ∃ i, j = 1 + i := ⟨j - 1, by omega⟩
      show ZOk L K goal e (W (1 + i)) (catT (fun _ => t₀) 1 t₁ (1 + i))
      rw [catT_add, hW1]
      exact ⟨hin i (by omega), hin (i + 1) (by omega), hw₁ i (by omega)⟩
  -- The composite permutation of the closed walk lies in the group of `a`'s stored state.
  have hrW : ∀ j, j < nZ → REdge M (W j).1 (tZ j) := fun j hj => (hZ j hj).2.2.2.2
  have hperm : ∀ k, k < nZ → h.mem a.1.1 (K.π (tZ k)) := by
    intro k hk
    have := walk_mem hrW (i := k + 1) (perm_mem (K := K) (tZ k)) nZ (by omega) (Nat.le_refl _)
    rw [hWn] at this; exact this
  have hP : ∀ i, i < nZ → h.mem a.1.1 (cmp h (fun k => K.π (tZ k)) 0 i) := by
    intro i
    induction i with
    | zero => exact fun _ => hperm 0 (by omega)
    | succ i ih => exact fun hi => h.mul_mem (hperm _ (by omega)) (ih (by omega))
  let P := cmp h (fun k => K.π (tZ k)) 0 (nZ - 1)
  have hTr : ∀ x, orbitSeq L.actF (fun k => K.π (tZ k)) x nZ = L.actF P x := by
    intro x
    have := moved_by_cmp (h := h) L.actF L.actF_mul
      (y := orbitSeq L.actF (fun k => K.π (tZ k)) x) (fun _ => rfl) 0 (nZ - 1)
    rwa [show 0 + (nZ - 1) + 1 = nZ by omega] at this
  obtain ⟨d, hd, hord⟩ := order_exists hG (hP (nZ - 1) (by omega))
  have hFinj : ∀ y z, L.actF P y = L.actF P z → y = z := fun y z e => by
    have := congrArg (L.actF (h.inv P)) e; rwa [L.actF_inv, L.actF_inv] at this
  have hid : ∀ x, iter (L.actF P) d x = x := hord L.actF L.actF_mul hFinj
  -- `k` rounds of the closed walk.
  let tpow : Nat → Nat → σ := fun k => Nat.rec (fun _ => t₀) (fun _ tk => catT tZ nZ tk) k
  let lenK : Nat → Nat := fun k => Nat.rec 0 (fun _ l => nZ + l) k
  have hround : ∀ k, (∀ j, j < lenK k → ZOk L K goal e (walkSeq (pn L K) (tpow k) a.1 j)
      (tpow k j)) ∧ ∀ x, walkSeq (tn L K) (tpow k) (a.1, x) (lenK k) =
        (a.1, iter (L.actF P) k x) := by
    intro k
    induction k with
    | zero => exact ⟨fun j hj => absurd hj (Nat.not_lt_zero _), fun x => rfl⟩
    | succ k ih =>
      obtain ⟨ih1, ih2⟩ := ih
      have ht : tpow (k + 1) = catT tZ nZ (tpow k) := rfl
      have hl : lenK (k + 1) = nZ + lenK k := rfl
      refine ⟨fun j hj => ?_, fun x => ?_⟩
      · rw [ht]
        by_cases hjn : j < nZ
        · rw [walkSeq_catT_le _ _ (by omega), catT_lt hjn]; exact hZ j hjn
        · obtain ⟨i, rfl⟩ : ∃ i, j = nZ + i := ⟨j - nZ, by omega⟩
          rw [walkSeq_catT_add, catT_add]
          show ZOk L K goal e (walkSeq (pn L K) (tpow k) (walkSeq (pn L K) tZ a.1 nZ) i) _
          rw [hWn]; exact ih1 i (by omega)
      · rw [ht, hl, walkSeq_catT_add, walkSeq_tn (t := tZ), hTr, hWn, ih2, iter_succ']
  obtain ⟨hr1, hr2⟩ := hround d
  obtain ⟨d', rfl⟩ : ∃ d', d = d' + 1 := ⟨d - 1, by omega⟩
  have hlen : 1 ≤ lenK (d' + 1) := by show 1 ≤ nZ + lenK d'; omega
  have hb : walkSeq (tn L K) (tpow (d' + 1)) a 1 = tn L K t₀ a := by
    show walkSeq (tn L K) (catT tZ nZ (tpow d')) a 1 = _
    rw [walkSeq_catT_le _ _ (by omega)]
    show tn L K (catT (fun _ => t₀) 1 t₁ 0) a = _
    rw [catT_lt (by omega)]
  have hend : walkSeq (tn L K) (tpow (d' + 1)) a (lenK (d' + 1)) = a := by
    have := hr2 a.2; rw [hid] at this; exact this
  have hstar := walkSeq_star (tn L K) (E := TEdge L K goal e)
    (ok := fun c t => ZOk L K goal e c.1 t) (fun _ _ h => ⟨_, h, rfl⟩) (t := tpow (d' + 1))
    (a := a) (n := lenK (d' + 1))
    (fun j hj => by rw [fst_walkSeq_tn]; exact hr1 j hj) 1 (lenK (d' + 1)) hlen (Nat.le_refl _)
  rwa [hb, hend] at hstar

/-- **Pieces are strongly connected.** A chain of thread edges read in either direction is a
path of thread edges. -/
theorem Conn.star (hG : FiniteGroups h) {e : σ × β} {a c : (σ × β) × Φ}
    (hc : Conn (TEdge L K goal e) a c) : Star (TEdge L K goal e) a c := by
  induction hc with
  | refl => exact .refl _
  | fwd he _ ih => exact (Star.single he).trans ih
  | bwd he _ ih => exact (he.reverse hG).trans ih

end Pairs

/-! ## (⇒) A fair violating lasso of `M` makes the orbit search accept -/

open Classical in
/-- The frames of the stored run over a concrete sequence, kept on a stutter: `Θ 0 = π(c 0)`,
and a move to `c (n+1)` multiplies the frame by `π` of the image of `c (n+1)`, as the run
stores `can` of each successor. -/
noncomputable def skipFrame (h : Sym M G) (K : Canon h) (c : Nat → σ) : Nat → G
  | 0 => K.π (c 0)
  | n + 1 => if c (n + 1) = c n then skipFrame h K c n
      else h.mul (K.π (h.act (skipFrame h K c n) (c (n + 1)))) (skipFrame h K c n)

section Complete

variable {L : SymLive h Φ β} {K : Canon h} {goal trig : β → σ → Prop} {stop : σ → Prop}
  {after : β → σ → Prop}

open Classical in
theorem skipFrame_succ (c : Nat → σ) (n : Nat) : skipFrame h K c (n + 1) =
    if c (n + 1) = c n then skipFrame h K c n
    else h.mul (K.π (h.act (skipFrame h K c n) (c (n + 1)))) (skipFrame h K c n) := rfl

theorem skipFrame_stut {c : Nat → σ} {n : Nat} (he : c (n + 1) = c n) :
    skipFrame h K c (n + 1) = skipFrame h K c n := by
  rw [skipFrame_succ, ite_eq_left he]

theorem skipFrame_move {c : Nat → σ} {n : Nat} (hne : c (n + 1) ≠ c n) :
    skipFrame h K c (n + 1) =
      h.mul (K.π (h.act (skipFrame h K c n) (c (n + 1)))) (skipFrame h K c n) := by
  rw [skipFrame_succ, ite_eq_right hne]

theorem skipFrame_mem {c : Nat → σ} (hb : M.Behaviour c) :
    ∀ n, h.mem (c n) (skipFrame h K c n)
  | 0 => K.hπ _
  | n + 1 => by
    have hmt : h.mem (c (n + 1)) (skipFrame h K c n) :=
      h.mem_step_or (hb.2 n) (skipFrame_mem hb n)
    by_cases he : c (n + 1) = c n
    · rw [skipFrame_stut he]; exact hmt
    · rw [skipFrame_move he]; exact h.mul_mem ((h.mem_act hmt).1 (K.hπ _)) hmt

/-- **N9 up to symmetry, (⇒).** A concrete lasso that satisfies `Fair_P`, stays forever only
where `stop` holds, and violates the property for a binding `b₀` makes the orbit search
accept. Its image under the frames `skipFrame` is a sequence of stored pairs that steps by
pair edges and repeats (the frames at the lasso's base state lie in one finite group); from
the trigger it stays in `N`. If the lasso settles at a state, the image settles at a stop
pair. Otherwise the pairs it repeats lie in one component with a cycle, and a piece of the
threads with no covered node would hold a thread of the lasso that is always enabled and
never taken. -/
theorem oAccepts_of_lasso (he : LiveEqui h L trig goal stop after) (hG : FiniteGroups h)
    {b₀ : β} {c : Nat → σ} (hb : M.Behaviour c) (hl : Lasso c) (hf : M.FairP L.inst c)
    (hs : StopsOnly stop c) (hv : Violates (trig b₀) (goal b₀) c) :
    OAccepts L K goal stop trig := by
  classical
  let Θ := skipFrame h K c
  have hmem : ∀ n, h.mem (c n) (Θ n) := skipFrame_mem hb
  let Pr : Nat → σ × β := fun n => (h.act (Θ n) (c n), L.actB (Θ n) b₀)
  have hstut : ∀ n, c (n + 1) = c n → Pr (n + 1) = Pr n := by
    intro n he'
    show (h.act (skipFrame h K c (n + 1)) (c (n + 1)), L.actB (skipFrame h K c (n + 1)) b₀) = _
    rw [skipFrame_stut he', he']
  have hmove : ∀ n, c (n + 1) ≠ c n →
      REdge M (Pr n).1 (h.act (Θ n) (c (n + 1))) ∧
        Pr (n + 1) = pn L K (h.act (Θ n) (c (n + 1))) (Pr n) ∧
        Θ (n + 1) = h.mul (K.π (h.act (Θ n) (c (n + 1)))) (Θ n) := by
    intro n hne
    have hΘ := skipFrame_move (K := K) hne
    have hnx : M.Next (c n) (c (n + 1)) := (hb.2 n).resolve_left hne
    refine ⟨⟨fun e => hne (h.act_inj e), h.next_act hnx (hmem n)⟩, ?_, hΘ⟩
    show (h.act (skipFrame h K c (n + 1)) (c (n + 1)), L.actB (skipFrame h K c (n + 1)) b₀) = _
    rw [hΘ, h.act_mul, L.actB_mul, ← K.hcan]; rfl
  -- The stored states are reachable in the quotient.
  have hreach : ∀ n, (quotient M K.can).Reach (Pr n).1 := by
    intro n
    induction n with
    | zero => exact Reach.init ⟨c 0, hb.1, (K.hcan _).symm⟩
    | succ n ih =>
      by_cases he' : c (n + 1) = c n
      · rw [hstut n he']; exact ih
      · obtain ⟨⟨_, a, ha⟩, h2, _⟩ := hmove n he'
        rw [h2]; exact Reach.step ih ⟨_, ha, rfl⟩
  -- From the trigger the states are after it, and the pairs are goal-free.
  obtain ⟨i, hti, hgi⟩ := hv
  have hafter : ∀ j, i ≤ j → after b₀ (c j) := by
    intro j hj
    obtain ⟨d, rfl⟩ : ∃ d, j = i + d := ⟨j - i, by omega⟩
    clear hj
    induction d with
    | zero => exact he.trig_after b₀ _ hti
    | succ d ih =>
      show after b₀ (c (i + d + 1))
      rcases hb.2 (i + d) with e | hn
      · rw [e]; exact ih
      · exact he.after_next b₀ _ _ ih hn
  have hPN : ∀ j, i ≤ j → PN goal (Pr j) := fun j hj hg =>
    hgi j hj ((he.goal_act b₀ (hmem j) (hafter j hj)).1 hg)
  have htrig : trig (Pr i).2 (Pr i).1 := (he.trig_act b₀ (hmem i)).2 hti
  -- The image steps by pair edges.
  have hpath : ∀ x, i ≤ x → ∀ d, Star (PEdge L K goal) (Pr x) (Pr (x + d)) := by
    intro x hx d
    induction d with
    | zero => exact .refl _
    | succ d ih =>
      rw [show x + (d + 1) = x + d + 1 by omega]
      by_cases he' : c (x + d + 1) = c (x + d)
      · rw [hstut _ he']; exact ih
      · obtain ⟨h1, h2, _⟩ := hmove _ he'
        refine .tail ih ⟨_, ⟨hPN _ (by omega), ?_, h1⟩, h2⟩
        rw [← h2]; exact hPN _ (by omega)
  have hto : ∀ x y, i ≤ x → x ≤ y → Star (PEdge L K goal) (Pr x) (Pr y) := by
    intro x y hx hxy
    have := hpath x hx (y - x); rwa [show x + (y - x) = y by omega] at this
  -- The image repeats: the frames at the base state of the lasso lie in one finite group.
  obtain ⟨k, q, hq, hc⟩ := hl
  have hcj := periodic_mul hc
  obtain ⟨j₁, j₂, hj, hje⟩ := repeats_in hG (c k) (fun j => Θ (k + j * q)) (fun j => by
    have := hmem (k + j * q); rwa [hcj j k (Nat.le_refl _)] at this)
  let a := k + j₁ * q
  let D := (j₂ - j₁) * q
  have hD : 0 < D := Nat.mul_pos (by omega) hq
  have hcD : ∀ n, k ≤ n → c (n + D) = c n := fun n hn => hcj _ n hn
  let F : Nat → G → G := fun n g =>
    if c (n + 1) = c n then g else h.mul (K.π (h.act g (c (n + 1)))) g
  have hΘD : ∀ n, a ≤ n → Θ (n + D) = Θ n := by
    refine periodic_of_recurrence (F := F) (fun n => skipFrame_succ (h := h) (K := K) c n) (fun n hn => ?_) ?_
    · funext g
      show (if c (n + D + 1) = c (n + D) then g else _) = (if c (n + 1) = c n then g else _)
      rw [show n + D + 1 = (n + 1) + D by omega, hcD (n + 1) (by omega), hcD n (by omega)]
    · show Θ (k + j₁ * q + (j₂ - j₁) * q) = Θ (k + j₁ * q)
      rw [show k + j₁ * q + (j₂ - j₁) * q = k + j₂ * q by
        rw [Nat.add_assoc, ← Nat.add_mul]; congr 2; omega]
      exact hje
  let A := a + i
  have hPD : ∀ n, A ≤ n → Pr (n + D) = Pr n := by
    intro n hn
    show (h.act (Θ (n + D)) (c (n + D)), L.actB (Θ (n + D)) b₀) = _
    rw [hΘD n (by omega), hcD n (by omega)]
  have hcA : ∀ n, A ≤ n → c (n + D) = c n := fun n hn => hcD n (by omega)
  have hback : ∀ n, A ≤ n → Star (PEdge L K goal) (Pr n) (Pr A) := by
    intro n hn
    have hle : n - A ≤ (n - A) * D := Nat.le_mul_of_pos_right _ hD
    have := hto n (A + (n - A) * D) (by omega) (by omega)
    rwa [periodic_mul hPD (n - A) A (Nat.le_refl _)] at this
  have hcomp : ∀ n, A ≤ n → SameP L K goal (Pr A) (Pr n) := fun n hn =>
    ⟨hto A n (by omega) hn, hback n hn⟩
  refine ⟨Pr i, Pr A, hreach i, htrig, hPN i (Nat.le_refl _), hto i A (Nat.le_refl _)
    (by omega), ?_⟩
  by_cases hconst : ∀ n, A ≤ n → c n = c A
  · -- The lasso settles at `c A`: a stop pair.
    refine .inl ⟨hPN A (by omega), fun x hx => ?_, ?_⟩
    · let f := L.actF (h.inv (Θ A)) x
      have hfx : L.actF (Θ A) f = x := L.actF_inv' _ _
      have hen : M.Enabled L.inst f (c A) := by
        rw [← hfx] at hx; exact (enabled_act L (hmem A)).1 hx
      obtain ⟨n, hn, htk⟩ := hf f ⟨A, fun n hn => by
        show M.Enabled L.inst f (c n); rw [hconst n hn]; exact hen⟩ A
      exact htk.1 (by rw [hconst n hn, hconst (n + 1) (by omega)])
    · show stop (h.act (Θ A) (c A))
      rw [he.stop_act (hmem A)]; exact hs A (c A) hconst
  · -- The lasso keeps moving: the component of `Pr A` is fair.
    have hmv : ∃ n, A ≤ n ∧ c (n + 1) ≠ c n := by
      refine Classical.byContradiction fun hno => hconst ?_
      exact const_of_stutter fun n hn =>
        Classical.byContradiction fun hne => hno ⟨n, hn, hne⟩
    obtain ⟨n, hn, hne⟩ := hmv
    obtain ⟨h1, h2, _⟩ := hmove n hne
    have hedge : PEdge L K goal (Pr n) (Pr (n + 1)) :=
      ⟨_, ⟨hPN n (by omega), by rw [← h2]; exact hPN _ (by omega), h1⟩, h2⟩
    refine .inr ⟨hreach A, hPN A (by omega), ?_, fun a₀ ha₀ => ?_⟩
    · by_cases h1A : Pr n = Pr A
      · by_cases h2A : Pr (n + 1) = Pr A
        · rw [h1A, h2A] at hedge; exact .inr hedge
        · exact .inl ⟨_, h2A, hcomp (n + 1) (by omega)⟩
      · exact .inl ⟨_, h1A, hcomp n hn⟩
    · refine Classical.byContradiction fun hno => ?_
      -- The thread of `a₀` reaches the lasso's pair `Pr A`.
      have hlift : ∀ w, Star (PEdge L K goal) a₀.1 w → Star (PEdge L K goal) w (Pr A) →
          ∃ y, Star (TEdge L K goal (Pr A)) a₀ (w, y) := by
        intro w hw
        induction hw with
        | refl => exact fun _ => ⟨a₀.2, .refl _⟩
        | @tail w₁ w₂ hp hstep ih =>
          intro hw₂
          obtain ⟨y, hy⟩ := ih (Star.head hstep hw₂)
          obtain ⟨t, hok, rfl⟩ := hstep
          refine ⟨L.actF (K.π t) y, .tail hy ⟨t, ⟨?_, ?_, hok⟩, rfl⟩⟩
          · exact ha₀.extend hp ((Star.head ⟨t, hok, rfl⟩ hw₂).trans ha₀.1)
          · exact ha₀.extend (hp.tail ⟨t, hok, rfl⟩) (hw₂.trans ha₀.1)
      obtain ⟨y₀, hy₀⟩ := hlift (Pr A) ha₀.2 (.refl _)
      let f := L.actF (h.inv (Θ A)) y₀
      let X : Nat → Φ := fun m => L.actF (Θ m) f
      have hXA : X A = y₀ := L.actF_inv' _ _
      -- The thread of `f` along the lasso stays in that piece.
      have hconn : ∀ d, Conn (TEdge L K goal (Pr A)) a₀ (Pr (A + d), X (A + d)) := by
        intro d
        induction d with
        | zero => rw [Nat.add_zero, hXA]; exact hy₀.conn
        | succ d ih =>
          rw [show A + (d + 1) = A + d + 1 by omega]
          by_cases he' : c (A + d + 1) = c (A + d)
          · have hX : X (A + d + 1) = X (A + d) := by
              show L.actF (skipFrame h K c (A + d + 1)) f = _
              rw [skipFrame_stut he']
            rw [hstut _ he', hX]; exact ih
          · obtain ⟨h1, h2, h3⟩ := hmove _ he'
            refine ih.trans (.fwd ⟨_, ⟨hcomp _ (by omega), by rw [← h2]; exact hcomp _ (by omega),
              hPN _ (by omega), by rw [← h2]; exact hPN _ (by omega), h1⟩, ?_⟩ (.refl _))
            show (Pr (A + d + 1), L.actF (Θ (A + d + 1)) f) = _
            rw [h3, L.actF_mul, h2]; rfl
      -- None of its nodes is covered, so it is enabled from `A` on and never taken.
      have hen : ∀ m, A ≤ m → M.Enabled L.inst f (c m) := by
        intro m hm
        refine Classical.byContradiction fun hdis => hno ⟨_, hconn (m - A), .inl ?_⟩
        rw [show A + (m - A) = m by omega]
        exact fun hx => hdis ((enabled_act L (hmem m)).1 hx)
      obtain ⟨m, hm, htk⟩ := hf f ⟨A, hen⟩ A
      have hne' : c (m + 1) ≠ c m := htk.1
      obtain ⟨h1, h2, _⟩ := hmove m hne'
      refine hno ⟨_, hconn (m - A), .inr ⟨h.act (Θ m) (c (m + 1)), ?_, ?_⟩⟩
      · rw [show A + (m - A) = m by omega]
        exact ⟨hcomp m hm, by rw [← h2]; exact hcomp _ (by omega), hPN m (by omega),
          by rw [← h2]; exact hPN _ (by omega), h1⟩
      · rw [show A + (m - A) = m by omega]
        exact (takes_act L (hmem m)).2 htk

/-- **N9 up to symmetry, (⇒), on behaviours.** With the reachable states in a list, a
behaviour that satisfies `Fair_P`, stays forever only where `stop` holds, and violates the
property for some binding makes the orbit search accept. -/
theorem oAccepts_of_behaviour (he : LiveEqui h L trig goal stop after) (hG : FiniteGroups h)
    (LM : List σ) (hLM : ∀ s, M.Reach s → s ∈ LM) {b₀ : β} {c : Nat → σ}
    (hb : M.Behaviour c) (hf : M.FairP L.inst c) (hs : StopsOnly stop c)
    (hv : Violates (trig b₀) (goal b₀) c) : OAccepts L K goal stop trig := by
  obtain ⟨c', hl, hb', hf', hs', hv'⟩ :=
    lasso_of_behaviour M LM hLM L.inst stop (trig b₀) (goal b₀) c hb hf hs hv
  exact oAccepts_of_lasso he hG hb' hl hf' hs' hv'

end Complete

/-! ## (⇐) An accepted pair gives a fair violating behaviour of `M` -/

theorem orbitSeq_walkSeq {γ : Type} (act : G → γ → γ) (p : Nat → G) (x : γ) :
    ∀ j, orbitSeq act p x j = walkSeq act p x j
  | 0 => rfl
  | j + 1 => by
    show act (p j) (orbitSeq act p x j) = act (p j) (walkSeq act p x j)
    rw [orbitSeq_walkSeq act p x j]

theorem orbitSeq_shift {γ : Type} (act : G → γ → γ) (p : Nat → G) (x : γ) (c j : Nat) :
    orbitSeq act p x (c + j) = orbitSeq act (fun i => p (c + i)) (orbitSeq act p x c) j := by
  rw [orbitSeq_walkSeq, orbitSeq_walkSeq, orbitSeq_walkSeq, walkSeq_shift]

theorem orbitSeq_congr {γ : Type} (act : G → γ → γ) {p q : Nat → G} (x : γ) (j : Nat)
    (hpq : ∀ i, i < j → p i = q i) : orbitSeq act p x j = orbitSeq act q x j := by
  rw [orbitSeq_walkSeq, orbitSeq_walkSeq]; exact walkSeq_congr _ _ j hpq

theorem walkSeq_map {γ : Type} (act : G → γ → γ) (g : σ → G) (t : Nat → σ) (x : γ) :
    ∀ j, walkSeq act (fun i => g (t i)) x j = walkSeq (fun s y => act (g s) y) t x j
  | 0 => rfl
  | j + 1 => by
    show act (g (t j)) (walkSeq act (fun i => g (t i)) x j) = act (g (t j)) _
    rw [walkSeq_map act g t x j]

/-- Every value is the image, after `n` permutations, of some value. -/
theorem orbitSeq_surj {γ : Type} (act : G → γ → γ) (hinv : ∀ g y, act g (act (h.inv g) y) = y)
    (P : Nat → G) : ∀ n y, ∃ x, orbitSeq act P x n = y
  | 0, y => ⟨y, rfl⟩
  | n + 1, y => by
    obtain ⟨x, hx⟩ := orbitSeq_surj act hinv P n (act (h.inv (P n)) y)
    exact ⟨x, by show act (P n) (orbitSeq act P x n) = y; rw [hx, hinv]⟩

/-- A sequence of images that closes on `[n₀, m)` is read through the fold by its index. -/
theorem orbitSeq_fold {γ : Type} (act : G → γ → γ) (P : Nat → G) (x : γ) {n₀ m : Nat}
    (hc : orbitSeq act P x m = orbitSeq act P x n₀) :
    ∀ n, orbitSeq act (fun k => P (foldIdx n₀ m k)) x n = orbitSeq act P x (foldIdx n₀ m n)
  | 0 => rfl
  | n + 1 => by
    show act (P (foldIdx n₀ m n)) (orbitSeq act (fun k => P (foldIdx n₀ m k)) x n) =
      orbitSeq act P x (if foldIdx n₀ m n + 1 = m then n₀ else foldIdx n₀ m n + 1)
    rw [orbitSeq_fold act P x hc n]
    split
    · next he =>
      have e' : orbitSeq act P x (foldIdx n₀ m n + 1) = orbitSeq act P x m := congrArg _ he
      rw [← hc, ← e']; rfl
    · rfl

section Sound

variable {L : SymLive h Φ β} {K : Canon h} {goal trig : β → σ → Prop} {stop : σ → Prop}
  {after : β → σ → Prop}

theorem snd_walkSeq_pn (t : Nat → σ) (u : σ × β) :
    ∀ j, (walkSeq (pn L K) t u j).2 = orbitSeq L.actB (fun i => K.π (t i)) u.2 j
  | 0 => rfl
  | j + 1 => by
    show L.actB (K.π (t j)) (walkSeq (pn L K) t u j).2 = _
    rw [snd_walkSeq_pn t u j]; rfl

theorem snd_walkSeq_tn' (t : Nat → σ) (a : (σ × β) × Φ) (j : Nat) :
    (walkSeq (tn L K) t a j).2 = walkSeq (fun s y => L.actF (K.π s) y) t a.2 j := by
  rw [snd_walkSeq_tn, orbitSeq_walkSeq, walkSeq_map]

/-- An instance at a stored state, with the edge to `t` from it: not enabled, or taken. -/
def Ev (L : SymLive h Φ β) (y : Φ) (r t : σ) : Prop :=
  ¬ M.Enabled L.inst y r ∨ M.Takes L.inst y r t

/-- A walk of `n` edges from `u` to `v` inside the component of `e`. -/
def ZW (L : SymLive h Φ β) (K : Canon h) (goal : β → σ → Prop) (e u : σ × β) (t : Nat → σ)
    (n : Nat) (v : σ × β) : Prop :=
  (∀ j, j < n → ZOk L K goal e (walkSeq (pn L K) t u j) (t j)) ∧ walkSeq (pn L K) t u n = v

theorem ZW.cat {e u v w : σ × β} {t₁ t₂ : Nat → σ} {n₁ n₂ : Nat}
    (h₁ : ZW L K goal e u t₁ n₁ v) (h₂ : ZW L K goal e v t₂ n₂ w) :
    ZW L K goal e u (catT t₁ n₁ t₂) (n₁ + n₂) w := by
  refine ⟨fun j hj => ?_, by rw [walkSeq_catT_add, h₁.2, h₂.2]⟩
  by_cases hjn : j < n₁
  · rw [walkSeq_catT_le _ _ (by omega), catT_lt hjn]; exact h₁.1 j hjn
  · obtain ⟨i, rfl⟩ : ∃ i, j = n₁ + i := ⟨j - n₁, by omega⟩
    rw [walkSeq_catT_add, catT_add, h₁.2]; exact h₂.1 i (by omega)

/-- A path of pairs between two pairs of the component is a walk inside it. -/
theorem zw_of_star {e u v : σ × β} (hu : SameP L K goal e u) (hv : SameP L K goal e v)
    (hp : Star (PEdge L K goal) u v) : ∃ t n, ZW L K goal e u t n v := by
  obtain ⟨t, n, hw, hn⟩ := star_walkSeq (pn L K) (ok := POk L K goal) (fun _ _ h => h) u.1 hp
  have hst := walkSeq_star (pn L K) (E := PEdge L K goal) (fun _ _ h => ⟨_, h, rfl⟩) hw
  have hin : ∀ j, j ≤ n → SameP L K goal e (walkSeq (pn L K) t u j) := by
    intro j hj
    refine hu.extend (hst 0 j (Nat.zero_le _) hj) ?_
    have := hst j n hj (Nat.le_refl _)
    rw [hn] at this
    exact this.trans (hv.2.trans hu.1)
  exact ⟨t, n, fun j hj => ⟨hin j (by omega), hin (j + 1) (by omega), hw j hj⟩, hn⟩

/-- A component with a cycle has a closed walk with at least one edge. -/
theorem Candidate.cycle {e : σ × β} (hc : Candidate L K goal e) :
    ∃ t n, 0 < n ∧ ZW L K goal e e t n e := by
  have hee : SameP L K goal e e := ⟨.refl e, .refl e⟩
  rcases hc with ⟨v, hne, hv⟩ | ⟨t₀, hok, hee'⟩
  · obtain ⟨t₁, n₁, hz₁⟩ := zw_of_star hee hv hv.1
    obtain ⟨t₂, n₂, hz₂⟩ := zw_of_star hv hee hv.2
    refine ⟨_, _, ?_, hz₁.cat hz₂⟩
    have : n₁ ≠ 0 := fun h0 => by subst h0; exact hne hz₁.2.symm
    omega
  · refine ⟨fun _ => t₀, 1, by omega, fun j hj => ?_, hee'.symm⟩
    have : j = 0 := by omega
    subst this
    exact ⟨hee, by show SameP L K goal e (pn L K t₀ e); rw [← hee']; exact hee, hok⟩

/-- **The thread decomposition.** In a fair component, from any pair `u` of it and any
instance `y`, a walk inside the component back to `e` meets a stored state where the
thread of `y` is not enabled or an edge that takes it. The piece of `(u, y)` holds a covered
node; a path of thread edges reaches it (`Conn.star`); the walk follows that path, then the
covering edge or a closed walk, then returns to `e`. -/
theorem FairPComp.cover_one (hG : FiniteGroups h) {e u : σ × β}
    (hfc : FairPComp L K goal e) (hu : SameP L K goal e u) (y : Φ) :
    ∃ t n, ZW L K goal e u t n e ∧ ∃ j, j < n ∧
      Ev L (walkSeq (fun s y => L.actF (K.π s) y) t y j) (walkSeq (pn L K) t u j).1 (t j) := by
  have hee : SameP L K goal e e := ⟨.refl e, .refl e⟩
  obtain ⟨z, hc, hcov⟩ := hfc.2.2.2 (u, y) hu
  have hst := hc.star hG
  have hz : SameP L K goal e z.1 :=
    hst.keep (P := fun a => SameP L K goal e a.1) (fun _ _ ⟨_, hok, hb⟩ => hb ▸ hok.2.1) hu
  obtain ⟨t₁, n₁, hw₁, hn₁⟩ := star_walkSeq (tn L K) (ok := fun c t => ZOk L K goal e c.1 t)
    (fun _ _ ⟨t, hok, hb⟩ => ⟨t, hok, hb⟩) u.1 hst
  have hz₁ : ZW L K goal e u t₁ n₁ z.1 :=
    ⟨fun j hj => by have := hw₁ j hj; rwa [fst_walkSeq_tn] at this,
     by rw [← hn₁, fst_walkSeq_tn]⟩
  have hy₁ : walkSeq (fun s y => L.actF (K.π s) y) t₁ y n₁ = z.2 := by
    rw [← hn₁, snd_walkSeq_tn']
  rcases hcov with hdis | ⟨t₀, hok₀, htk⟩
  · obtain ⟨t₂, n₂, hz₂⟩ := zw_of_star hz hee hz.2
    obtain ⟨t₃, n₃, hn₃, hz₃⟩ := hfc.2.2.1.cycle
    refine ⟨_, _, hz₁.cat (hz₂.cat hz₃), n₁, by omega, .inl ?_⟩
    rw [walkSeq_catT_le _ _ (Nat.le_refl _), walkSeq_catT_le _ _ (Nat.le_refl _), hy₁, hz₁.2]
    exact hdis
  · obtain ⟨t₂, n₂, hz₂⟩ := zw_of_star hok₀.2.1 hee hok₀.2.1.2
    have hz₀ : ZW L K goal e z.1 (fun _ => t₀) 1 (pn L K t₀ z.1) := by
      refine ⟨fun j hj => ?_, rfl⟩
      have : j = 0 := by omega
      subst this; exact hok₀
    refine ⟨_, _, hz₁.cat (hz₀.cat hz₂), n₁, by omega, .inr ?_⟩
    rw [walkSeq_catT_le _ _ (Nat.le_refl _), walkSeq_catT_le _ _ (Nat.le_refl _), hy₁, hz₁.2,
      catT_self, catT_lt (by omega)]
    exact htk

/-- **The covering loop.** A fair component has a closed walk at `e`, with at least one edge,
on which the thread of every instance enabled at `e`'s stored state meets a state where it
is not enabled or an edge that takes it: one walk of `cover_one` per instance of a list,
each from where the previous ones left that instance's thread. -/
theorem FairPComp.cover (hG : FiniteGroups h) {e : σ × β} (hfc : FairPComp L K goal e)
    (LΦ : List Φ) (hLΦ : ∀ x, M.Enabled L.inst x e.1 → x ∈ LΦ) :
    ∃ t n, 0 < n ∧ ZW L K goal e e t n e ∧ ∀ x, M.Enabled L.inst x e.1 → ∃ j, j < n ∧
      Ev L (walkSeq (fun s y => L.actF (K.π s) y) t x j) (walkSeq (pn L K) t e j).1 (t j) := by
  have hee : SameP L K goal e e := ⟨.refl e, .refl e⟩
  have main : ∀ T : List Φ, ∃ t n, 0 < n ∧ ZW L K goal e e t n e ∧ ∀ x, x ∈ T → ∃ j, j < n ∧
      Ev L (walkSeq (fun s y => L.actF (K.π s) y) t x j) (walkSeq (pn L K) t e j).1 (t j) := by
    intro T
    induction T with
    | nil =>
      obtain ⟨t, n, hn, hz⟩ := hfc.2.2.1.cycle
      exact ⟨t, n, hn, hz, fun _ h => nomatch h⟩
    | cons x T ih =>
      obtain ⟨t, n, hn, hz, hT⟩ := ih
      obtain ⟨t', n', hz', j', hj', hev⟩ :=
        hfc.cover_one hG hee (walkSeq (fun s y => L.actF (K.π s) y) t x n)
      refine ⟨_, _, by omega, hz.cat hz', fun x' hx' => ?_⟩
      rcases List.mem_cons.mp hx' with rfl | hx'
      · refine ⟨n + j', by omega, ?_⟩
        rw [walkSeq_catT_add, walkSeq_catT_add, catT_add, hz.2]; exact hev
      · obtain ⟨j, hj, hev'⟩ := hT x' hx'
        refine ⟨j, by omega, ?_⟩
        rw [walkSeq_catT_le _ _ (by omega), walkSeq_catT_le _ _ (by omega), catT_lt hj]
        exact hev'
  obtain ⟨t, n, hn, hz, hT⟩ := main LΦ
  exact ⟨t, n, hn, hz, fun x hx => hT x (hLΦ x hx)⟩

/-- A finite piece of an annotated run: stutters or steps from `R j` to `T j`, stored as
`R (j+1) = P j (T j)` with `P j ∈ G_{T j}`. -/
def ASeg (h : Sym M G) (R T : Nat → σ) (P : Nat → G) (n : Nat) : Prop :=
  ∀ j, j < n → (T j = R j ∨ M.Next (R j) (T j)) ∧ h.mem (T j) (P j) ∧
    R (j + 1) = h.act (P j) (T j)


theorem ASeg.cat {R₁ T₁ R₂ T₂ : Nat → σ} {P₁ P₂ : Nat → G} {n₁ n₂ : Nat}
    (h₁ : ASeg h R₁ T₁ P₁ n₁) (h₂ : ASeg h R₂ T₂ P₂ n₂) (hj : R₁ n₁ = R₂ 0) :
    ASeg h (catW R₁ n₁ R₂) (catT T₁ n₁ T₂) (catT P₁ n₁ P₂) (n₁ + n₂) := by
  intro j hj'
  by_cases hjn : j < n₁
  · rw [catW_le (by omega), catW_le (by omega), catT_lt hjn, catT_lt hjn]; exact h₁ j hjn
  · obtain ⟨i, rfl⟩ : ∃ i, j = n₁ + i := ⟨j - n₁, by omega⟩
    rw [catW_add hj, show n₁ + i + 1 = n₁ + (i + 1) by omega, catW_add hj, catT_add, catT_add]
    exact h₂ i (by omega)

theorem aseg_of_pairs {u : σ × β} {t : Nat → σ} {n : Nat}
    (hw : ∀ j, j < n → REdge M (walkSeq (pn L K) t u j).1 (t j)) :
    ASeg h (fun j => (walkSeq (pn L K) t u j).1) t (fun j => K.π (t j)) n :=
  fun j hj => ⟨.inr (hw j hj).2, K.hπ _, K.hcan _⟩

/-- A reachable stored state is the end of an annotated piece from the canonical form of an
initial state. -/
theorem qreach_aseg {r : σ} (hr : (quotient M K.can).Reach r) :
    ∃ c₀ R T n, M.init c₀ ∧ R 0 = K.can c₀ ∧ R n = r ∧ ASeg h R T (fun j => K.π (T j)) n := by
  induction hr with
  | @init r hi =>
    obtain ⟨c₀, hc₀, rfl⟩ := hi
    exact ⟨c₀, fun _ => K.can c₀, fun _ => K.can c₀, 0, hc₀, rfl, rfl,
      fun j hj => absurd hj (Nat.not_lt_zero _)⟩
  | @step r a r' _ hst ih =>
    obtain ⟨c₀, R, T, n, hc₀, h0, hn, hs⟩ := ih
    obtain ⟨t, hrt, rfl⟩ := hst
    refine ⟨c₀, snocW R n (K.can t), catT T n (fun _ => t), n + 1, hc₀,
      by rw [snocW_le (Nat.zero_le _), h0], snocW_succ, fun j hj => ?_⟩
    dsimp only
    by_cases hjn : j < n
    · rw [snocW_le (by omega), snocW_le (by omega), catT_lt hjn]; exact hs j hjn
    · have : j = n := by omega
      subst this
      rw [snocW_le (Nat.le_refl _), snocW_succ, catT_self, hn]
      exact ⟨.inr ⟨a, hrt⟩, K.hπ t, K.hcan t⟩

/-- The fold of an annotated piece that closes on `[n₀, m)` is an annotated run. -/
theorem fold_arun {R T : Nat → σ} {P : Nat → G} {n₀ m : Nat} (hs : ASeg h R T P m)
    (hnm : n₀ < m) (hmo : R m = R n₀) (hinit : ∃ c g, M.init c ∧ h.mem c g ∧ R 0 = h.act g c) :
    ARun h (fun n => R (foldIdx n₀ m n)) (fun n => T (foldIdx n₀ m n))
      (fun n => P (foldIdx n₀ m n)) := by
  refine ⟨hinit, fun n => (hs _ (foldIdx_lt hnm n)).1, fun n => (hs _ (foldIdx_lt hnm n)).2.1,
    fun n => ?_⟩
  have := foldSeq_succ R hmo n
  show foldSeq R (n₀ := n₀) (m := m) (n + 1) = _
  rw [this]; exact (hs _ (foldIdx_lt hnm n)).2.2

/-- Along stutters the stop rule reads the same at every stored state. -/
theorem ARun.stop_stutter {r t : Nat → σ} {p : Nat → G} (ha : ARun h r t p)
    (hstop : ∀ {s g}, h.mem s g → (stop (h.act g s) ↔ stop s)) {N : Nat}
    (hst : ∀ m, N ≤ m → t m = r m) : ∀ d, stop (r (N + d)) ↔ stop (r N)
  | 0 => Iff.rfl
  | d + 1 => by
    have e1 := ha.next (N + d)
    have e2 := hst (N + d) (by omega)
    have hm : h.mem (r (N + d)) (p (N + d)) := by rw [← e2]; exact ha.mem _
    show stop (r (N + d + 1)) ↔ _
    rw [e1, e2, hstop hm]; exact ARun.stop_stutter ha hstop hst d

/-- **The counterexample's annotated run.** From a stored pair `s` where the trigger holds
and the goal fails, a path of pairs in `N` to `e`, then a cycle at `e` given as an annotated
piece whose pairs stay in `N` and whose binding comes back: an annotated run that violates
the property, from some time on runs round the cycle, and starts a round of it after any
time. -/
theorem run_of_parts {s e : σ × β} (hrs : (quotient M K.can).Reach s.1) (hts : trig s.2 s.1)
    (hps : PN goal s) {t₂ : Nat → σ} {n₂ : Nat}
    (hw₂ : ∀ j, j < n₂ → POk L K goal (walkSeq (pn L K) t₂ s j) (t₂ j))
    (he₂ : walkSeq (pn L K) t₂ s n₂ = e) {R₃ T₃ : Nat → σ} {P₃ : Nat → G} {n₃ : Nat}
    (hn₃ : 0 < n₃) (hs₃ : ASeg h R₃ T₃ P₃ n₃) (h30 : R₃ 0 = e.1) (h3n : R₃ n₃ = e.1)
    (hb₃ : orbitSeq L.actB P₃ e.2 n₃ = e.2)
    (hpn₃ : ∀ j, j < n₃ → PN goal (R₃ j, orbitSeq L.actB P₃ e.2 j)) :
    ∃ r t p b', ARun h r t p ∧ AViolates L trig goal r p b' ∧
      (∃ N₀, ∀ n, N₀ ≤ n → ∃ j, j < n₃ ∧ r n = R₃ j ∧ t n = T₃ j ∧ p n = P₃ j) ∧
      ∀ N, ∃ c₀, N ≤ c₀ ∧ ∀ j, j < n₃ → r (c₀ + j) = R₃ j ∧ t (c₀ + j) = T₃ j ∧
        p (c₀ + j) = P₃ j := by
  obtain ⟨c₀, R₁, T₁, n₁, hc₀, h10, h1n, hs₁⟩ := qreach_aseg (K := K) hrs
  let R₂ : Nat → σ := fun j => (walkSeq (pn L K) t₂ s j).1
  let P₂ : Nat → G := fun j => K.π (t₂ j)
  have hs₂ : ASeg h R₂ t₂ P₂ n₂ := aseg_of_pairs fun j hj => (hw₂ j hj).2.2
  have j23 : R₂ n₂ = R₃ 0 := by rw [h30]; show (walkSeq (pn L K) t₂ s n₂).1 = _; rw [he₂]
  have hs23 := hs₂.cat hs₃ j23
  have j12 : R₁ n₁ = catW R₂ n₂ R₃ 0 := by rw [catW_le (Nat.zero_le _), h1n]; rfl
  have hs0 := hs₁.cat hs23 j12
  let R0 := catW R₁ n₁ (catW R₂ n₂ R₃)
  let T0 := catT T₁ n₁ (catT t₂ n₂ T₃)
  let P0 := catT (fun j => K.π (T₁ j)) n₁ (catT P₂ n₂ P₃)
  let n₀ := n₁ + n₂
  let m := n₀ + n₃
  have hm : m = n₁ + (n₂ + n₃) := by omega
  have hR3 : ∀ j, R0 (n₀ + j) = R₃ j := fun j => by
    show catW R₁ n₁ _ (n₁ + n₂ + j) = _
    rw [Nat.add_assoc, catW_add j12, catW_add j23]
  have hT3 : ∀ j, T0 (n₀ + j) = T₃ j := fun j => by
    show catT T₁ n₁ _ (n₁ + n₂ + j) = _; rw [Nat.add_assoc, catT_add, catT_add]
  have hP3 : ∀ j, P0 (n₀ + j) = P₃ j := fun j => by
    show catT _ n₁ _ (n₁ + n₂ + j) = _; rw [Nat.add_assoc, catT_add, catT_add]
  have hR2 : ∀ j, j ≤ n₂ → R0 (n₁ + j) = R₂ j := fun j hj => by
    show catW R₁ n₁ _ (n₁ + j) = _; rw [catW_add j12, catW_le hj]
  have hP2 : ∀ j, j < n₂ → P0 (n₁ + j) = P₂ j := fun j hj => by
    show catT _ n₁ _ (n₁ + j) = _; rw [catT_add, catT_lt hj]
  have hnm : n₀ < m := by omega
  have hmo : R0 m = R0 n₀ := by
    show R0 (n₀ + n₃) = R0 (n₀ + 0); rw [hR3, hR3, h3n, h30]
  have hinit : ∃ c g, M.init c ∧ h.mem c g ∧ R0 0 = h.act g c :=
    ⟨c₀, K.π c₀, hc₀, K.hπ c₀, by
      show catW R₁ n₁ _ 0 = _; rw [catW_le (Nat.zero_le _), h10, K.hcan]⟩
  have ha := fold_arun (hm ▸ hs0 : ASeg h R0 T0 P0 m) hnm hmo hinit
  -- The binding: chosen so that it is `s`'s at the trigger.
  obtain ⟨b', hb'⟩ := orbitSeq_surj (h := h) L.actB L.actB_inv' P0 n₁ s.2
  have hO2 : ∀ j, j ≤ n₂ → orbitSeq L.actB P0 b' (n₁ + j) = (walkSeq (pn L K) t₂ s j).2 := by
    intro j hj
    rw [orbitSeq_shift, hb', snd_walkSeq_pn]
    exact orbitSeq_congr _ _ j fun i hi => hP2 i (by omega)
  have hO0 : orbitSeq L.actB P0 b' n₀ = e.2 := by rw [hO2 n₂ (Nat.le_refl _), he₂]
  have hO3 : ∀ j, orbitSeq L.actB P0 b' (n₀ + j) = orbitSeq L.actB P₃ e.2 j := by
    intro j
    rw [orbitSeq_shift, hO0]
    exact orbitSeq_congr _ _ j fun i _ => hP3 i
  have hclose : orbitSeq L.actB P0 b' m = orbitSeq L.actB P0 b' n₀ := by
    show orbitSeq L.actB P0 b' (n₀ + n₃) = _; rw [hO3, hb₃, hO0]
  have hfold := orbitSeq_fold L.actB P0 b' hclose
  -- The pairs from the trigger on are goal-free.
  have hpn2 : ∀ j, j ≤ n₂ → PN goal (walkSeq (pn L K) t₂ s j) := by
    intro j hj
    induction j with
    | zero => exact hps
    | succ j ih => exact (hw₂ j (by omega)).2.1
  have hpn : ∀ j, n₁ ≤ j → j < m → PN goal (R0 j, orbitSeq L.actB P0 b' j) := by
    intro j h1 h2
    by_cases hj : j ≤ n₀
    · obtain ⟨i, rfl⟩ : ∃ i, j = n₁ + i := ⟨j - n₁, by omega⟩
      rw [hR2 i (by omega), hO2 i (by omega)]; exact hpn2 i (by omega)
    · obtain ⟨i, rfl⟩ : ∃ i, j = n₀ + i := ⟨j - n₀, by omega⟩
      rw [hR3, hO3]; exact hpn₃ i (by omega)
  have hR20 : R0 n₁ = s.1 := hR2 0 (Nat.zero_le _)
  refine ⟨_, _, _, b', ha, ⟨n₁, ?_, fun j hj => ?_⟩, ⟨n₀, fun n hn => ?_⟩, fun N => ?_⟩
  · show trig (orbitSeq L.actB (fun k => P0 (foldIdx n₀ m k)) b' n₁) (R0 (foldIdx n₀ m n₁))
    rw [hfold, foldIdx_eq n₁ (by omega), hb', hR20]
    exact hts
  · show ¬ goal (orbitSeq L.actB (fun k => P0 (foldIdx n₀ m k)) b' j) (R0 (foldIdx n₀ m j))
    rw [hfold]
    refine hpn _ ?_ (foldIdx_lt hnm j)
    by_cases hjn : j < n₀
    · rw [foldIdx_eq j (by omega)]; exact hj
    · have := foldIdx_ge hnm j (by omega); omega
  · have h1 := foldIdx_ge hnm n hn
    have h2 := foldIdx_lt hnm n
    obtain ⟨i, hi⟩ : ∃ i, foldIdx n₀ m n = n₀ + i := ⟨foldIdx n₀ m n - n₀, by omega⟩
    refine ⟨i, by omega, ?_, ?_, ?_⟩
    · show R0 _ = _; rw [hi, hR3]
    · show T0 _ = _; rw [hi, hT3]
    · show P0 _ = _; rw [hi, hP3]
  · obtain ⟨d, hd, _⟩ := foldIdx_reaches hnm (max N n₀) (by omega) n₀ (Nat.le_refl _) hnm
    refine ⟨max N n₀ + d, by omega, fun j hj => ?_⟩
    have hidx : foldIdx n₀ m (max N n₀ + d + j) = n₀ + j := by
      rw [foldIdx_add _ j (by omega), hd]
    refine ⟨?_, ?_, ?_⟩
    · show R0 _ = _; rw [hidx, hR3]
    · show T0 _ = _; rw [hidx, hT3]
    · show P0 _ = _; rw [hidx, hP3]

/-- **N9 up to symmetry, (⇐).** If the orbit search accepts, some binding has a behaviour of
`M` that satisfies `Fair_P`, stays forever only where `stop` holds, and violates the
property. The accepted pair gives an annotated run (`run_of_parts`): the stored path, the
path of pairs, then a stutter at a stop pair or the covering loop of a fair component
round and round; it is thread-fair and stops only where the rule allows, and Theorem A23's
lift (`a23_lift`) carries it to `M`. -/
theorem behaviour_of_oAccepts (he : LiveEqui h L trig goal stop after) (hG : FiniteGroups h)
    (hΦ : ∀ r, ∃ LΦ : List Φ, ∀ x, M.Enabled L.inst x r → x ∈ LΦ)
    (hacc : OAccepts L K goal stop trig) :
    ∃ b c, M.Behaviour c ∧ M.FairP L.inst c ∧ StopsOnly stop c ∧
      Violates (trig b) (goal b) c := by
  obtain ⟨s, e, hrs, hts, hps, hpath, hend⟩ := hacc
  obtain ⟨t₂, n₂, hw₂, he₂⟩ :=
    star_walkSeq (pn L K) (ok := POk L K goal) (fun _ _ h => h) s.1 hpath
  suffices hrun : ∃ r t p b', ARun h r t p ∧ AFair L r t p ∧ AStops stop r t ∧
      AViolates L trig goal r p b' by
    obtain ⟨r, t, p, b', ha, hf, hs, hv⟩ := hrun
    obtain ⟨c, b, hb, hfc, hsc, hvc⟩ := a23_lift L he ha hf hs hv
    exact ⟨b, c, hb, hfc, hsc, hvc⟩
  rcases hend with ⟨hpe, hnone, hstop⟩ | hfc
  · -- Stutter at the stop pair, with a permutation that acts as the identity.
    let g := h.mul (K.π e.1) (h.inv (K.π e.1))
    have hgid : h.act g e.1 = e.1 := by rw [h.act_mul, h.act_inv']
    have hgB : L.actB g e.2 = e.2 := by rw [L.actB_mul, L.actB_inv']
    have hgm : h.mem e.1 g := h.mul_mem (K.hπ _) (h.inv_mem (K.hπ _))
    have hseg : ASeg h (fun _ => e.1) (fun _ => e.1) (fun _ => g) 1 :=
      fun _ _ => ⟨.inl rfl, hgm, hgid.symm⟩
    obtain ⟨r, t, p, b', ha, hv, ⟨N₀, hev⟩, _⟩ :=
      run_of_parts hrs hts hps hw₂ he₂ (by omega) hseg rfl rfl hgB (fun j hj => by
        have : j = 0 := by omega
        subst this; exact hpe)
    refine ⟨r, t, p, b', ha, fun x ⟨N, hN⟩ => ?_, fun N hst => ?_, hv⟩
    · exfalso
      obtain ⟨_, _, hr, _⟩ := hev (max N N₀) (by omega)
      have : M.Enabled L.inst (orbitSeq L.actF p x (max N N₀)) (r (max N N₀)) := hN _ (by omega)
      rw [hr] at this
      exact hnone _ this
    · have := ha.stop_stutter he.stop_act hst (max N N₀ - N)
      rw [show N + (max N N₀ - N) = max N N₀ by omega] at this
      obtain ⟨_, _, hr, _⟩ := hev (max N N₀) (by omega)
      rw [hr] at this
      exact this.1 hstop
  · -- Round the covering loop.
    obtain ⟨LΦ, hLΦ⟩ := hΦ e.1
    obtain ⟨t₃, n₃, hn₃, ⟨hz₃, he₃⟩, hcov⟩ := hfc.cover hG LΦ hLΦ
    have hseg := aseg_of_pairs (L := L) (K := K) (u := e) (t := t₃) (n := n₃)
      fun j hj => (hz₃ j hj).2.2.2.2
    have hpair : ∀ j, (((walkSeq (pn L K) t₃ e j : σ × β)).1,
        orbitSeq L.actB (fun i => K.π (t₃ i)) e.2 j) = walkSeq (pn L K) t₃ e j :=
      fun j => Prod.ext rfl (snd_walkSeq_pn t₃ e j).symm
    obtain ⟨r, t, p, b', ha, hv, _, hcopy⟩ :=
      run_of_parts hrs hts hps hw₂ he₂ hn₃ hseg rfl
        (by show (walkSeq (pn L K) t₃ e n₃ : σ × β).1 = e.1; rw [he₃])
        (by rw [← snd_walkSeq_pn, he₃]) (fun j hj => by rw [hpair]; exact (hz₃ j hj).2.2.1)
    refine ⟨r, t, p, b', ha, fun x ⟨N, hN⟩ N' => ?_, fun N hst => ?_, hv⟩
    · obtain ⟨c₀, hc₀, hcp⟩ := hcopy (max N N')
      let y := orbitSeq L.actF p x c₀
      have htr : ∀ j, j ≤ n₃ → orbitSeq L.actF p x (c₀ + j) =
          walkSeq (fun s y => L.actF (K.π s) y) t₃ y j := by
        intro j hj
        rw [orbitSeq_shift, orbitSeq_congr _ _ j fun i hi => (hcp i (by omega)).2.2,
          orbitSeq_walkSeq, walkSeq_map]
      have hr0 : r c₀ = e.1 := (hcp 0 hn₃).1
      have hen : M.Enabled L.inst y e.1 := by
        have : M.Enabled L.inst (orbitSeq L.actF p x c₀) (r c₀) := hN c₀ (by omega)
        rwa [hr0] at this
      obtain ⟨j, hj, hev⟩ := hcov y hen
      obtain ⟨hr, ht, _⟩ := hcp j hj
      rcases hev with hdis | htk
      · exfalso
        have : M.Enabled L.inst (orbitSeq L.actF p x (c₀ + j)) (r (c₀ + j)) := hN _ (by omega)
        rw [htr j (by omega), hr] at this
        exact hdis this
      · refine ⟨c₀ + j, by omega, ?_⟩
        show M.Takes L.inst (orbitSeq L.actF p x (c₀ + j)) (r (c₀ + j)) (t (c₀ + j))
        rw [htr j (by omega), hr, ht]; exact htk
    · exfalso
      obtain ⟨c₀, hc₀, hcp⟩ := hcopy N
      obtain ⟨hr, ht, _⟩ := hcp 0 hn₃
      have h1 := hst (c₀ + 0) (by omega)
      rw [hr, ht] at h1
      exact (hz₃ 0 hn₃).2.2.2.2.1 h1

end Sound

end RdraIsh
