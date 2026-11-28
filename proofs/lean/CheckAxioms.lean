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
#print axioms RdraIsh.Act.mem_closed
#print axioms RdraIsh.Act.mem_least
#print axioms RdraIsh.Act.mem_eq_of_sound
#print axioms RdraIsh.Act.closed_of_fixed
#print axioms RdraIsh.Act.computed_fixed
#print axioms RdraIsh.Act.computed_iff
#print axioms RdraIsh.Act.lemma5_of_closed
#print axioms RdraIsh.Act.lemma5_computed
#print axioms RdraIsh.Act.no_counter_input
#print axioms RdraIsh.Act.split_of_lemma5
#print axioms RdraIsh.Act.split_computed
#print axioms RdraIsh.ExampleSplit.sets
#print axioms RdraIsh.ExampleSplit.split
#print axioms RdraIsh.ExampleSplit.input_and_obs
#print axioms RdraIsh.Act.Local.idle
#print axioms RdraIsh.Act.lemma5ab_of_closed
#print axioms RdraIsh.Act.cone_reach_iff
#print axioms RdraIsh.ExampleSplit.lemma5
