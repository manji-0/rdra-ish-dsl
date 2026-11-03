//! Relate bindings: mandatory owners need a present parent, optional owners may stay unlinked.

use super::*;
use crate::lifecycle::{Effect, GuardWhy, InvKind, ParamRole, Rhs, Value};

/// clinic-ops' Charge/Claim shape: the claim's creator updates the charge it bills.
fn billing(relate: &str) -> String {
    format!(
        r#"
entity Claim "請求" {{ id: Int @pk  status: Enum(draft, sent) }}
entity Charge "料金" {{ id: Int @pk  status: Enum(posted, billed) }}
usecase CreateCharge "計上"
usecase GenerateClaim "請求作成"
usecase SendClaim "送付"
{relate}
creates(CreateCharge, Charge)
creates(GenerateClaim, Claim)
updates(GenerateClaim, Charge)
transitions(GenerateClaim, Charge.status, posted -> billed)
transitions(SendClaim, Claim.status, draft -> sent)
"#
    )
}

#[test]
fn mandatory_owner_needs_a_present_parent_so_the_cycle_creates_nothing() {
    let model = model_from(&billing("relate(Charge, Claim, N:1)"));
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    let create = built
        .ts
        .actions()
        .iter()
        .find(|a| a.name == "CreateCharge")
        .unwrap();
    assert!(create.params.iter().any(|p| !p.role.is_written()));
    let d = derive(&model);
    assert!(d.entity("Charge").patterns.is_empty());
    assert!(d.entity("Claim").patterns.is_empty());
}

#[test]
fn optional_owner_starts_unlinked_and_the_claim_links_it() {
    let model = model_from(&billing("relate(Charge, Claim, N:1).optional()"));
    let built = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL);
    let create = built
        .ts
        .actions()
        .iter()
        .find(|a| a.name == "CreateCharge")
        .unwrap();
    assert_eq!(
        create.params.len(),
        1,
        "no implied parent for an optional relate"
    );
    let d = derive(&model);
    assert_eq!(d.rows("Charge"), vec!["status=posted", "status=billed"]);
    assert_eq!(d.rows("Claim"), vec!["status=draft", "status=sent"]);
    let charge = d.entity("Charge");
    let axes: Vec<&str> = charge.axes.iter().map(|a| a.column.as_str()).collect();
    assert_eq!(
        axes,
        vec!["status"],
        "the foreign key is the owner var, not an axis"
    );
}

#[test]
fn an_unlinked_optional_owner_is_not_linked_for_along() {
    let src = format!(
        "{}\ninvariant(Charge, Claim).along(Charge, Claim).when(Claim.status == sent).then(Charge.status == billed)\nforbidden(Charge, Claim, Charge.status == posted, Claim.status == sent).along(Charge, Claim)",
        billing("relate(Charge, Claim, N:1).optional()")
    );
    // Two slots: the second charge is created after the claim and stays unlinked.
    let d = derive_scoped(&model_from(&src), Bounds::VERIFY, &Scope::ALL);
    assert!(
        !d.has("Charge", |x| matches!(
            x,
            StateDiag::ConstraintViolated { .. }
        )),
        "a posted charge is never linked to a claim: {:?}",
        d.entity("Charge").diagnostics
    );
    let global = format!(
        "{}\nforbidden(Charge, Claim, Charge.status == posted, Claim.status == sent)",
        billing("relate(Charge, Claim, N:1).optional()")
    );
    let d = derive_scoped(&model_from(&global), Bounds::VERIFY, &Scope::ALL);
    assert!(
        d.has("Charge", |x| matches!(
            x,
            StateDiag::ConstraintViolated {
                kind: InvKind::CrossForbidden,
                ..
            }
        )),
        "without .along any pair counts: {:?}",
        d.entity("Charge").diagnostics
    );
}

