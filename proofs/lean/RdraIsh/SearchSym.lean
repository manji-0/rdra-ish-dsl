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

end RdraIsh
