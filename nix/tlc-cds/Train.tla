---- MODULE Train ----
\* The cone `verify --with-tlc` writes for the `ShippedReopens` property of
\* crates/rdra-ish-cli/tests/engine_corpus/fair_stop_vs_wf_next.rdra. flake.nix runs TLC on
\* it once to record the classes TLC loads in its class-data-sharing archive. The property
\* fails, so the run parses, explores, checks liveness, and prints a lasso.

EXTENDS Integers

InstanceCount == 2
IntRange == 0..5
Order_Ids == 1..InstanceCount

\* The lowest absent slot: a create takes it, and a deleted slot is never reused.
FirstAbsent(ex, i) == ex[i] = "absent" /\ \A j \in DOMAIN ex : j < i => ex[j] # "absent"

VARIABLES Order_exists, Order_status

Order_vars == <<Order_exists, Order_status>>

vars == <<Order_vars>>

TypeOK ==
  /\ Order_exists \in [Order_Ids -> {"absent", "present", "deleted"}]
  /\ Order_status \in [Order_Ids -> {"pending", "paid", "shipped"}]

Init ==
  /\ Order_exists = [i \in Order_Ids |-> "absent"]
  /\ Order_status = [i \in Order_Ids |-> "pending"]

\* p0: Order
Pay(p0) ==
  /\ Order_exists[p0] = "present"
  /\ Order_status[p0] = "pending"
  /\ Order_status' = [Order_status EXCEPT ![p0] = "paid"]
  /\ UNCHANGED Order_exists

\* p0: Order
Ship(p0) ==
  /\ Order_exists[p0] = "present"
  /\ Order_status[p0] = "paid"
  /\ Order_status' = [Order_status EXCEPT ![p0] = "shipped"]
  /\ UNCHANGED Order_exists

\* p0: Order (new)
Place(p0) ==
  /\ FirstAbsent(Order_exists, p0)
  /\ Order_exists' = [Order_exists EXCEPT ![p0] = "present"]
  /\ UNCHANGED Order_status

Next ==
  \/ \E p0 \in Order_Ids : Pay(p0)
  \/ \E p0 \in Order_Ids : Ship(p0)
  \/ \E p0 \in Order_Ids : Place(p0)

Spec == Init /\ [][Next]_vars

PaidShips == \A b0 \in Order_Ids : ((Order_exists[b0] = "present") /\ (Order_status[b0] = "paid")) ~> (Order_status[b0] = "shipped")
ShippedReopens == \A b0 \in Order_Ids : ((Order_exists[b0] = "present") /\ (Order_status[b0] = "shipped")) ~> (Order_status[b0] = "pending")

====
