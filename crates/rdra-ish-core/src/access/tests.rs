use super::*;
use crate::build_model;
use rdra_ish_syntax::parse;

fn model_from(src: &str) -> SemanticModel {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let (model, diags) = build_model(&ast);
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    model
}

#[test]
fn derive_permission_callables_groups_usecases_and_apis() {
    let model = model_from(
        r#"
usecase BookAppointment "Book Appointment"
usecase CancelAppointment "Cancel Appointment"
api BookingApi "Booking API"
api CancelApi "Cancel API"
permission ScheduleWrite "Schedule Write"
permission PatientRead "Patient Read"
requires_permission(BookAppointment, ScheduleWrite)
requires_permission(BookAppointment, PatientRead)
requires_permission(CancelAppointment, ScheduleWrite)
invokes(BookAppointment, BookingApi)
invokes(CancelAppointment, CancelApi)
requires_permission(BookingApi, PatientRead)
requires_permission(CancelApi, ScheduleWrite)
"#,
    );

    let callables = derive_permission_callables(&model);
    assert_eq!(callables.len(), 2);

    let schedule = callables
        .iter()
        .find(|entry| model.permissions[entry.permission].id == "ScheduleWrite")
        .expect("ScheduleWrite should be listed");
    assert_eq!(
        schedule
            .usecases
            .iter()
            .map(|key| model.use_cases[*key].id.as_str())
            .collect::<Vec<_>>(),
        vec!["BookAppointment", "CancelAppointment"]
    );
    assert_eq!(
        schedule
            .apis
            .iter()
            .map(|key| model.apis[*key].id.as_str())
            .collect::<Vec<_>>(),
        vec!["CancelApi"]
    );
    assert_eq!(
        schedule
            .api_paths
            .iter()
            .map(|path| format!(
                "{}->{}",
                model.use_cases[path.usecase].id, model.apis[path.api].id
            ))
            .collect::<Vec<_>>(),
        vec!["CancelAppointment->CancelApi"]
    );

    let patient = callables
        .iter()
        .find(|entry| model.permissions[entry.permission].id == "PatientRead")
        .expect("PatientRead should be listed");
    assert_eq!(
        patient
            .usecases
            .iter()
            .map(|key| model.use_cases[*key].id.as_str())
            .collect::<Vec<_>>(),
        vec!["BookAppointment"]
    );
    assert_eq!(
        patient
            .apis
            .iter()
            .map(|key| model.apis[*key].id.as_str())
            .collect::<Vec<_>>(),
        vec!["BookingApi"]
    );
    assert_eq!(
        patient
            .api_paths
            .iter()
            .map(|path| format!(
                "{}->{}",
                model.use_cases[path.usecase].id, model.apis[path.api].id
            ))
            .collect::<Vec<_>>(),
        vec!["BookAppointment->BookingApi"]
    );
}

#[test]
fn derive_permission_callables_includes_permissions_without_callables() {
    let model = model_from(
        r#"
permission UnusedPermission "Unused Permission"
usecase BookAppointment "Book Appointment"
requires_permission(BookAppointment, ScheduleWrite)
permission ScheduleWrite "Schedule Write"
"#,
    );

    let callables = derive_permission_callables(&model);
    assert_eq!(callables.len(), 2);

    let unused = callables
        .iter()
        .find(|entry| model.permissions[entry.permission].id == "UnusedPermission")
        .expect("unused permission should be listed");
    assert!(unused.usecases.is_empty());
    assert!(unused.apis.is_empty());
    assert!(unused.api_paths.is_empty());
}

