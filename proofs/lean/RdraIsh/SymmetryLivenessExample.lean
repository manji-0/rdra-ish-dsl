import RdraIsh.SymmetryLiveness

/-!
# Examples for Theorem A23

`ExampleSymLive` has two interchangeable instances, each with one bit that its own fair
action toggles. The group swaps the instances, their actions, their fair instances and the
binding. Every hypothesis of `a23` is discharged: the plain verdict of `eventually` on each
bit is proved directly (`plain_ok`), and `a23` gives the quotient's verdict (`quotient_ok`).
The canonical form sorts the two bits, so the quotient has three stored states. Its cycle
`(F,F) → (F,T) → (F,F)` has the swap as its composite permutation: the lift of one copy
does not close, and two copies do (`one_copy_open`, `two_copies_close`), as `lasso_closure`
predicts.

`reach_only_counterexample` shows why `Sym` asks for the commutation on every state and
`SymOn` on a set closed under `G_s`: with commutation on reachable states only, the
canonical quotient can store an unreachable image and reach a state the plain run does not.
-/

namespace RdraIsh

namespace ExampleSymLive

open LTS

/-- Toggle the bit of instance `i`. -/
def toggle : Bool → Bool × Bool → Bool × Bool
  | false, (a, b) => (!a, b)
  | true, (a, b) => (a, !b)

/-- Two bits; a step of label `i` toggles bit `i`. -/
def M : LTS (Bool × Bool) Bool where
  init s := s = (false, false)
  step s i t := t = toggle i s

/-- `true` swaps the two instances. -/
def swap : Bool → Bool × Bool → Bool × Bool
  | false, s => s
  | true, (a, b) => (b, a)

def h : Sym M Bool where
  mul g k := g ^^ k
  inv g := g
  act := swap
  actL g i := g ^^ i
  mem _ _ := True
  act_mul g k s := by cases g <;> cases k <;> rfl
  act_inv g s := by cases g <;> rfl
  act_inv' g s := by cases g <;> rfl
  mul_mem _ _ := trivial
  inv_mem _ := trivial
  mem_act _ := Iff.rfl
  step := by
    intro s a t g hst _
    show _ = _
    rw [hst]
    obtain ⟨x, y⟩ := s
    cases g <;> cases a <;> rfl
  mem_step _ _ := trivial
  init := by
    intro s g hs _
    show _ = _
    rw [hs]; cases g <;> rfl

/-- Fair instance `f` is the toggle of instance `f`; binding `b` names an instance too. -/
def L : SymLive h Bool Bool where
  actF g f := g ^^ f
  actB g b := g ^^ b
  actF_mul g k f := by cases g <;> cases k <;> cases f <;> rfl
  actF_inv g f := by cases g <;> cases f <;> rfl
  actF_inv' g f := by cases g <;> cases f <;> rfl
  actB_mul g k b := by cases g <;> cases k <;> cases b <;> rfl
  actB_inv' g b := by cases g <;> cases b <;> rfl
  inst f a := a = f
  inst_act g f a hi := by show _ = _; rw [hi]; rfl

/-- The bit of instance `b`. -/
def bit : Bool → Bool × Bool → Bool
  | false, s => s.1
  | true, s => s.2

def trig (_ : Bool) (_ : Bool × Bool) : Prop := True
def goal (b : Bool) (s : Bool × Bool) : Prop := bit b s = true

/-- `eventually` the bit of `b` is set, under `Fair_P ∧ WF_vars(Next)`. -/
theorem he : LiveEqui h L trig goal (fun s => ¬ M.Moves s) (fun _ _ => True) where
  trig_act _ _ := Iff.rfl
  trig_after _ _ _ := trivial
  after_next _ _ _ _ _ := trivial
  goal_act := by
    intro s g b _ _
    obtain ⟨x, y⟩ := s
    cases g <;> cases b <;> exact Iff.rfl
  stop_act hg := stop_moves_equi hg

/-- The canonical form sorts the bits. -/
def π : Bool × Bool → Bool
  | (true, false) => true
  | _ => false

def can (s : Bool × Bool) : Bool × Bool := swap (π s) s

theorem hcan : ∀ s, can s = h.act (π s) s := fun _ => rfl
theorem hπ : ∀ s, h.mem s (π s) := fun _ => trivial

def LM : List (Bool × Bool) := [(false, false), (false, true), (true, false), (true, true)]

theorem hLM : ∀ s, M.Reach s → s ∈ LM := by
  intro s _
  obtain ⟨x, y⟩ := s
  cases x <;> cases y <;> simp [LM]

