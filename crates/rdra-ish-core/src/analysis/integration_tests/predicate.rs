use crate::analysis::build_model;
use crate::diagnostics::RdraError;
use crate::model::*;
use rdra_ish_syntax::parse;

#[test]
fn test_api_declaration_and_invokes() {
    let src = r#"
usecase PlaceOrder "注文する"
api OrderApi "注文API" description "注文を永続化するAPI"
invokes(PlaceOrder, OrderApi)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

    assert_eq!(model.apis.len(), 1);
    let api = model.apis.values().next().unwrap();
    assert_eq!(api.id, "OrderApi");
    assert_eq!(api.label, "注文API");
    assert_eq!(api.description.as_deref(), Some("注文を永続化するAPI"));

    let invokes_rel = model.relations.iter().find(|r| r.kind == RelKind::Invokes);
    assert!(invokes_rel.is_some(), "Invokes relation should exist");
}

#[test]
fn test_belongs_when_by_context() {
    let src = r#"
business ClinicOps "Clinic Operations"
buc BucAppointmentScheduling "Appointment Scheduling"
timing AppointmentRequested "Appointment Requested"
medium FrontDeskTerminal "Front Desk Terminal"
belongs(BucAppointmentScheduling, ClinicOps)
  .when("patient requests a booking")
  .when(AppointmentRequested)
  .by(FrontDeskTerminal)
  .by("tablet")
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

    let rel = model.relations.iter().find(|r| r.kind == RelKind::Belongs);
    assert!(rel.is_some(), "Belongs relation should still exist");

    assert_eq!(model.business_mapping_contexts.len(), 1);
    let ctx = &model.business_mapping_contexts[0];
    assert_eq!(model.bucs[ctx.buc].id, "BucAppointmentScheduling");
    assert_eq!(model.businesses[ctx.business].id, "ClinicOps");
    assert_eq!(ctx.whens.len(), 2);
    assert_eq!(ctx.bys.len(), 2);

    assert!(matches!(
        &ctx.whens[0],
        BusinessMappingContextValue::Text(s) if s == "patient requests a booking"
    ));
    assert!(matches!(
        &ctx.whens[1],
        BusinessMappingContextValue::Ref(NodeRef::Timing(_))
    ));
    assert!(matches!(
        &ctx.bys[0],
        BusinessMappingContextValue::Ref(NodeRef::Medium(_))
    ));
    assert!(matches!(
        &ctx.bys[1],
        BusinessMappingContextValue::Text(s) if s == "tablet"
    ));
}

#[test]
fn test_actor_permission_attachment() {
    let src = r#"
actor Staff "Staff"
permission ManageSchedule "Manage Schedule"
has_permission(Staff, ManageSchedule)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

    assert_eq!(model.permissions.len(), 1);
    let permission = model.permissions.values().next().unwrap();
    assert_eq!(permission.id, "ManageSchedule");
    assert_eq!(permission.label, "Manage Schedule");

    let rel = model
        .relations
        .iter()
        .find(|r| r.kind == RelKind::HasPermission)
        .expect("HasPermission relation should exist");
    assert!(matches!(rel.from, NodeRef::Actor(_)));
    assert!(matches!(rel.to, NodeRef::Permission(_)));
}
#[test]
fn test_screen_constraint_patterns_derive_from_usecase_and_api() {
    let src = r#"
usecase BookAppointment "Book Appointment"
screen BookingScreen "Booking Screen"
api BookingApi "Booking API"
permission ScheduleWrite "Schedule Write"
permission PatientRead "Patient Read"
medium StaffTerminal "Staff Terminal"
medium SecureChannel "Secure Channel"
displays(BookAppointment, BookingScreen)
invokes(BookAppointment, BookingApi)
requires_permission(BookAppointment, ScheduleWrite)
requires_medium(BookAppointment, StaffTerminal)
requires_permission(BookingApi, PatientRead)
requires_medium(BookingApi, SecureChannel)
"#;
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);

    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

    let patterns = crate::derive_screen_constraint_patterns(&model);
    assert_eq!(patterns.len(), 1);

    let pattern = &patterns[0];
    assert_eq!(model.screens[pattern.screen].id, "BookingScreen");
    assert_eq!(model.use_cases[pattern.usecase].id, "BookAppointment");
    assert_eq!(
        model.apis[pattern.api.expect("api should be part of the path")].id,
        "BookingApi"
    );

    let permission_ids: Vec<_> = pattern
        .permissions
        .iter()
        .map(|key| model.permissions[*key].id.as_str())
        .collect();
    assert_eq!(permission_ids, vec!["ScheduleWrite", "PatientRead"]);

    let medium_ids: Vec<_> = pattern
        .media
        .iter()
        .map(|key| model.media[*key].id.as_str())
        .collect();
    assert_eq!(medium_ids, vec!["StaffTerminal", "SecureChannel"]);
}

