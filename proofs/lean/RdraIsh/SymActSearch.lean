import RdraIsh.SymActions
import RdraIsh.SearchSym

/-!
# The orbit search on the slot model (cones.md, A18 and A23; native.md, N9)

`SymAct.Sys.symOn` derives Theorem A18's commutation from (G1)–(G6) on the base-absent
states `W`, and `SearchSym.lean` proves that the orbit search, stated over a `SymOn`, decides
the property. Together: under `WF`, `Built` and `Hyp`, the orbit search's verdict on the cone
system `S.lts` is the plain verdict, and it is the verdict of N9's search on `S.lts`, with no
commutation assumed outside `W`.

What stays a hypothesis is what the slot model does not hold: properties (the trigger and the
goal, and so `LiveEqui`'s `trig_act` and `goal_act`, which (G6) gives the code's reads), the
fair instances and the bindings with their actions (`SymLive`), the canonical form (`Canon`),
and the finiteness of the reachable states, the groups and the enabled instances.
-/

namespace RdraIsh.SymAct.Sys

open LTS

variable {K E C O A : Type} [DecidableEq E] [DecidableEq C] [DecidableEq O]
  {S : Sys K E C O A} {Φ β : Type}

section Orbit

variable (hwf : S.WF) (hb : S.Built) (hH : S.Hyp)
  {L : SymLive (S.symOn hwf hb hH).toSym Φ β} {Kc : Canon (S.symOn hwf hb hH).toSym}
  {trig goal : β → St E C O → Prop} {stop stop' : St E C O → Prop}
  {after : β → St E C O → Prop}

/-- **The orbit verdict from (G1)–(G6).** The property holds on the cone system for every
binding under `Fair_P` and `stop` iff the orbit search, with a stop rule that agrees with
`stop` on the base-absent states, does not accept. -/
theorem orbit_verdict (he : LiveEqui (S.symOn hwf hb hH).toSym L trig goal stop' after)
    (hstop : ∀ s, S.BaseAbsent s → (stop' s ↔ stop s))
    (hG : FiniteGroups (S.symOn hwf hb hH).toSym)
    (hΦ : ∀ r, ∃ LΦ : List Φ, ∀ x, S.lts.Enabled L.inst x r → x ∈ LΦ)
    (LM : List (St E C O)) (hLM : ∀ s, S.lts.Reach s → s ∈ LM) :
    (∀ b, S.lts.LiveHolds (fun ρ => S.lts.FairP L.inst ρ ∧ StopsOnly stop ρ) (trig b) (goal b)) ↔
      ¬ OAccepts L Kc goal stop' trig :=
  liveHolds_iff_not_oAccepts_on (S.symOn hwf hb hH) he hstop hG hΦ LM hLM

/-- **The orbit search and N9's search agree, from (G1)–(G6).** The orbit search accepts iff
N9's search on the cone system accepts for some binding. -/
theorem orbit_accepts_iff (he : LiveEqui (S.symOn hwf hb hH).toSym L trig goal stop' after)
    (hstop : ∀ s, S.BaseAbsent s → (stop' s ↔ stop s))
    (hG : FiniteGroups (S.symOn hwf hb hH).toSym)
    (hΦ : ∀ r, ∃ LΦ : List Φ, ∀ x, S.lts.Enabled L.inst x r → x ∈ LΦ)
    (LM : List (St E C O)) (hLM : ∀ s, S.lts.Reach s → s ∈ LM) :
    OAccepts L Kc goal stop' trig ↔ ∃ b, S.lts.Accepts L.inst stop (trig b) (goal b) :=
  oAccepts_on_iff_accepts (S.symOn hwf hb hH) he hstop hG hΦ LM hLM

/-- **The orbit verdict under `WF_vars(Next)`, from (G1)–(G6).** The search's stop rule is
`¬ Moves` of the cone system restricted to the base-absent states. -/
theorem orbit_verdict_wfNext (he : LiveEqui (S.symOn hwf hb hH).toSym L trig goal stop' after)
    (hG : FiniteGroups (S.symOn hwf hb hH).toSym)
    (hΦ : ∀ r, ∃ LΦ : List Φ, ∀ x, S.lts.Enabled L.inst x r → x ∈ LΦ)
    (LM : List (St E C O)) (hLM : ∀ s, S.lts.Reach s → s ∈ LM) :
    (∀ b, S.lts.LiveHolds (fun ρ => S.lts.FairP L.inst ρ ∧ S.lts.WFNext ρ) (trig b) (goal b)) ↔
      ¬ OAccepts L Kc goal (fun q => ¬ (S.lts.restrict S.BaseAbsent).Moves q) trig :=
  liveHolds_wfNext_iff_not_oAccepts_on (S.symOn hwf hb hH) he hG hΦ LM hLM

end Orbit

end RdraIsh.SymAct.Sys
