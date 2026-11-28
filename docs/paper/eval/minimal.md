# Least instance count of each known counterexample (roadmap T4)

**Question.** At how many instances per entity does each known counterexample first appear?
This supports the bounded-verification argument (`verify` defaults to 2 instances) and the split
between BFS and `--prove`.

**Method.** `scripts/paper/eval.py minimal` takes every case of the engine corpus
(`crates/rdra-ish-cli/tests/engine_corpus/`, 91 cases) whose header expects a native
counterexample, and every `// expect verify ...: fail` header of `samples/formal-verification`
and `samples/formal-verification-fail`. It runs `verify` at the header's flags with every
`--instance-count` and `--instances` removed, at `--instance-count` 1, 2 and 3. For each
expected failing check it records the least count whose run reports `error[<check>]`, and the
length of the printed counterexample at that count. Data: [data/minimal.tsv](./data/minimal.tsv).
Main at `9e4a123`.

**Result.** 132 expected counterexamples:

| Least uniform instance count | Counterexamples |
|---|--:|
| 1 | 106 |
| 2 | 24 |
| 3 | 0 |
| not found at 1 to 3 | 2 |

- No counterexample needs three uniform instances where two do not suffice.
- The two not found are expected. `safety_count_per_entity` / `Quantifier_Member_0` needs the
  header's per-entity count `--instances Loan=4`, which a uniform count of 3 does not reach.
  `cap_memory_limit` / `NoShip` is a case about the memory cap: its header sets
  `--memory 2KiB`, and the script passes `--memory 12GiB` after the header's flags, so the run
  does not stop where the header expects it to.
- The 24 cases that need two instances are those the corpus wrote to need two rows: uniqueness
  (`Unique_*`), counting quantifiers, foreign keys that move (`presence_along_key_moves`), the
  asymmetry of lowest-slot creation (`presence_first_absent_asymmetry`), the symmetry cases
  (`sym_*`), and seven deadlock findings.
- Of the 84 counterexamples with a printed trace length, the mean length is 3.3 steps and the
  longest is 9. At their least count, those at one instance average 3.1 steps (67 traces), and
  those at two instances 4.1 steps (17 traces). Deadlock and witness findings print no step count.

**Reading.** On these models, two instances find every known counterexample that a uniform bound
finds. This is evidence about a corpus written to exercise the checker, not a small-scope theorem.
The corpus cases are adversarial by design, so they over-represent the two-row cases. The domain
samples have one expected counterexample (`insurance-claim-ops`,
`Deadlock_ClaimCase_status_assessed` at 2 instances), outside this table.

## Table