/// A revoked certificate is never assigned, so no session reaches it through a terminal.
fn session_path(along: &str) -> String {
    format!(
        r#"
entity AccessSession "セッション" {{ id: Int @pk  status: Enum(open, closed) @default(open) }}
entity Terminal "端末" {{ id: Int @pk }}
entity TerminalCertAssignment "割当" {{ id: Int @pk  status: Enum(live) @default(live) }}
entity ClientCertificate "証明書" {{ id: Int @pk  status: Enum(active, revoked) @default(active) }}
usecase AddTerminal "端末登録"
usecase IssueCert "発行"
usecase AssignCert "割当"
usecase OpenSession "接続"
usecase Revoke "失効"
creates(AddTerminal, Terminal)
creates(IssueCert, ClientCertificate)
creates(AssignCert, TerminalCertAssignment)
creates(OpenSession, AccessSession)
relate(AccessSession, Terminal, N:1)
relate(TerminalCertAssignment, Terminal, N:1)
relate(TerminalCertAssignment, ClientCertificate, N:1)
transitions(Revoke, ClientCertificate.status, active -> revoked)
condition CertActive "有効な証明書だけ割り当てる"
holds(CertActive, ClientCertificate.status == active)
applies_to(CertActive, AssignCert)
condition Unassigned "割り当てのない証明書だけ失効できる"
holds(Unassigned, ClientCertificate.status == active).none(TerminalCertAssignment.status == live)
applies_to(Unassigned, Revoke)
forbidden(AccessSession, ClientCertificate, AccessSession.status == open, ClientCertificate.status == revoked){along}
"#
    )
}

fn cross_forbidden_violated(d: &Derived) -> bool {
    d.has("AccessSession", |x| {
        matches!(
            x,
            StateDiag::ConstraintViolated {
                kind: InvKind::CrossForbidden,
                ..
            }
        )
    })
}

#[test]
fn along_binds_the_path_entities_the_scope_leaves_out() {
    let along = ".along(AccessSession, Terminal, TerminalCertAssignment, ClientCertificate)";
    let d = derive(&model_from(&session_path(along)));
    assert!(
        !d.has("AccessSession", |x| matches!(
            x,
            StateDiag::UnbindableScope { .. }
        )),
        "{:?}",
        d.entity("AccessSession").diagnostics
    );
    assert!(
        !cross_forbidden_violated(&d),
        "no session reaches a revoked certificate: {:?}",
        d.entity("AccessSession").diagnostics
    );
    let global = derive(&model_from(&session_path("")));
    assert!(
        cross_forbidden_violated(&global),
        "without .along an unassigned revoked certificate counts: {:?}",
        global.entity("AccessSession").diagnostics
    );
}

#[test]
fn quantifier_ignores_unlinked_related_rows() {
    let src = r#"
entity Cert "証明書" { id: Int @pk  status: Enum(active, revoked) @default(active) }
entity Assign "割当" { id: Int @pk  status: Enum(active, ended) @default(active) }
usecase Issue "発行"
usecase Revoke "失効"
usecase Spare "予備"
creates(Issue, Cert)
creates(Spare, Assign)
transitions(Revoke, Cert.status, active -> revoked)
relate(Assign, Cert, N:1).optional()
invariant(Cert).when(status == revoked).none(Assign.status == active)
"#;
    let d = derive_src(src);
    assert!(
        !d.has("Cert", |x| matches!(
            x,
            StateDiag::ConstraintViolated { .. }
        )),
        "the spare assignment belongs to no cert: {:?}",
        d.entity("Cert").diagnostics
    );
}

#[test]
fn owner_var_names_carry_the_role_only_when_one_is_given() {
    let model = model_from(
        r#"
entity Address "Address" { id: Int @pk }
entity Order "Order" { id: Int @pk }
usecase Place "Place"
creates(Place, Order)
relate(Order, Address, N:1)
relate(Order, Address, N:1).role(bill_to).optional()
"#,
    );
    let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).ts;
    let owners: Vec<String> = ts
        .var_ids()
        .map(|v| ts.var_name(v))
        .filter(|n| n.contains("._owner_"))
        .collect();
    assert_eq!(owners, ["Order._owner_Address", "Order._owner_bill_to"]);
}

#[test]
fn an_adopted_key_column_is_the_owner_not_a_state_column() {
    let src = |relate: &str| {
        format!(
            r#"
entity Order "Order" {{ id: Int @pk }}
entity Shipment "Shipment" {{ id: Int @pk  order_ref: Int @null }}
usecase Ship "Ship"
creates(Ship, Shipment)
{relate}
"#
        )
    };
    let names = |relate: &str| {
        let model = model_from(&src(relate));
        let ts = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).ts;
        ts.var_ids().map(|v| ts.var_name(v)).collect::<Vec<_>>()
    };
    let generated = names("relate(Shipment, Order, N:1).optional()");
    assert!(generated.iter().any(|n| n == "order_ref"), "{generated:?}");
    let adopted = names("relate(Shipment, Order, N:1).optional().key(order_ref)");
    assert!(!adopted.iter().any(|n| n == "order_ref"), "{adopted:?}");
    assert!(adopted.iter().any(|n| n == "Shipment._owner_Order"));
}

