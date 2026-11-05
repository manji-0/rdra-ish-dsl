//! `holds(..).along(A, B, C)` (#77): a guard reads the row a key path joins to the rows the
//! use case binds, and a guard that reads an entity no key joins to them is
//! `UnjoinedGuardRead`.

use super::*;

/// A claim and a settlement are both children of a case, which has one settlement at most:
/// opening it moves the case. `Complete` runs while `{GUARD}`; the rule forbids a completed
/// claim whose case's settlement is not agreed.
const CASE: &str = r#"
entity Case "案件" { id: Int @pk  status: Enum(open, negotiating) @default(open) }
entity Claim "請求" { id: Int @pk  status: Enum(submitted, completed) @default(submitted) }
entity Settlement "示談" { id: Int @pk  status: Enum(negotiating, agreed) @default(negotiating) }
relate(Claim, Case, N:1)
relate(Settlement, Case, N:1)
usecase Open "受付"
usecase Submit "請求"
usecase Negotiate "示談開始"
usecase Agree "示談成立"
usecase Complete "請求完了"
creates(Open, Case)
creates(Submit, Claim)
creates(Negotiate, Settlement)
transitions(Negotiate, Case.status, open -> negotiating)
transitions(Agree, Settlement.status, negotiating -> agreed)
transitions(Complete, Claim.status, submitted -> completed)
condition Signed "示談書がある"
holds(Signed, Settlement.status == agreed){GUARD}
applies_to(Signed, Complete)
forbidden(Claim, Settlement, Claim.status == completed, Settlement.status != agreed).along(Claim, Case, Settlement)
"#;

fn case(guard: &str) -> SemanticModel {
    model_from(&CASE.replace("{GUARD}", guard))
}

fn violated(model: &SemanticModel) -> bool {
    derive_scoped(model, Bounds::VERIFY, &Scope::ALL)
        .diags
        .iter()
        .any(|x| matches!(&x.diag, StateDiag::ConstraintViolated { name, .. } if name == "MultiForbidden_Claim_0"))
}

fn unjoined(model: &SemanticModel) -> Vec<StateDiag> {
    TransitionSystem::build(model, Bounds::STATES, &Scope::ALL)
        .diags
        .into_iter()
        .map(|d| d.diag)
        .filter(|d| matches!(d, StateDiag::UnjoinedGuardRead { .. }))
        .collect()
}

/// Along the case, the guard reads the settlement of the claim's own case, so the rule holds at
/// two instances. Without it, an agreed settlement of another case completes the claim.
#[test]
fn a_guard_along_a_path_reads_the_row_of_its_own_case() {
    assert!(!violated(&case(".along(Claim, Case, Settlement)")));
    assert!(violated(&case("")));
}

/// The guard with no path reads a settlement anywhere, and says so: the entity, the use case,
/// and the `.along` that joins it.
#[test]
fn a_guard_on_an_unjoined_entity_warns_with_the_path() {
    let warned = unjoined(&case(""));
    let [StateDiag::UnjoinedGuardRead {
        condition,
        usecase,
        entity,
        fix,
        ..
    }] = warned.as_slice()
    else {
        panic!("{warned:?}");
    };
    assert_eq!(
        (condition.as_str(), usecase.as_str(), entity.as_str()),
        ("Signed", "Complete", "Settlement")
    );
    assert!(fix.contains("`.along(Claim, Case, Settlement)`"), "{fix}");
    assert!(fix.contains("write Settlement in Complete"), "{fix}");
    assert_eq!(
        StateDiag::UnjoinedGuardRead {
            condition: condition.clone(),
            usecase: usecase.clone(),
            entity: entity.clone(),
            fix: fix.clone(),
            site: crate::lifecycle::Site::bare(crate::lifecycle::ModelRef::Synthesized),
        }
        .severity(),
        crate::lifecycle::Severity::Warning
    );
    assert!(unjoined(&case(".along(Claim, Case, Settlement)")).is_empty());
}

/// An entity one key joins to a row the use case binds needs no path.
#[test]
fn a_guard_one_key_joins_does_not_warn() {
    let src = CASE
        .replace("{GUARD}", "")
        .replace("Settlement.status == agreed", "Case.status == open");
    assert!(unjoined(&model_from(&src)).is_empty());
}

/// A path that leaves out a hop no key joins says why the guard binds nothing.
#[test]
fn a_path_with_an_unrelated_hop_is_unbindable() {
    let model = case(".along(Claim, Settlement)");
    let diags = TransitionSystem::build(&model, Bounds::STATES, &Scope::ALL).diags;
    assert!(
        diags.iter().any(|d| matches!(&d.diag,
            StateDiag::UnbindableCondition { reason, .. } if reason.contains("binds Claim and Settlement"))),
        "{diags:?}"
    );
}

