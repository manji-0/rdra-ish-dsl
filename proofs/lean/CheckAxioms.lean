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