| Case | Check | Least n | Steps at least n |
|---|---|---:|---:|
| cap_memory_limit.rdra | NoShip | >3 | - |
| cone_entity_mark_left_out.rdra | Deadlock_Order_status_placed | 1 | - |
| cone_entity_mark_left_out.rdra | Witness_Order_status_shipped | 1 | - |
| cone_entity_mark_left_out.rdra | Witness_Notice_status_received | 1 | - |
| cone_entity_mark_left_out.rdra | Witness_Notice_status_read | 1 | - |
| cone_entity_mark_left_out.rdra | Witness_Payment_status_paid | 1 | - |
| cone_entity_mark_left_out.rdra | Deadlock_Payment_status_unpaid | 1 | - |
| cone_fold_key_dropped.rdra | Forbidden_Order_0 | 1 | 9 |
| cone_handler_mark_left_out.rdra | ShipsUnfair | 1 | 3 |
| cone_handler_mark_left_out.rdra | ShipsFair | 1 | 3 |
| cone_handler_mark_left_out.rdra | Deadlock_Order_status_paid | 1 | - |
| cone_independent_loop.rdra | PaidShipsUnfair | 1 | 5 |
| cone_receipt_frees_slots.rdra | PendingPays | 1 | 2 |
| cone_reduced_class_loops.rdra | JobStarts | 1 | 3 |
| cone_reduced_class_loops.rdra | JobStartsUnfair | 1 | 3 |
| cone_reduced_class_loops.rdra | JobWaits | 1 | 5 |
| cone_reduced_inner_class_loops.rdra | JobStarts | 1 | 3 |
| cone_reduced_loop_unreached.rdra | Deadlock_Gate_phase_pa | 1 | - |
| cone_reduced_loop_unreached.rdra | Deadlock_Job_status_waiting | 1 | - |
| cone_rise_names_a_dropped_new_row.rdra | Forbidden_Order_0 | 1 | 8 |
| cone_rise_names_a_dropped_new_row.rdra | Deadlock_Order_status_paid | 2 | - |
| cone_rise_param_on_a_stub.rdra | Forbidden_Order_0 | 1 | 9 |
| cone_rise_param_on_a_stub.rdra | Deadlock_Order_status_paid | 2 | - |
| cone_triggered_param_on_a_stub.rdra | Forbidden_Order_0 | 1 | 7 |
| cone_triggered_param_on_a_stub.rdra | Deadlock_Order_status_paid | 2 | - |
| fair_counted_exposes_lost_run.rdra | OrderComplete | 1 | 3 |
| fair_counted_exposes_lost_run.rdra | Deadlock_Order_delivered_partial | 1 | - |
| fair_counted_exposes_lost_run.rdra | Witness_Order_delivered_complete | 1 | - |
| fair_creating_use_case.rdra | PendingPays | 1 | 2 |
| fair_enabled_disabled_forever.rdra | PaidShips | 1 | 4 |
| fair_same_edge_two_bindings.rdra | LoopsForever | 1 | 3 |
| fair_selfloop_not_enabling.rdra | PaidShips | 1 | 2 |
| fair_split_use_case.rdra | EndsUnfairSplit | 1 | 5 |
| fair_stop_vs_wf_next.rdra | ShippedReopens | 1 | 3 |
| fair_triggered_handler.rdra | PostedUnfair | 1 | 6 |
| presence_along_key_moves.rdra | CapturedPaid | 2 | 9 |
| presence_along_key_moves.rdra | Deadlock_Order_status_open | 2 | - |
| presence_deleted_row_owes_goal.rdra | PendingPays | 1 | 2 |
| presence_first_absent_asymmetry.rdra | RushFinishes | 2 | 7 |
| presence_first_absent_asymmetry.rdra | Deadlock_Order_status_open | 2 | - |
| presence_goal_present_modifier.rdra | HeldReleasedPresent | 1 | 3 |
| presence_per_entity_slots.rdra | Unique_Loan_tag | 2 | 3 |
| presence_per_entity_slots.rdra | Unique_Room_tag | 2 | 0 |
| safety_at_least_guard.rdra | Quantifier_Claim_0 | 2 | 5 |
| safety_at_least_guard.rdra | Deadlock_Claim_status_open | 1 | - |
| safety_at_least_guard.rdra | Deadlock_Order_status_open | 1 | - |
| safety_can_reach_frees_slots.rdra | OrderCanPay | 1 | 2 |
| safety_count_guard_order.rdra | Deadlock_Shelf_status_open | 2 | - |
| safety_count_per_entity.rdra | Quantifier_Member_0 | >3 | - |
| safety_count_per_entity.rdra | Quantifier_Shelf_1 | 2 | 3 |
| safety_counted_once_per_raise.rdra | Forbidden_Crate_1 | 1 | 4 |
| safety_created_set_beside_transitions.rdra | ConflictingEffects_Order_Place_status | 1 | - |
| safety_delete_cascade.rdra | Forbidden_OrderLine_0 | 1 | 3 |
| safety_delete_set_null.rdra | Forbidden_OrderLine_0 | 1 | 4 |
| safety_edge_free_cone.rdra | Forbidden_Ticket_0 | 1 | 2 |
| safety_edge_free_cone.rdra | Witness_Order_status_refunded | 1 | - |
| safety_event_overflow.rdra | EventOverflow_Inbox_EvNoted_Read | 1 | 4 |
| safety_exactly_along.rdra | Deadlock_Member_status_pending | 1 | - |
| safety_null_columns_cross.rdra | MultiForbidden_Line_1 | 1 | 2 |
| safety_null_columns_cross.rdra | MultiForbidden_Line_2 | 1 | 4 |
| safety_null_columns_eq.rdra | Forbidden_Item_1 | 1 | 3 |
| safety_null_columns_eq.rdra | ABNeverMet | 1 | 3 |
| safety_null_columns_ne.rdra | Forbidden_Item_0 | 1 | 1 |
| safety_null_columns_ne.rdra | Required_Item_0 | 1 | 1 |
| safety_null_derived_in.rdra | Forbidden_Item_2 | 1 | 1 |
| safety_null_derived_in.rdra | Forbidden_Item_5 | 1 | 1 |
| safety_null_derived_in.rdra | Forbidden_Box_7 | 1 | 2 |
| safety_null_enum_bool_columns.rdra | Forbidden_Item_0 | 1 | 1 |
| safety_null_enum_bool_columns.rdra | Forbidden_Item_1 | 1 | 1 |
| safety_null_enum_bool_columns.rdra | Forbidden_Item_3 | 1 | 3 |
| safety_null_enum_bool_columns.rdra | Forbidden_Item_4 | 1 | 2 |
| safety_null_enum_bool_literals.rdra | Forbidden_Item_0 | 1 | 1 |
| safety_null_enum_bool_literals.rdra | Forbidden_Item_3 | 1 | 1 |
| safety_null_enum_bool_literals.rdra | Required_Item_3 | 1 | 1 |
| safety_precedes_latch.rdra | ShippedAfterAudit | 1 | 4 |
| safety_stable_across_delete.rdra | PendingStays | 1 | 2 |
| safety_unique_sets.rdra | Unique_Badge_color | 2 | 2 |
| safety_unique_sets.rdra | Unique_Enrollment_course_id_student_id | 2 | 2 |
| safety_vacuity.rdra | AlwaysDone | 1 | 1 |
| sym_deleted_row_owner.rdra | Quantifier_Mall_0 | 2 | 8 |
| sym_identity_pair.rdra | MultiForbidden_Account_0 | 1 | 1 |
| sym_lift_compose.rdra | Quantifier_Queue_0 | 2 | 6 |
| sym_master_ties.rdra | MultiForbidden_Visit_0 | 1 | 3 |
| sym_master_ties.rdra | Deadlock_Visit_status_booked | 1 | - |
| sym_relink_loop.rdra | TicketCloses | 2 | 5 |
| sym_relink_loop.rdra | TicketClosesFair | 2 | 5 |
| sym_self_owned.rdra | Deadlock_Task_status_open | 1 | - |
| width_nullable_enum_256.rdra | DomainTooLarge_Row_s | 1 | - |
| width_nullable_int_top.rdra | DomainTooLarge_Item_n | 1 | - |
| z3_cti_true_rule.rdra | Deadlock_Door_state_closed | 1 | - |
| z3_cti_true_rule.rdra | Deadlock_Light_state_on | 1 | - |
| z3_inductive.rdra | Forbidden_Order_1 | 1 | 4 |
| z3_int_and_now_boundary.rdra | Forbidden_Item_1 | 1 | 2 |
| z3_int_default_out_of_range.rdra | IntLiteralOutOfBounds_Item_7 | 1 | - |
| z3_int_listing_guard.rdra | Deadlock_Item_status_open | 1 | - |
| z3_int_listing_guard.rdra | Witness_Item_status_flagged | 1 | - |
| z3_int_listing_past_the_top.rdra | Forbidden_Item_0 | 1 | 2 |
| z3_lemma.rdra | Deadlock_Door_state_closed | 1 | - |
| z3_lemma.rdra | Deadlock_Light_state_on | 1 | - |
| z3_lemma_run_mark.rdra | Deadlock_Light_state_on | 1 | - |
| z3_lemma_run_mark.rdra | Deadlock_Sign_state_dark | 1 | - |
| z3_needs_exists.rdra | Deadlock_Order_status_open | 2 | - |
| z3_needs_exists_global.rdra | Deadlock_Gate_state_shut | 1 | - |
| z3_needs_exists_global.rdra | Deadlock_Guard_state_on | 1 | - |
| z3_needs_run_mark.rdra | Deadlock_Light_state_on | 1 | - |
| z3_needs_run_mark.rdra | Deadlock_Sign_state_dark | 1 | - |
| z3_now_past_the_top.rdra | Forbidden_Coupon_0 | 1 | 7 |
| z3_two_ok_three_fails.rdra | Deadlock_Order_status_c | 1 | - |
| z3_two_ok_three_fails.rdra | Witness_Order_status_d | 1 | - |
| after_assert_unmet.rdra | AfterAssert_DeliverOrder_0 | 1 | 4 |
| can_reach_unmet.rdra | OrderCanFinish | 1 | 3 |
| check_unmet.rdra | Check_CartLine_qty_0 | 1 | 2 |
| cross_order_payment_split.rdra | MultiForbidden_Order_0 | 1 | 3 |
| deadlock.rdra | Deadlock_Order_status_paid | 1 | - |
| min_card_unmet.rdra | MinCard_OrderLine_Order | 1 | 1 |
| none_any_child_bypassed.rdra | Unique_Invoice_Order | 2 | 4 |
| now_coupon.rdra | Forbidden_Coupon_0 | 1 | 2 |
| one_to_one_shared.rdra | Unique_Shipment_Order | 2 | 3 |
| order_stability.rdra | CancelledStaysCancelled | 1 | 3 |
| order_stability.rdra | AmountKeptOnceDecided | 1 | 4 |
| order_stability.rdra | ShippedAfterInvoiced | 1 | 4 |
| partial_scope.rdra | DerivedColumnRead_Audit_score | 1 | - |
| quantifier_none.rdra | Quantifier_Cert_0 | 1 | 2 |
| temporal_along.rdra | CapturedPaidAnyOrder | 2 | 1 |
| temporal_fair.rdra | GrantEndsUnfair | 2 | 5 |
| temporal_unmet.rdra | NeverShipped | 1 | 3 |
| temporal_unmet.rdra | PendingLeadsToDelivered | 1 | 3 |
| unique_double_booking.rdra | Unique_Booking_room_id_slot | 2 | 2 |
| untriggered_sets.rdra | Forbidden_Order_0 | 1 | 3 |
| unwitnessed_state.rdra | Witness_Order_status_refunded | 1 | - |
| unwitnessed_state.rdra | Deadlock_Order_status_shipped | 1 | - |
| order.rdra | Inv_Order_0 | 1 | 2 |