/// A claim can move to another case (`moved_from`), so two keys join a claim and a case; a
/// `.role` after the `.along` picks the hop's key, and the verdict follows it.
#[test]
fn a_by_after_along_picks_the_key_of_a_hop() {
    let src = CASE
        .replace(
            "relate(Claim, Case, N:1)",
            "relate(Claim, Case, N:1).role(home)\nrelate(Claim, Case, N:1).role(moved_from)",
        )
        .replace(
            "forbidden(Claim, Settlement, Claim.status == completed, Settlement.status != agreed).along(Claim, Case, Settlement)",
            "forbidden(Claim, Settlement, Claim.status == completed, Settlement.status != agreed).along(Claim, Case, Settlement).role(home)",
        );
    let along = |by: &str| {
        model_from(&src.replace(
            "{GUARD}",
            &format!(".along(Claim, Case, Settlement).role({by})"),
        ))
    };
    assert!(!violated(&along("home")));
    assert!(violated(&along("moved_from")));
}

/// `list --kind condition` and the diagrams print the path.
#[test]
fn the_body_display_prints_the_path() {
    let model = case(".along(Claim, Case, Settlement)");
    let ck = model
        .conditions
        .iter()
        .find(|(_, c)| c.id == "Signed")
        .map(|(k, _)| k)
        .expect("Signed");
    assert_eq!(
        crate::analysis::condition_body_display(&model, ck).as_deref(),
        Some("(Settlement.status == agreed).along(Claim, Case, Settlement)")
    );
}

#[test]
fn a_guard_along_a_path_keeps_the_cone_exact() {
    let model = case(".along(Claim, Case, Settlement)");
    let (exact, _) = super::cone::assert_cones_exact("along_guard", &model, Bounds::VERIFY);
    assert!(exact > 0);
}

/// A use case that creates the child of an `.optional()` key links it to the parent its
/// guard's path goes through, as a guard that compares the parent does; with no path the child
/// is created unlinked.
#[test]
fn a_path_through_an_optional_key_links_the_created_child() {
    use crate::lifecycle::types::{Effect, ParamRole, Rhs};
    let linked = |guard: &str| {
        let src = CASE
            .replace("{GUARD}", guard)
            .replace(
                "relate(Claim, Case, N:1)",
                "relate(Claim, Case, N:1).optional()",
            )
            .replace("applies_to(Signed, Complete)", "applies_to(Signed, Submit)");
        let ts = TransitionSystem::build(&model_from(&src), Bounds::VERIFY, &Scope::ALL).ts;
        let submit = ts
            .actions()
            .iter()
            .find(|a| a.name == "Submit")
            .expect("Submit");
        submit.effects.iter().find_map(|e| match e {
            Effect::Assign { rhs, .. } => Some(match rhs {
                Rhs::Param(p) => matches!(
                    submit.params[usize::from(*p)].role,
                    ParamRole::Parent { .. }
                ),
                _ => false,
            }),
            _ => None,
        })
    };
    assert_eq!(linked(".along(Claim, Case, Settlement)"), Some(true));
    assert_eq!(linked(""), Some(false));
}

/// A member withdraws while no loan of theirs is open. `{PATH}` is the `.along` of the guard,
/// `{EXTRA}` any further statement.
const LIBRARY: &str = r#"
entity Member "会員" { id: Int @pk  status: Enum(active, withdrawn) @default(active) }
entity Card "カード" { id: Int @pk  status: Enum(valid, lost) @default(valid) }
entity Loan "貸出" { id: Int @pk  status: Enum(open, returned) @default(open) }
relate(Card, Member, N:1)
relate(Loan, Card, N:1)
usecase Register "登録"
usecase IssueCard "カード発行"
usecase Lend "貸出"
usecase Return "返却"
usecase Withdraw "退会"
creates(Register, Member)
creates(IssueCard, Card)
creates(Lend, Loan)
transitions(Return, Loan.status, open -> returned)
transitions(Withdraw, Member.status, active -> withdrawn)
condition MemberActive "有効な会員"
holds(MemberActive, Member.status == active)
applies_to(MemberActive, IssueCard)
condition CardOfActive "有効な会員のカード"
holds(CardOfActive, Member.status == active).along(Loan, Card, Member)
applies_to(CardOfActive, Lend)
condition NoOpenLoan "未返却なし"
holds(NoOpenLoan, Member.status == active).none(Loan.status == open){PATH}
applies_to(NoOpenLoan, Withdraw)
{EXTRA}
"#;