/// Profile is master data; Account is too unless `parent_birth` creates it.
fn master(relate: &str, parent_birth: &str) -> String {
    format!(
        r#"
entity Account "Account" {{ id: Int @pk }}
entity Profile "Profile" {{ id: Int @pk }}
usecase Open "Open"
{parent_birth}
{relate}
"#
    )
}

/// Profile's owner in each initial state at two slots, as parent slots (`None`: unlinked).
fn initial_owners(src: &str) -> (Vec<Vec<Option<u8>>>, Vec<Diag>) {
    let built = TransitionSystem::build(&model_from(src), Bounds::VERIFY, &Scope::ALL);
    let mut idle = built.ts;
    // Without actions the reachable states are the initial states.
    idle.actions.clear();
    let profile = idle
        .entity_ids()
        .find(|e| idle.entity(*e).id == "Profile")
        .unwrap();
    let owner = idle.entity(profile).owners[0];
    let domain = idle.var(owner).domain.clone();
    let run = idle.run(Caps::new(Caps::DEFAULT_PATTERNS, 100));
    let mut out: Vec<Vec<Option<u8>>> = run
        .states
        .iter()
        .map(|s| {
            (0..2)
                .map(|slot| domain.slot(s.get(&run.system, owner, slot)))
                .collect()
        })
        .collect();
    out.sort();
    (out, built.diags)
}

#[test]
fn a_pre_existing_n_to_one_child_starts_under_any_parent() {
    let (starts, _) = initial_owners(&master("relate(Profile, Account, N:1)", ""));
    let s = |a, b| vec![Some(a), Some(b)];
    assert_eq!(starts, [s(0, 0), s(0, 1), s(1, 0), s(1, 1)]);
    let (starts, _) = initial_owners(&master("relate(Profile, Account, N:1).optional()", ""));
    assert_eq!(
        starts.len(),
        9,
        "unlinked or either slot, per child: {starts:?}"
    );
}

#[test]
fn a_pre_existing_one_to_one_child_starts_under_its_own_slot() {
    for relate in [
        "relate(Profile, Account, 1:1)",
        "relate(Profile, Account, 1:1).optional()",
    ] {
        let (starts, diags) = initial_owners(&master(relate, ""));
        assert_eq!(starts, [vec![Some(0), Some(1)]], "{relate}");
        assert!(
            !diags
                .iter()
                .any(|d| matches!(d.diag, StateDiag::PreExistingChildOfCreatedParent { .. })),
            "{relate}: {diags:?}"
        );
    }
}

#[test]
fn a_pre_existing_child_of_a_created_parent_is_warned() {
    let created = "creates(Open, Account)";
    let (_, diags) = initial_owners(&master("relate(Profile, Account, 1:1)", created));
    assert!(
        diags.iter().any(|d| matches!(
            &d.diag,
            StateDiag::PreExistingChildOfCreatedParent { parent, .. } if parent == "Account"
        )),
        "{diags:?}"
    );
    // An optional key has a legitimate start: unlinked until the parent exists.
    let (starts, diags) =
        initial_owners(&master("relate(Profile, Account, N:1).optional()", created));
    assert_eq!(starts, [vec![None, None]]);
    assert!(!diags
        .iter()
        .any(|d| matches!(d.diag, StateDiag::PreExistingChildOfCreatedParent { .. })));
}

fn unique_violated(d: &Derived, child: &str) -> bool {
    d.diags.iter().any(|x| {
        matches!(&x.diag, StateDiag::ConstraintViolated { name, kind: InvKind::Unique, .. }
            if *name == format!("Unique_{child}_Order"))
    })
}

#[test]
fn two_children_of_one_parent_break_a_one_to_one_key_only_at_two_slots() {
    let model = sample_model("samples/formal-verification/one_to_one_shared.rdra");
    assert!(unique_violated(
        &derive_scoped(&model, Bounds::VERIFY, &Scope::ALL),
        "Shipment"
    ));
    assert!(
        !unique_violated(&derive(&model), "Shipment"),
        "vacuous at one slot"
    );
}

