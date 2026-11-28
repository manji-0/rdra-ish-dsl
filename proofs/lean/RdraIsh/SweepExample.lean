import RdraIsh.Sweep

/-!
# The certificate checker on the cases of `the_stability_certificate_rejects_a_partition_that_is_not_stable`

Three states with one label, a silent edge `0 → 1` and no rises. The identity partition
passes `check`. The partition `{0, 2}, {1}` does not: state `0` leaves its block into `{1}`
and state `2` does not, so the sweep splits `{0, 2}`, and the certificate itself fails
(`not_cert`), not only the checker.
-/

namespace RdraIsh.ExampleSweep

open Sweep

def R : ExplRun Unit Unit Nat := ⟨3, fun _ => ((), ()), fun _ => 0, [(0, 1)], [], fun _ => 0⟩

def ident : Nat → Nat := fun i => i
def merged : Nat → Nat := fun i => if i = 1 then 1 else 0

theorem accepts : check R ident = true := by decide

theorem accepted_cert : Cert R ident := check_sound accepts

theorem rejects : check R merged = false := by decide

theorem not_cert : ¬ Cert R merged := by
  intro h
  have hm : Marked R.silent merged 1 0 :=
    .front (List.mem_singleton_self _) (by decide) (by decide) (by decide)
  have h2 := h.b2 (sp := 1) (i := 0) (j := 2) (by decide) (by decide) (by decide) hm
  -- State `2` has no silent edge, so it cannot be marked.
  have : ∀ {p}, Marked R.silent merged 1 p → p = 0 := by
    intro p hp
    induction hp with
    | front he _ _ _ =>
      simp only [R, List.mem_singleton, Prod.mk.injEq] at he
      exact he.1
    | back he _ _ _ _ =>
      simp only [R, List.mem_singleton, Prod.mk.injEq] at he
      exact he.1
  exact absurd (this h2) (by decide)

end RdraIsh.ExampleSweep