fn library(path: &str, extra: &str) -> SemanticModel {
    model_from(&LIBRARY.replace("{PATH}", path).replace("{EXTRA}", extra))
}

/// Every diagnostic of the model at two instances, each target on its own cone as `verify`
/// explores them: the build's, the verdicts, and each entity's state-model findings.
fn verify_diags(model: &SemanticModel) -> Vec<StateDiag> {
    let built = TransitionSystem::build(model, Bounds::VERIFY, &Scope::ALL);
    let run = built.ts.explore_targets(
        &built.selection.targets,
        Caps::new(Caps::DEFAULT_PATTERNS, 1_000_000),
    );
    let views = run.entity_views(&built.ts, model, &built.diags);
    built
        .diags
        .iter()
        .chain(run.verdicts())
        .map(|d| d.diag.clone())
        .chain(views.into_iter().flat_map(|v| v.diagnostics))
        .collect()
}

fn deadlocked(model: &SemanticModel, entity: &str) -> bool {
    verify_diags(model)
        .iter()
        .any(|d| matches!(d, StateDiag::Deadlock { entity: e, .. } if e == entity))
}

/// `.none(Loan.status == open).along(Member, Card, Loan)` counts the loans of the member's
/// cards: a member with no card or no loan withdraws (#81). The path entities are the rows
/// the quantifier counts, not witnesses that must exist.
#[test]
fn a_none_along_a_path_holds_when_no_row_is_on_the_path() {
    assert!(!deadlocked(
        &library(".along(Member, Card, Loan)", ""),
        "Member"
    ));
}

/// Along the path, the guard counts the loans of the member's own cards: no member withdraws
/// while a loan on one of their cards is open. The rule sees the violation once the guard is
/// off the use case.
#[test]
fn a_none_along_a_path_counts_the_rows_of_its_own_chain() {
    let rule = "forbidden(Member, Loan, Member.status == withdrawn, Loan.status == open).along(Member, Card, Loan)";
    let violated = |model: &SemanticModel| {
        verify_diags(model).iter().any(|d| {
            matches!(d, StateDiag::ConstraintViolated { name, .. } if name == "MultiForbidden_Member_0")
        })
    };
    assert!(!violated(&library(".along(Member, Card, Loan)", rule)));
    let unguarded = LIBRARY
        .replace("{PATH}", ".along(Member, Card, Loan)")
        .replace("applies_to(NoOpenLoan, Withdraw)", "")
        .replace("{EXTRA}", rule);
    assert!(violated(&model_from(&unguarded)));
}

/// `.has(Loan.status == open).along(Member, Card, Loan)` needs an open loan on the member's
/// chain: `Remind` never runs while every loan is returned from the start, and runs once a
/// loan opens.
#[test]
fn a_has_along_a_path_needs_a_row_on_the_path() {
    let never_runs = |default: &str| {
        let src = LIBRARY
            .replace("{PATH}", "")
            .replace(
                "Enum(open, returned) @default(open)",
                &format!("Enum(open, returned) @default({default})"),
            )
            .replace(
                "status: Enum(active, withdrawn) @default(active) }",
                "status: Enum(active, withdrawn) @default(active)  reminded: Bool @default(false) }",
            )
            .replace(
                "{EXTRA}",
                r#"usecase Remind "督促"
sets(Remind, Member.reminded == true)
condition HasOpenLoan "未返却あり"
holds(HasOpenLoan, Member.status == active).has(Loan.status == open).along(Member, Card, Loan)
applies_to(HasOpenLoan, Remind)"#,
            );
        verify_diags(&model_from(&src))
            .iter()
            .any(|d| matches!(d, StateDiag::NeverRuns { action, .. } if action == "Remind"))
    };
    assert!(never_runs("returned"));
    assert!(!never_runs("open"));
}

