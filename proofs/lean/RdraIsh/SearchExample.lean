import RdraIsh.SearchSym
import RdraIsh.FairnessExample
import RdraIsh.SymmetryLivenessExample

/-!
# The searches on the A15 and A23 examples

`ExampleFair` (FairnessExample.lean) and `ExampleSymLive` (SymmetryLivenessExample.lean)
discharge the hypotheses of `a15` and `a23`. Here the searches of N9 run on them:

- `ExampleFair.search_ok`: N9's search on the reduced cone `R` under `LoopingStop` does not
  accept, and `a15_search` gives the verdict on `C` from that alone.
- `ExampleSymLive.orbit_ok`, `ExampleSymLive.plain_search_ok`: the orbit search on the
  swap's quotient does not accept, and neither does the plain search for any binding. The
  list of fair instances enabled at each state, the one hypothesis `oAccepts_iff` adds to
  `a23`'s, is the two instances.
-/

namespace RdraIsh

open LTS

namespace ExampleFair

/-- N9's search on `R` under `StopRule::Looping` does not accept. -/
theorem search_ok : ¬ R.Accepts (onT inst) (LoopingStop R canLoop)
    (fun q => trig (obsR obs rep q)) (fun q => goal (obsR obs rep q)) := fun hacc => by
  obtain ⟨ρ, hl, hb, hf, hs, hv⟩ := lasso_of_accepts LX (fun q _ => mem_LX q) hacc
  exact run_ok ρ hl hb hf hs hv

/-- Theorem A15 with the search: the verdict on `C` from the search on `R`. -/
theorem eventually_flag_search :
    C.LiveHolds (fun σ => C.FairP (onT inst) σ ∧ C.WFNext σ)
      (fun p => trig (obsC obs p)) (fun p => goal (obsC obs p)) :=
  a15_search split lemma6 extended stable LX (fun x _ => mem_LX x) LX (fun q _ => mem_LX q)
    canLoop (fun _ h => h) inst trig goal search_ok

end ExampleFair

namespace ExampleSymLive

/-- The canonical form of the example. -/
def K : Canon h := ⟨can, π, hcan, hπ⟩

/-- The fair instances enabled at a state lie in a list: there are two. -/
theorem hΦ : ∀ r, ∃ LΦ : List Bool, ∀ x, M.Enabled L.inst x r → x ∈ LΦ :=
  fun _ => ⟨[false, true], fun x _ => by cases x <;> simp⟩

/-- The orbit search on the quotient does not accept. -/
theorem orbit_ok : ¬ OAccepts L K goal (fun s => ¬ M.Moves s) trig :=
  (liveHolds_iff_not_oAccepts he hG hΦ LM hLM).1 plain_ok

/-- N9's search on `M` accepts for no binding. -/
theorem plain_search_ok : ∀ b, ¬ M.Accepts L.inst (fun s => ¬ M.Moves s) (trig b) (goal b) :=
  fun b hacc => orbit_ok ((oAccepts_iff_accepts he hG hΦ LM hLM).2 ⟨b, hacc⟩)

end ExampleSymLive

end RdraIsh
