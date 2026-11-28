import RdraIsh

/-! The headline theorems. `scripts/check-lean-proofs.sh` fails when one of them depends on
an axiom outside Lean's three standard ones (`propext`, `Classical.choice`, `Quot.sound`),
`sorryAx` among them. proofs/lean/README.md maps each to its claim in docs/theory/claims.md. -/

#print axioms RdraIsh.holds_of_stutterSim
#print axioms RdraIsh.holds_iff_of_dbsb
#print axioms RdraIsh.reachable_obs_eq_of_dbsb
#print axioms RdraIsh.a8_dbsb
#print axioms RdraIsh.a8_holds_iff
#print axioms RdraIsh.holds_of_reduced
#print axioms RdraIsh.a18_dbsb
#print axioms RdraIsh.Sym.sub
#print axioms RdraIsh.quotient_covers
#print axioms RdraIsh.quotient_sound
#print axioms RdraIsh.quotient_holds_iff
#print axioms RdraIsh.Example.flag_after_s
#print axioms RdraIsh.ExampleSym.sorted_quotient
#print axioms RdraIsh.LTS.wfNext_iff
#print axioms RdraIsh.recurrent_segment
#print axioms RdraIsh.LTS.lasso_of_behaviour
#print axioms RdraIsh.image_step
#print axioms RdraIsh.takes_fwd
#print axioms RdraIsh.enabled_back
#print axioms RdraIsh.moves_back
#print axioms RdraIsh.silentCycle_of_divergence
#print axioms RdraIsh.a15_image
#print axioms RdraIsh.a15_behaviour
#print axioms RdraIsh.a15
#print axioms RdraIsh.ExampleFair.silent_cycle
#print axioms RdraIsh.ExampleFair.eventually_flag
#print axioms RdraIsh.takes_act
#print axioms RdraIsh.enabled_act
#print axioms RdraIsh.Framed.fair_iff
#print axioms RdraIsh.Framed.stops_iff
#print axioms RdraIsh.Framed.violates_iff
#print axioms RdraIsh.ARun.lift
#print axioms RdraIsh.project
#print axioms RdraIsh.a23_lift
#print axioms RdraIsh.a23_project
#print axioms RdraIsh.a23_iff
#print axioms RdraIsh.project_lasso
#print axioms RdraIsh.a23
#print axioms RdraIsh.order_exists
#print axioms RdraIsh.ARun.mem_const
#print axioms RdraIsh.lasso_closure
#print axioms RdraIsh.threadFair_iff_pieces
#print axioms RdraIsh.restrict_liveHolds_iff
#print axioms RdraIsh.a23_on
#print axioms RdraIsh.ExampleSymLive.plain_ok
#print axioms RdraIsh.ExampleSymLive.quotient_ok
#print axioms RdraIsh.ExampleSymLive.one_copy_open
#print axioms RdraIsh.ExampleSymLive.two_copies_close
#print axioms RdraIsh.ExampleReachOnly.reach_only_counterexample