#[test]
fn test_api_crud_type_check_ok() {
    let src = r#"
api OrderApi "注文API"
entity Order "注文" { id: Int @pk }
creates(OrderApi, Order)
"#;
    let (ast, _) = parse(src);
    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {:?}", errors);

    let creates_rel = model.relations.iter().find(|r| r.kind == RelKind::Creates);
    assert!(creates_rel.is_some());
}

#[test]
fn test_invokes_type_mismatch() {
    // invokes(uc, entity) は TypeMismatch になるはず
    let src = r#"
usecase PlaceOrder "注文する"
entity Order "注文" { id: Int @pk }
invokes(PlaceOrder, Order)
"#;
    let (ast, _) = parse(src);
    let (_, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(!errors.is_empty(), "type mismatch expected");
    assert!(errors[0].error.to_string().contains("type mismatch"));
}

#[test]
fn test_usecase_crud_still_allowed() {
    // 後方互換: usecase が直接 entity を creates しても OK
    let src = r#"
usecase PlaceOrder "注文する"
entity Order "注文" { id: Int @pk }
creates(PlaceOrder, Order)
"#;
    let (ast, _) = parse(src);
    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(
        errors.is_empty(),
        "legacy creates(uc, entity) should still work"
    );
    assert_eq!(
        model
            .relations
            .iter()
            .filter(|r| r.kind == RelKind::Creates)
            .count(),
        1
    );
}

#[test]
fn unknown_predicate_is_error() {
    let src = r#"
actor A "a"
usecase U "u"
totally_bogus(A, U)
"#;
    let (ast, _) = parse(src);
    let (_model, diags) = build_model(&ast);
    assert!(
        diags.iter().any(|d| matches!(
            &d.error,
            RdraError::UnknownPredicate { name } if name == "totally_bogus"
        )),
        "expected UnknownPredicate, got: {:?}",
        diags.iter().map(|d| &d.error).collect::<Vec<_>>()
    );
}

#[test]
fn wrong_arity_is_error() {
    let src = r#"
actor A "a"
performs(A)
"#;
    let (ast, _) = parse(src);
    let (_model, diags) = build_model(&ast);
    assert!(
        diags.iter().any(|d| matches!(
            &d.error,
            RdraError::WrongArity { name, .. } if name == "performs"
        )),
        "expected WrongArity, got: {:?}",
        diags.iter().map(|d| &d.error).collect::<Vec<_>>()
    );
}

fn unknown_chain_errors(src: &str) -> Vec<(String, String, String)> {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let (_, diags) = build_model(&ast);
    diags
        .iter()
        .filter(|d| !d.is_warning)
        .filter_map(|d| match &d.error {
            RdraError::UnknownChainMethod { pred, name, hint } => {
                Some((pred.clone(), name.clone(), hint.clone()))
            }
            _ => None,
        })
        .collect()
}

const BELONGS_BASE: &str = "business B \"b\"\nbuc U \"u\"\n";

#[test]
fn removed_where_on_belongs_is_unknown_chain_method() {
    let errs = unknown_chain_errors(&format!("{BELONGS_BASE}belongs(U, B).where(A)\n"));
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(
        (errs[0].0.as_str(), errs[0].1.as_str()),
        ("belongs", "where")
    );
    assert!(errs[0].2.contains("location"), "{}", errs[0].2);
}

#[test]
fn typo_chain_method_is_unknown_chain_method() {
    let errs = unknown_chain_errors(&format!("{BELONGS_BASE}belongs(U, B).wehn(T)\n"));
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(errs[0].1, "wehn");
    assert!(errs[0].2.contains(".when"), "{}", errs[0].2);
}

#[test]
fn predicate_without_chain_methods_rejects_any_chain() {
    let errs = unknown_chain_errors("actor A \"a\"\nusecase X \"x\"\nperforms(A, X).when(\"a\")\n");
    assert_eq!(errs.len(), 1, "{errs:?}");
}

#[test]
fn known_chain_methods_are_accepted() {
    let errs = unknown_chain_errors(&format!(
        "{BELONGS_BASE}belongs(U, B).when(\"x\").by(\"y\")\n"
    ));
    assert!(errs.is_empty(), "{errs:?}");
}