/// A path that puts the counted entity before any anchor, goes on past it, or joins a compared
/// entity through it binds the guard wrong, and says how to write it.
#[test]
fn a_counted_entity_out_of_place_on_the_path_is_unbindable() {
    let reasons = |path: &str, body: &str| -> String {
        let src = LIBRARY
            .replace("{PATH}", path)
            .replace("holds(NoOpenLoan, Member.status == active)", body)
            .replace(
                "{EXTRA}",
                r#"entity Fine "延滞金" { id: Int @pk  paid: Bool @default(false) }
relate(Fine, Loan, N:1)
usecase Charge "請求"
creates(Charge, Fine)"#,
            );
        TransitionSystem::build(&model_from(&src), Bounds::STATES, &Scope::ALL)
            .diags
            .into_iter()
            .filter_map(|d| match d.diag {
                StateDiag::UnbindableCondition { reason, .. } => Some(reason),
                _ => None,
            })
            .collect::<Vec<String>>()
            .join("\n")
    };
    let body = "holds(NoOpenLoan, Member.status == active)";
    let before = reasons(".along(Loan, Card, Member)", body);
    assert!(
        before.contains("names Loan, which `.none` counts, before any entity Withdraw binds"),
        "{before}"
    );
    let past = reasons(".along(Member, Card, Loan, Fine)", body);
    assert!(
        past.contains("goes on past Loan, which `.none` counts, to Fine"),
        "{past}"
    );
    let compares = "holds(NoOpenLoan, Member.status == active, Fine.paid == true)";
    let through = reasons(".along(Member, Card, Loan, Fine)", compares);
    assert!(
        through.contains("joins Fine, which the body compares, through Loan"),
        "{through}"
    );
    for reasons in [&before, &past, &through] {
        assert_eq!(reasons.lines().count(), 1, "{reasons}");
    }
    assert!(reasons(".along(Member, Card, Loan)", body).is_empty());
}

/// Whether the rule `name` is violated at two instances.
fn rule_violated(model: &SemanticModel, name: &str) -> bool {
    verify_diags(model)
        .iter()
        .any(|d| matches!(d, StateDiag::ConstraintViolated { name: n, .. } if n == name))
}

/// The library model with `Withdraw` guarded along the path and `rule`, and the same without
/// the guard.
fn guarded_and_unguarded(rule: &str) -> (SemanticModel, SemanticModel) {
    let guarded = library(".along(Member, Card, Loan)", rule);
    let unguarded = LIBRARY
        .replace("{PATH}", ".along(Member, Card, Loan)")
        .replace("applies_to(NoOpenLoan, Withdraw)", "")
        .replace("{EXTRA}", rule);
    (guarded, model_from(&unguarded))
}

/// `invariant(Member).when(status == withdrawn).none(Loan.status == open).along(Member, Card,
/// Loan)` counts the loans of the member's own cards (#90). With `Withdraw` guarded, the rule
/// holds at two instances while another member's loan is open; read one hop from the anchor,
/// with no key between Member and Loan, it would count every loan and fail there. Without the
/// guard a member withdraws with an open loan on their chain, and the rule sees it.
#[test]
fn an_invariant_none_along_a_path_counts_the_rows_of_its_own_chain() {
    let (guarded, unguarded) = guarded_and_unguarded(
        "invariant(Member).when(status == withdrawn).none(Loan.status == open).along(Member, Card, Loan)",
    );
    assert!(!rule_violated(&guarded, "Quantifier_Member_0"));
    assert!(rule_violated(&unguarded, "Quantifier_Member_0"));
}

/// `invariant(Loan).when(status == open).has(Member.status == active).along(Loan, Card, Member)`
/// needs the member of the loan's own card to be active. Without the guard on `Withdraw`, that
/// member withdraws while the loan is open and the rule fails. (Both readings fail it there,
/// in the state where that member is the only one; the `.none` above is where they part.)
#[test]
fn an_invariant_has_along_a_path_needs_the_row_of_its_own_chain() {
    let (guarded, unguarded) = guarded_and_unguarded(
        "invariant(Loan).when(status == open).has(Member.status == active).along(Loan, Card, Member)",
    );
    assert!(!rule_violated(&guarded, "Quantifier_Loan_0"));
    assert!(rule_violated(&unguarded, "Quantifier_Loan_0"));
}

/// A hop of a quantified invariant's path that no key joins is `UnbindableScope` on the rule.
#[test]
fn a_quantifier_path_with_an_unrelated_hop_is_unbindable() {
    let rule = r#"entity Branch "支店" { id: Int @pk }
invariant(Member).when(status == withdrawn).none(Loan.status == open).along(Member, Branch, Loan)"#;
    let reasons: Vec<String> =
        TransitionSystem::build(&library("", rule), Bounds::STATES, &Scope::ALL)
            .diags
            .into_iter()
            .filter_map(|d| match d.diag {
                StateDiag::UnbindableScope { reason, .. } => Some(reason),
                _ => None,
            })
            .collect();
    assert_eq!(
        reasons,
        vec!["no N:1, 1:N, or 1:1 relate binds Member and Branch".to_string()]
    );
}