#[test]
fn unlinked_children_share_no_parent_and_identity_starts_hold() {
    let src = |relate: &str, births: &str| {
        format!(
            "entity Order \"Order\" {{ id: Int @pk }}\nentity Note \"Note\" {{ id: Int @pk }}\nusecase Write \"Write\"\n{births}\n{relate}\n"
        )
    };
    // Two notes are created and never linked.
    let unlinked = src(
        "relate(Note, Order, 1:1).optional()",
        "creates(Write, Note)",
    );
    let d = derive_scoped(&model_from(&unlinked), Bounds::VERIFY, &Scope::ALL);
    assert!(!unique_violated(&d, "Note"), "{:?}", d.diags);
    let master = src("relate(Note, Order, 1:1)", "");
    let d = derive_scoped(&model_from(&master), Bounds::VERIFY, &Scope::ALL);
    assert!(!unique_violated(&d, "Note"), "{:?}", d.diags);
}

fn min_card_violated(d: &Derived) -> bool {
    d.diags.iter().any(|x| {
        matches!(&x.diag, StateDiag::ConstraintViolated { name, kind: InvKind::MinCard, .. }
            if name == "MinCard_OrderLine_Order")
    })
}

fn needs_create(d: &Derived) -> Vec<&str> {
    d.diags
        .iter()
        .filter_map(|x| match &x.diag {
            StateDiag::MinCardNeedsCreate { usecase, .. } => Some(usecase.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_parent_created_without_a_child_breaks_the_minimum_at_one_slot() {
    let model = sample_model("samples/formal-verification/min_card_unmet.rdra");
    for d in [
        derive(&model),
        derive_scoped(&model, Bounds::VERIFY, &Scope::ALL),
    ] {
        assert!(min_card_violated(&d), "{:?}", d.diags);
        assert_eq!(needs_create(&d), ["Place"]);
    }
    // `check` reads the views: Order has no state column, and its view still reports it.
    let order = derive(&model);
    assert!(order.entity("Order").diagnostics.iter().any(|x| matches!(
        x,
        StateDiag::ConstraintViolated {
            kind: InvKind::MinCard,
            ..
        }
    )));
}

#[test]
fn a_parent_created_with_its_first_child_meets_the_minimum_until_a_delete() {
    let path = "skills/rdra-ish-verify/samples/min_card_met.rdra";
    let model = sample_model(path);
    for d in [
        derive(&model),
        derive_scoped(&model, Bounds::VERIFY, &Scope::ALL),
    ] {
        assert!(!min_card_violated(&d), "{:?}", d.diags);
        assert!(needs_create(&d).is_empty(), "{:?}", d.diags);
    }
    let src = std::fs::read_to_string(repo_root().join(path)).unwrap();
    let deleting =
        format!("{src}usecase RemoveLine \"明細削除\"\ndeletes(RemoveLine, OrderLine)\n");
    assert!(min_card_violated(&derive_src(&deleting)));
}

#[test]
fn a_pre_existing_parent_of_a_created_child_is_warned_and_fails_at_the_start() {
    let src = "entity Order \"O\" { id: Int @pk }\nentity OrderLine \"L\" { id: Int @pk }\nusecase AddLine \"A\"\ncreates(AddLine, OrderLine)\nrelate(OrderLine, Order, 1..*:1)\n";
    let d = derive_src(src);
    assert!(min_card_violated(&d), "{:?}", d.diags);
    assert!(d
        .diags
        .iter()
        .any(|x| matches!(&x.diag, StateDiag::MinCardOfPreExistingParent { child, .. } if child == "OrderLine")));
    // Master data on both sides starts linked one to one, so it holds at any slot count.
    let master = model_from("entity Order \"O\" { id: Int @pk }\nentity OrderLine \"L\" { id: Int @pk }\nrelate(OrderLine, Order, 1..*:1)\n");
    assert!(!min_card_violated(&derive(&master)));
    let d = derive_scoped(&master, Bounds::VERIFY, &Scope::ALL);
    assert!(!min_card_violated(&d), "{:?}", d.diags);
}

/// GenerateClaim creates a claim and updates an existing charge, so it links the charge.
fn relinking(relate: &str, extra: &str) -> String {
    format!(
        r#"
entity Claim "請求" {{ id: Int @pk  origin: Enum(opened, generated) @default(opened) }}
entity Charge "料金" {{ id: Int @pk }}
usecase CreateCharge "計上"
usecase GenerateClaim "請求作成"
creates(CreateCharge, Charge)
creates(GenerateClaim, Claim)
updates(GenerateClaim, Charge)
sets(GenerateClaim, Claim.origin == generated)
{relate}
{extra}
"#
    )
}

/// Every reachable `(claims present, charges present)` pair at two slots.
fn populations(src: &str) -> Vec<(usize, usize)> {
    let ts = TransitionSystem::build(&model_from(src), Bounds::VERIFY, &Scope::ALL).ts;
    let run = ts.run(Caps::new(Caps::DEFAULT_PATTERNS, 100_000));
    let ts = &run.system;
    let exists = |id: &str| {
        let e = ts.entity_ids().find(|e| ts.entity(*e).id == id).unwrap();
        ts.entity(e).exists
    };
    let (claim, charge) = (exists("Claim"), exists("Charge"));
    let mut out: Vec<(usize, usize)> = run
        .states
        .iter()
        .map(|s| {
            let present = |var| {
                (0..2)
                    .filter(|&i| s.get(ts, var, i) == Value::PRESENT)
                    .count()
            };
            (present(claim), present(charge))
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

#[test]
fn optional_child_is_linked_only_when_unlinked() {
    let pops = populations(&relinking("relate(Charge, Claim, N:1).optional()", ""));
    assert!(pops.contains(&(2, 2)), "{pops:?}");
    assert!(
        !pops.contains(&(2, 1)),
        "the second claim cannot take the first claim's charge: {pops:?}"
    );
}

#[test]
fn mandatory_child_moves_to_a_created_parent() {
    // OpenClaim creates the first claim; every charge then has one, so a link is a move.
    let src = relinking(
        "relate(Charge, Claim, N:1)",
        "usecase OpenClaim \"開設\"\ncreates(OpenClaim, Claim)",
    );
    let d = derive_scoped(&model_from(&src), Bounds::VERIFY, &Scope::ALL);
    assert_eq!(d.rows("Claim"), vec!["origin=opened", "origin=generated"]);
    assert!(
        populations(&src).contains(&(2, 1)),
        "the charge moved to the second claim"
    );
}

#[test]
fn a_creating_action_that_links_an_existing_child_needs_no_create_hint() {
    let d = derive_src(&relinking("relate(Charge, Claim, 1..*:1).optional()", ""));
    assert!(needs_create(&d).is_empty(), "{:?}", d.diags);
}

/// A case is opened unlinked and `Assign` links it to a handler (#89). `{relate}` is the key,
/// `{assign}` what else Assign does.
fn assignment(relate: &str, assign: &str) -> String {
    format!(
        r#"
entity Handler "担当" {{ id: Int @pk  busy: Bool @default(false) }}
entity Case "案件" {{ id: Int @pk  status: Enum(new, assigned) @default(new) }}
{relate}
usecase OpenCase "受付"
usecase Assign "担当を割り当てる"
creates(OpenCase, Case)
transitions(Assign, Case.status, new -> assigned)
links(Assign, Case, Handler)
{assign}
"#
    )
}

fn never_runs(d: &Derived) -> Vec<String> {
    d.diags
        .iter()
        .filter_map(|x| match &x.diag {
            StateDiag::NeverRuns { action, .. } => Some(action.clone()),
            _ => None,
        })
        .collect()
}

/// `links` binds any present parent and links the child to it: the assigned case has a
/// handler, which a `.has(Handler)` on it sees. Its guard is that the child is unlinked, so a
/// second `links` on an assigned case never runs, and says so by the `links` guard.
#[test]
fn a_links_binds_a_present_parent_and_links_the_child_once() {
    let src = assignment(
        "relate(Case, Handler, N:1).optional()",
        "invariant(Case).when(status == assigned).has(Handler)\n\
         usecase Reassign \"担当を替える\"\nlinks(Reassign, Case, Handler)\n\
         condition Assigned \"割当済\"\nholds(Assigned, Case.status == assigned)\napplies_to(Assigned, Reassign)",
    );
    let d = derive_src(&src);
    assert_eq!(d.rows("Case"), vec!["status=new", "status=assigned"]);
    assert!(
        !d.has("Case", |x| matches!(
            x,
            StateDiag::ConstraintViolated { .. }
        )),
        "{:?}",
        d.diags
    );
    assert_eq!(never_runs(&d), vec!["Reassign"]);
    let ts = TransitionSystem::build(&model_from(&src), Bounds::STATES, &Scope::ALL).ts;
    let reassign = ts.actions().iter().find(|a| a.name == "Reassign").unwrap();
    assert!(reassign.guards.iter().any(|g| g.why == GuardWhy::Link));
    assert!(reassign
        .params
        .iter()
        .any(|p| matches!(p.role, ParamRole::Parent { .. })));
}

/// The parent the use case writes is the one the child is linked to: Assign marks the
/// handler busy, and the assigned case's handler is that one.
#[test]
fn a_links_to_a_written_parent_links_the_row_the_step_writes() {
    let d = derive_src(&assignment(
        "relate(Case, Handler, N:1).optional()",
        "updates(Assign, Handler)\nsets(Assign, Handler.busy == true)\n\
         invariant(Case).when(status == assigned).has(Handler.busy == true)\n\
         invariant(Case).when(status == assigned).none(Handler.busy == false)",
    ));
    assert!(
        !d.has("Case", |x| matches!(
            x,
            StateDiag::ConstraintViolated { .. }
        )),
        "{:?}",
        d.diags
    );
}

/// An `.along` path through the linked key reaches the grandparent: the assigned case's
/// handler's team is open, which only a case linked to its handler can satisfy.
#[test]
fn a_links_is_followed_by_an_along_path_through_the_key() {
    let d = derive_src(&assignment(
        "relate(Case, Handler, N:1).optional()\n\
         entity Team \"班\" { id: Int @pk  open: Bool @default(true) }\n\
         relate(Handler, Team, N:1)\n\
         usecase Hire \"採用\"\ncreates(Hire, Handler)",
        "invariant(Case).when(status == assigned).has(Team.open == true).along(Case, Handler, Team)",
    ));
    assert_eq!(d.rows("Case"), vec!["status=new", "status=assigned"]);
    assert!(
        !d.has("Case", |x| matches!(
            x,
            StateDiag::ConstraintViolated { .. }
        )),
        "{:?}",
        d.diags
    );
}

/// `links` does not check that a 1:1 parent is free: two cases linked to one handler break
/// `Unique_Case_Handler` at two slots, as a guard with `.none(Case)` on the handler would
/// prevent.
#[test]
fn a_links_on_a_one_to_one_key_leaves_uniqueness_to_the_rule() {
    let src = assignment("relate(Case, Handler, 1:1).optional()", "");
    let d = derive_scoped(&model_from(&src), Bounds::VERIFY, &Scope::ALL);
    assert!(d.diags.iter().any(|x| matches!(
        &x.diag,
        StateDiag::ConstraintViolated { name, kind: InvKind::Unique, .. } if name == "Unique_Case_Handler"
    )));
}

/// In a split use case the child links to the row an earlier step of the run wrote: the
/// handler step 1 marks busy is the one step 2 links the case to.
#[test]
fn a_links_in_a_later_step_links_to_the_runs_parent() {
    let src = assignment(
        "relate(Case, Handler, N:1).optional()",
        "api HandlerApi \"handler\"\napi CaseApi \"case\"\n\
         invokes(Assign, HandlerApi)\ninvokes(Assign, CaseApi)\n\
         updates(HandlerApi, Handler)\nupdates(CaseApi, Case)\n\
         sets(Assign, Handler.busy == true)\n\
         invariant(Case).when(status == assigned).has(Handler.busy == true)",
    );
    let ts = TransitionSystem::build(&model_from(&src), Bounds::STATES, &Scope::ALL).ts;
    let step_2 = ts.actions().iter().find(|a| a.name == "Assign_2").unwrap();
    let earlier = step_2
        .params
        .iter()
        .position(|p| matches!(p.role, ParamRole::Earlier { step: 1 }))
        .expect("the handler step 1 wrote") as u8;
    assert!(step_2.effects.iter().any(|e| matches!(
        e,
        Effect::Assign { rhs: Rhs::Param(p), .. } if *p == earlier
    )));
    let d = derive_src(&src);
    assert!(
        !d.has("Case", |x| matches!(
            x,
            StateDiag::ConstraintViolated { .. }
        )),
        "{:?}",
        d.diags
    );
}
