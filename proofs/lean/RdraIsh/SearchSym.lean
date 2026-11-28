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

end RdraIsh