theorem hG : FiniteGroups h := fun _ => ⟨[false, true], fun g _ => by cases g <;> simp⟩

theorem toggle_ne (i : Bool) (s : Bool × Bool) : toggle i s ≠ s := by
  obtain ⟨x, y⟩ := s
  cases i <;> cases x <;> cases y <;> simp [toggle]

theorem bit_toggle (b : Bool) (s : Bool × Bool) (hs : bit b s = false) :
    bit b (toggle b s) = true := by
  obtain ⟨x, y⟩ := s
  cases b <;> simp_all [bit, toggle]

/-- **The plain verdict.** Every fair behaviour sets each bit: the toggle of `b` is enabled
everywhere, so `Fair_P` takes it, and a toggle from an unset bit sets it. -/
theorem plain_ok : ∀ b, M.LiveHolds (fun ρ => M.FairP L.inst ρ ∧ StopsOnly (fun s => ¬ M.Moves s) ρ)
    (trig b) (goal b) := by
  intro b ρ _ ⟨hf, _⟩ ⟨i, _, hg⟩
  have hen : EvAlways fun n => M.Enabled L.inst b (ρ n) :=
    ⟨0, fun n _ => ⟨toggle b (ρ n), toggle_ne b _, b, rfl, rfl⟩⟩
  obtain ⟨j, hj, _, a, ha, hst⟩ := hf b hen i
  have ha' : a = b := ha
  have hst' : ρ (j + 1) = toggle a (ρ j) := hst
  subst ha'
  have hb : bit a (ρ j) = false := by
    have := hg j hj; simp only [goal] at this; cases e : bit a (ρ j) <;> simp_all
  exact hg (j + 1) (by omega) (by show bit a (ρ (j + 1)) = true; rw [hst']; exact bit_toggle a _ hb)

/-- **The quotient's verdict**, from `a23`: no annotated lasso of the canonical quotient
that is thread-fair and stops only where no move leaves violates the property. -/
theorem quotient_ok : ∀ r t p b', RunOf h can π r t p → ALasso r t p → AFair L r t p →
    AStops (fun s => ¬ M.Moves s) r t → ¬ AViolates L trig goal r p b' :=
  (a23 L he can π hcan hπ LM hLM hG).1 plain_ok

/-! The quotient cycle `(F,F) → (F,T) → (F,F)`: toggle instance `false` to `(T,F)`, stored as
`(F,T)` by the swap; toggle instance `true` back to `(F,F)`. -/

def rq (n : Nat) : Bool × Bool := if n % 2 = 0 then (false, false) else (false, true)
def tq (n : Nat) : Bool × Bool := if n % 2 = 0 then (true, false) else (false, false)
def pq (n : Nat) : Bool := π (tq n)

theorem run : RunOf h can π rq tq pq := by
  have hsucc : ∀ n, ((n + 1) % 2 = 0 ↔ ¬ n % 2 = 0) := fun n => by omega
  refine ⟨⟨⟨(false, false), false, rfl, trivial, rfl⟩, fun n => .inr ?_, fun _ => trivial,
    fun n => ?_⟩, ⟨(false, false), rfl, rfl⟩, fun n => ⟨?_, rfl⟩⟩
  · by_cases e : n % 2 = 0
    · exact ⟨false, by simp only [rq, tq, e]; rfl⟩
    · exact ⟨true, by simp only [rq, tq, e]; rfl⟩
  · by_cases e : n % 2 = 0
    · simp [rq, tq, pq, e, hsucc, π, h, swap]
    · simp [rq, tq, pq, e, hsucc, π, h, swap]
  · by_cases e : n % 2 = 0
    · simp [rq, tq, e, hsucc, can, π, swap]
    · simp [rq, tq, e, hsucc, can, π, swap]

theorem lasso : ALasso rq tq pq := by
  refine ⟨0, 2, by omega, fun n _ => ?_⟩
  have e : (n + 2) % 2 = n % 2 := by omega
  simp [rq, tq, pq, e]

/-- The composite permutation of the cycle is the swap. -/
theorem composite_swaps : cmp h pq 0 1 = true := rfl

/-- The concrete lift from `(F,F)`. -/
def cl (n : Nat) : Bool × Bool := (liftSeq h tq pq (false, false) false n).1

theorem lift_behaviour : M.Behaviour cl :=
  ((run).1.lift (c₀ := (false, false)) (Θ₀ := false) rfl trivial rfl).2

/-- One copy of the cycle does not close the concrete lasso: the concrete states two edges
apart differ. -/
theorem one_copy_open : cl 3 ≠ cl 1 := by decide

/-- Two copies, the order of the swap, close it. -/
theorem two_copies_close : cl 4 = cl 0 ∧ cl 5 = cl 1 := by decide

end ExampleSymLive

/-! ## Commutation on reachable states only is not enough -/

namespace ExampleReachOnly

open LTS

inductive St | s0 | u | v | bad
  deriving DecidableEq

/-- `s0 → u` and `v → bad`; `u` and `v` are images of each other, `v` is not reachable. -/
def M : LTS St Unit where
  init s := s = .s0
  step s _ t := (s = .s0 ∧ t = .u) ∨ (s = .v ∧ t = .bad)

/-- `true` swaps `u` and `v`. -/
def act : Bool → St → St
  | true, .u => .v
  | true, .v => .u
  | _, s => s

/-- `G_{s0}` is trivial; the other states allow the swap. -/
def mem (s : St) (g : Bool) : Prop := s = .s0 → g = false

def π : St → Bool
  | .u => true
  | _ => false

def can (s : St) : St := act (π s) s

theorem reach : ∀ s, M.Reach s → s = .s0 ∨ s = .u := by
  intro s hs
  induction hs with
  | init h => exact .inl h
  | step _ hst ih =>
    rcases hst with ⟨_, rfl⟩ | ⟨rfl, _⟩
    · exact .inr rfl
    · rcases ih with h | h <;> cases h

/-- **Reachable-only commutation does not suffice.** With `mul := xor` and `inv := id`, every
field of `Sym` holds, with the commutation (`step`, `mem_step`) restricted to reachable
states; the canonical form is an image under `G_s`; and the invariant `s ≠ bad` is preserved
by the group. Yet the invariant holds on `M` and fails on the canonical quotient, which
stores the unreachable image `v` of `u` and steps from it. -/
theorem reach_only_counterexample :
    (∀ g k s, act (g ^^ k) s = act g (act k s)) ∧ (∀ g s, act g (act g s) = s) ∧
    (∀ s g k, mem s g → mem s k → mem s (g ^^ k)) ∧
    (∀ s g k, mem s g → (mem (act g s) k ↔ mem s k)) ∧
    (∀ s a t g, M.Reach s → mem s g → M.step s a t → M.step (act g s) a (act g t) ∧ mem t g) ∧
    (∀ s g, M.init s → mem s g → M.init (act g s)) ∧
    (∀ s, can s = act (π s) s ∧ mem s (π s)) ∧
    (∀ s g, mem s g → (act g s ≠ .bad ↔ s ≠ .bad)) ∧
    M.Holds (· ≠ .bad) ∧ ¬ (quotient M can).Holds (· ≠ .bad) := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · intro g k s; cases g <;> cases k <;> cases s <;> rfl
  · intro g s; cases g <;> cases s <;> rfl
  · intro s g k hg hk hs; rw [hg hs, hk hs]; rfl
  · intro s g k hg
    have e : act g s = .s0 ↔ s = .s0 := by cases g <;> cases s <;> decide
    exact ⟨fun hm hs => hm (e.2 hs), fun hm hs => hm (e.1 hs)⟩
  · intro s a t g hs hg hst
    rcases reach s hs with rfl | rfl
    · have hg' : g = false := hg rfl
      subst hg'
      exact ⟨hst, fun _ => rfl⟩
    · rcases hst with ⟨h, _⟩ | ⟨h, _⟩ <;> cases h
  · intro s g hs hg
    have hs' : s = .s0 := hs
    subst hs'
    have hg' : g = false := hg rfl
    subst hg'; rfl
  · intro s
    refine ⟨rfl, fun hs => ?_⟩
    subst hs; rfl
  · intro s g _
    cases s <;> cases g <;> decide
  · intro s hs
    rcases reach s hs with rfl | rfl <;> decide
  · intro hQ
    have h0 : (quotient M can).Reach .s0 := Reach.init ⟨.s0, rfl, rfl⟩
    have h1 : (quotient M can).Reach .v := Reach.step (a := ()) h0 ⟨.u, .inl ⟨rfl, rfl⟩, rfl⟩
    have h2 : (quotient M can).Reach .bad := Reach.step (a := ()) h1 ⟨.bad, .inr ⟨rfl, rfl⟩, rfl⟩
    exact hQ _ h2 rfl

end ExampleReachOnly

end RdraIsh