#[test]
fn derive_actor_permission_audit_marks_missing_ok_and_excess() {
    let model = model_from(
        r#"
actor Staff "Staff"
actor BillingBot "Billing Bot"
usecase BookAppointment "Book Appointment"
usecase GenerateClaim "Generate Claim"
api BookingApi "Booking API"
permission ScheduleWrite "Schedule Write"
permission BillingClaimWrite "Billing Claim Write"
permission LegacyAdmin "Legacy Admin"
performs(Staff, BookAppointment)
performs(BillingBot, GenerateClaim)
has_permission(BillingBot, BillingClaimWrite)
has_permission(Staff, LegacyAdmin)
requires_permission(BookAppointment, ScheduleWrite)
invokes(BookAppointment, BookingApi)
requires_permission(BookingApi, ScheduleWrite)
requires_permission(GenerateClaim, BillingClaimWrite)
"#,
    );

    let audit = derive_actor_permission_audit(&model);

    let staff_schedule = audit
        .iter()
        .find(|entry| {
            model.actors[entry.actor].id == "Staff"
                && model.permissions[entry.permission].id == "ScheduleWrite"
        })
        .expect("Staff/ScheduleWrite should be inferred");
    assert_eq!(staff_schedule.status, ActorPermissionAuditStatus::Missing);
    assert!(!staff_schedule.assigned);
    assert!(staff_schedule.required);
    assert_eq!(staff_schedule.sources.len(), 2);

    let staff_legacy = audit
        .iter()
        .find(|entry| {
            model.actors[entry.actor].id == "Staff"
                && model.permissions[entry.permission].id == "LegacyAdmin"
        })
        .expect("Staff/LegacyAdmin should be listed");
    assert_eq!(staff_legacy.status, ActorPermissionAuditStatus::Excess);
    assert!(staff_legacy.assigned);
    assert!(!staff_legacy.required);

    let bot_billing = audit
        .iter()
        .find(|entry| {
            model.actors[entry.actor].id == "BillingBot"
                && model.permissions[entry.permission].id == "BillingClaimWrite"
        })
        .expect("BillingBot/BillingClaimWrite should be listed");
    assert_eq!(bot_billing.status, ActorPermissionAuditStatus::Ok);
}

#[test]
fn a_buc_actor_does_not_perform_a_use_case_an_event_or_timer_starts() {
    let model = model_from(
        r#"
actor Clerk "Clerk"
actor Patient "Patient"
buc BucPay "Pay"
usecase SettleAtWindow "Settle at window"
usecase SettleAtKiosk "Settle at kiosk"
usecase Remind "Remind"
event Settled "Settled"
timer Nightly "Nightly"
performs(Clerk, BucPay)
performs(Patient, Remind)
contains(BucPay, SettleAtWindow)
contains(BucPay, SettleAtKiosk)
contains(BucPay, Remind)
triggers(Settled, SettleAtKiosk)
triggers(Nightly, Remind)
"#,
    );
    let actors = |id: &str| -> Vec<String> {
        let (key, _) = model.use_cases.iter().find(|(_, u)| u.id == id).unwrap();
        actors_for_usecase(&model, key)
            .into_iter()
            .map(|a| model.actors[a].id.clone())
            .collect()
    };

    assert_eq!(actors("SettleAtWindow"), ["Clerk"]);
    assert!(actors("SettleAtKiosk").is_empty());
    assert_eq!(actors("Remind"), ["Patient"]);
}

#[test]
fn permission_diagnostics_warns_when_usecase_actor_lacks_required_permission() {
    let model = model_from(
        r#"
actor Staff "Staff"
usecase BookAppointment "Book Appointment"
permission ScheduleWrite "Schedule Write"
performs(Staff, BookAppointment)
requires_permission(BookAppointment, ScheduleWrite)
"#,
    );

    let diags = permission_diagnostics(&model);
    assert!(diags.iter().any(|diag| matches!(
        &diag.error,
        RdraError::ActorPermissionMissing {
            actor,
            permission,
            required_by,
            ..
        } if actor == "Staff"
            && permission == "ScheduleWrite"
            && required_by.contains("usecase 'BookAppointment'")
    )));
}

#[test]
fn permission_diagnostics_warns_when_required_usecase_has_no_actor_path() {
    let model = model_from(
        r#"
usecase GenerateClaim "Generate Claim"
permission BillingClaimWrite "Billing Claim Write"
requires_permission(GenerateClaim, BillingClaimWrite)
"#,
    );

    let diags = permission_diagnostics(&model);
    assert!(diags.iter().any(|diag| matches!(
        &diag.error,
        RdraError::UseCasePermissionNoActor {
            usecase,
            permission,
        } if usecase == "GenerateClaim" && permission == "BillingClaimWrite"
    )));
}

#[test]
fn permission_diagnostics_accepts_buc_actor_permission_for_contained_usecase() {
    let model = model_from(
        r#"
actor Staff "Staff"
buc BucScheduling "Scheduling"
usecase BookAppointment "Book Appointment"
permission ScheduleWrite "Schedule Write"
performs(Staff, BucScheduling)
has_permission(Staff, ScheduleWrite)
contains(BucScheduling, BookAppointment)
requires_permission(BookAppointment, ScheduleWrite)
"#,
    );

    assert!(permission_diagnostics(&model).is_empty());
}

#[test]
fn permission_diagnostics_unions_buc_and_direct_performers() {
    let model = model_from(
        r#"
actor Customer "Customer"
actor Admin "Admin"
buc BucOrder "Order"
usecase RefundOrder "Refund"
permission AdminOnly "Admin Only"
performs(Customer, BucOrder)
performs(Admin, RefundOrder)
has_permission(Admin, AdminOnly)
contains(BucOrder, RefundOrder)
requires_permission(RefundOrder, AdminOnly)
"#,
    );

    let diags = permission_diagnostics(&model);
    assert!(
        diags.iter().any(|diag| matches!(
            &diag.error,
            RdraError::ActorPermissionMissing {
                actor,
                permission,
                ..
            } if actor == "Customer" && permission == "AdminOnly"
        )),
        "BUC performer must remain visible when a direct performer is also declared: {:?}",
        diags
            .iter()
            .map(|d| d.error.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn permission_diagnostics_warns_for_each_actor_missing_on_same_path() {
    let model = model_from(
        r#"
actor Staff "Staff"
actor Assistant "Assistant"
buc BucScheduling "Scheduling"
usecase BookAppointment "Book Appointment"
permission ScheduleWrite "Schedule Write"
performs(Staff, BucScheduling)
performs(Assistant, BucScheduling)
has_permission(Staff, ScheduleWrite)
contains(BucScheduling, BookAppointment)
requires_permission(BookAppointment, ScheduleWrite)
"#,
    );

    let diags = permission_diagnostics(&model);
    assert_eq!(diags.len(), 1);
    assert!(matches!(
        &diags[0].error,
        RdraError::ActorPermissionMissing {
            actor,
            permission,
            required_by,
            ..
        } if actor == "Assistant"
            && permission == "ScheduleWrite"
            && required_by.contains("usecase 'BookAppointment'")
    ));
}

#[test]
fn permission_diagnostics_checks_api_requirement_per_invoking_usecase() {
    let model = model_from(
        r#"
actor Staff "Staff"
actor Patient "Patient"
usecase BookAppointment "Book Appointment"
usecase CancelAppointment "Cancel Appointment"
api BookingApi "Booking API"
permission ScheduleWrite "Schedule Write"
performs(Staff, BookAppointment)
performs(Patient, CancelAppointment)
has_permission(Staff, ScheduleWrite)
invokes(BookAppointment, BookingApi)
invokes(CancelAppointment, BookingApi)
requires_permission(BookingApi, ScheduleWrite)
"#,
    );

    let diags = permission_diagnostics(&model);
    assert_eq!(diags.len(), 1);
    assert!(matches!(
        &diags[0].error,
        RdraError::ActorPermissionMissing {
            actor,
            permission,
            required_by,
            ..
        } if actor == "Patient"
            && permission == "ScheduleWrite"
            && required_by.contains("api 'BookingApi' via usecase 'CancelAppointment'")
    ));
}

#[test]
fn permission_diagnostics_warns_when_api_invocation_has_no_actor_path() {
    let model = model_from(
        r#"
usecase GenerateClaim "Generate Claim"
api ClaimsApi "Claims API"
permission BillingClaimWrite "Billing Claim Write"
invokes(GenerateClaim, ClaimsApi)
requires_permission(ClaimsApi, BillingClaimWrite)
"#,
    );

    let diags = permission_diagnostics(&model);
    assert!(diags.iter().any(|diag| matches!(
        &diag.error,
        RdraError::ApiPermissionNoActor {
            api,
            permission,
            usecase,
        } if api == "ClaimsApi"
            && permission == "BillingClaimWrite"
            && usecase == "GenerateClaim"
    )));
}

/// A timer or an event starts the use case and no actor performs it: the system runs it,
/// so neither its permissions nor its APIs' need an actor.
#[test]
fn a_use_case_a_timer_or_event_starts_with_no_performer_needs_no_actor_permission() {
    let model = model_from(
        r#"
usecase ExpireTerm "Expire term"
usecase ReceiveNotice "Receive notice"
api DocketApi "Docket API"
timer Deadline "Deadline"
event NoticeArrived "Notice arrived"
permission DocketManage "Docket Manage"
invokes(ExpireTerm, DocketApi)
invokes(ReceiveNotice, DocketApi)
requires_permission(DocketApi, DocketManage)
requires_permission(ReceiveNotice, DocketManage)
triggers(Deadline, ExpireTerm)
triggers(NoticeArrived, ReceiveNotice)
"#,
    );

    let diags = permission_diagnostics(&model);
    assert!(diags.is_empty(), "{diags:?}");
}

/// A started use case that an actor performs is that actor's work: the actor's permissions
/// are checked as for any other use case.
#[test]
fn a_started_use_case_with_a_performer_checks_the_performer() {
    let model = model_from(
        r#"
actor Clerk "Clerk"
usecase ExpireTerm "Expire term"
api DocketApi "Docket API"
timer Deadline "Deadline"
permission DocketManage "Docket Manage"
performs(Clerk, ExpireTerm)
invokes(ExpireTerm, DocketApi)
requires_permission(DocketApi, DocketManage)
triggers(Deadline, ExpireTerm)
"#,
    );

    let diags = permission_diagnostics(&model);
    assert!(
        diags.iter().any(|diag| matches!(
            &diag.error,
            RdraError::ActorPermissionMissing { actor, .. } if actor == "Clerk"
        )),
        "{diags:?}"
    );
}

/// `performs(Actor, Buc)` makes the actor perform every use case of the BUC; when that is
/// how a permission comes to be missing, the message names the BUC and the narrower forms.
#[test]
fn a_permission_missing_through_a_buc_names_the_buc() {
    let model = model_from(
        r#"
actor Clerk "Clerk"
buc BucFees "Fees"
usecase PayFees "Pay fees"
usecase RecordGrant "Record grant"
permission FeePay "Fee Pay"
permission DocketManage "Docket Manage"
contains(BucFees, PayFees)
contains(BucFees, RecordGrant)
performs(Clerk, BucFees)
performs(Clerk, PayFees)
has_permission(Clerk, FeePay)
requires_permission(PayFees, FeePay)
requires_permission(RecordGrant, DocketManage)
"#,
    );

    let messages: Vec<String> = permission_diagnostics(&model)
        .into_iter()
        .filter(|diag| matches!(diag.error, RdraError::ActorPermissionMissing { .. }))
        .map(|diag| diag.error.to_string())
        .collect();
    assert_eq!(
        messages,
        ["actor 'Clerk' is missing permission 'DocketManage' required by usecase 'RecordGrant'\n  hint: add `has_permission(Clerk, DocketManage)`, narrow the actor's `performs` path, or relax the UC/API requirement if it is too strict; `performs(Clerk, BucFees)` makes Clerk perform every use case of BucFees: to say Clerk only takes part in BucFees, write `performs(Clerk, UseCase)` for the use cases it runs or `performs(Clerk, Step)` for its steps instead"]
    );
}
