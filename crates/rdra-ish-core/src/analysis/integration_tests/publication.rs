use crate::analysis::build_model;
use crate::diagnostics::{Diagnostic, RdraError};
use crate::model::{CallerKind, Exposed, PublishMode};
use rdra_ish_syntax::parse;

/// An owner model with one published upsert entity, one published append entity, and one
/// internal entity. `{extra}` adds the line under test.
fn owner(extra: &str) -> String {
    format!(
        r#"
system Auth "Auth"
actor User "User"
usecase Login "Log in"
usecase Revoke "Revoke"
performs(User, Login)
performs(User, Revoke)
entity Session "Session" {{ id: Int @pk  status: Enum(active, revoked) @default(active) }}
entity SessionView "Session (published)" {{ view_id: Int @pk  status: Enum(active, ended) }}
entity LoginLog "Login log" {{ id: Int @pk  result: Enum(ok, failed) }}
creates(Login, Session)
writes(Login, SessionView)
sets(Login, SessionView, status == active)
creates(Login, LoginLog)
sets(Login, LoginLog, result == ok)
transitions(Session.status, Revoke, active -> revoked)
writes(Revoke, SessionView)
sets(Revoke, SessionView, status == ended)
exposes(Auth, Login)
exposes(Auth, SessionView).upsert()
exposes(Auth, LoginLog).append()
{extra}
"#
    )
}

fn diagnostics(src: &str) -> Vec<Diagnostic> {
    let (ast, parse_errors) = parse(src);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    build_model(&ast).1
}

/// The names of the publication diagnostics, each with its severity.
fn publication_names(src: &str) -> Vec<(&'static str, bool)> {
    diagnostics(src)
        .iter()
        .filter_map(|d| {
            let name = match d.error {
                RdraError::PublishedEntityMutated { .. } => "PublishedEntityMutated",
                RdraError::PublishedEntityReadByOwner { .. } => "PublishedEntityReadByOwner",
                RdraError::PublishedEntityKeysInternal { .. } => "PublishedEntityKeysInternal",
                RdraError::PublishedNotInSourceTx { .. } => "PublishedNotInSourceTx",
                RdraError::InternalEntityExposed { .. } => "InternalEntityExposed",
                RdraError::PublishedEntityNeverWritten { .. } => "PublishedEntityNeverWritten",
                RdraError::SharedInternalEntity { .. } => "SharedInternalEntity",
                RdraError::ExposesChainInvalid { .. } => "ExposesChainInvalid",
                _ => return None,
            };
            Some((name, d.is_warning))
        })
        .collect()
}

#[test]
fn a_well_formed_owner_model_has_no_publication_diagnostic() {
    let diags = diagnostics(&owner(""));
    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
    assert!(errors.is_empty(), "{errors:?}");
    assert!(publication_names(&owner("")).is_empty());
}

#[test]
fn exposes_records_the_caller_and_the_publish_mode() {
    let (ast, _) = parse(&owner("exposes(Auth, Revoke).caller(system)"));
    let (model, _) = build_model(&ast);
    let items: Vec<&Exposed> = model
        .publication
        .exposures
        .iter()
        .map(|x| &x.item)
        .collect();
    let revoke = model
        .use_cases
        .iter()
        .find(|(_, u)| u.id == "Revoke")
        .unwrap()
        .0;
    let login = model
        .use_cases
        .iter()
        .find(|(_, u)| u.id == "Login")
        .unwrap()
        .0;
    let view = model
        .entities
        .iter()
        .find(|(_, e)| e.id == "SessionView")
        .unwrap()
        .0;
    let log = model
        .entities
        .iter()
        .find(|(_, e)| e.id == "LoginLog")
        .unwrap()
        .0;
    assert!(items.contains(&&Exposed::UseCase {
        usecase: login,
        caller: CallerKind::Actor
    }));
    assert!(items.contains(&&Exposed::UseCase {
        usecase: revoke,
        caller: CallerKind::System
    }));
    assert_eq!(model.publication.mode_of(view), Some(PublishMode::Upsert));
    assert_eq!(model.publication.mode_of(log), Some(PublishMode::Append));
    assert_eq!(
        model.publication.caller_of(revoke),
        Some(CallerKind::System)
    );
    assert_eq!(
        model
            .typed_predicates
            .iter()
            .filter(|p| matches!(p, crate::model::TypedPredicate::Exposes { .. }))
            .count(),
        4
    );
}

#[test]
fn an_append_entity_is_never_updated_and_no_published_entity_is_deleted_or_transitioned() {
    assert_eq!(
        publication_names(&owner("updates(Revoke, LoginLog)")),
        [("PublishedEntityMutated", false)]
    );
    assert_eq!(
        publication_names(&owner("deletes(Revoke, SessionView)")),
        [("PublishedEntityMutated", false)]
    );
    assert_eq!(
        publication_names(&owner(
            "transitions(SessionView.status, Revoke, active -> ended)"
        )),
        [("PublishedEntityMutated", false)]
    );
    assert!(publication_names(&owner("updates(Revoke, SessionView)")).is_empty());
}

#[test]
fn the_owner_never_reads_a_published_entity() {
    for read in [
        "reads(Revoke, SessionView)",
        "screen S \"S\"\ndisplays(Login, S)\nshows(S, SessionView)",
        "condition Active \"Active\"\nholds(Active, SessionView.status == active)\napplies_to(Active, Revoke)",
        "forbidden(SessionView, status == ended)",
        "invariant(Session, SessionView).when(Session.status == revoked).then(SessionView.status == ended)",
    ] {
        assert_eq!(
            publication_names(&owner(read)),
            [("PublishedEntityReadByOwner", false)],
            "{read}"
        );
    }
}

#[test]
fn a_published_entity_keys_only_to_a_published_entity() {
    assert_eq!(
        publication_names(&owner("relate(SessionView, Session, N:1)")),
        [("PublishedEntityKeysInternal", false)]
    );
    assert_eq!(
        publication_names(&owner("relate(Session, SessionView, N:1)")),
        [("PublishedEntityKeysInternal", false)]
    );
    assert!(publication_names(&owner("relate(LoginLog, SessionView, N:1)")).is_empty());
}

#[test]
fn a_published_write_outside_an_internal_change_is_reported() {
    let names = publication_names(&owner(
        "usecase Audit \"Audit\"\nperforms(User, Audit)\nwrites(Audit, SessionView)\nsets(Audit, SessionView, status == ended)",
    ));
    assert_eq!(names, [("PublishedNotInSourceTx", false)]);
    let two_groups = owner(
        "usecase Split \"Split\"\nperforms(User, Split)\nentity Audit \"Audit\" { id: Int @pk }\nupdates(Split, Session)\ncreates(Split, Audit)\ncreates(Split, LoginLog)",
    );
    let diags = diagnostics(&two_groups);
    let msg = diags
        .iter()
        .find(|d| matches!(d.error, RdraError::PublishedNotInSourceTx { .. }))
        .map(|d| d.error.to_string())
        .expect("reported");
    assert!(
        msg.contains("'Split' has 2 internal transaction(s)"),
        "{msg}"
    );
}

#[test]
fn an_internal_entity_is_never_exposed_and_an_unwritten_one_is_a_warning() {
    assert_eq!(
        publication_names(&owner("exposes(Auth, Session)")),
        [("InternalEntityExposed", false)]
    );
    assert_eq!(
        publication_names(&owner(
            "entity Stale \"Stale\" { id: Int @pk }\nexposes(Auth, Stale).upsert()"
        )),
        [("PublishedEntityNeverWritten", true)]
    );
}

#[test]
fn shares_is_a_warning_on_the_owner() {
    assert_eq!(
        publication_names(&owner("shares(Auth, Session)")),
        [("SharedInternalEntity", true)]
    );
}

#[test]
fn a_chain_on_the_wrong_exposed_kind_is_an_error() {
    for line in [
        "exposes(Auth, Revoke).append()",
        "exposes(Auth, LoginLog).caller(system)",
        "exposes(Auth, LoginLog).append().upsert()",
        "exposes(Auth, Revoke).caller(robot)",
        "event EvX \"X\"\nraises(Login, EvX)\noutbox(EvX)\nexposes(Auth, EvX).caller(any)",
    ] {
        let names = publication_names(&owner(line));
        assert!(
            names.contains(&("ExposesChainInvalid", false)),
            "{line}: {names:?}"
        );
    }
}

#[test]
fn a_system_callable_use_case_needs_no_actor() {
    let src = r#"
system Pay "Pay"
permission CanCapture "Can capture"
usecase Capture "Capture"
usecase Refund "Refund"
requires_permission(Capture, CanCapture)
requires_permission(Refund, CanCapture)
exposes(Pay, Capture).caller(system)
exposes(Pay, Refund)
"#;
    let (ast, _) = parse(src);
    let (model, _) = build_model(&ast);
    let no_actor: Vec<String> = crate::access::permission_diagnostics(&model)
        .iter()
        .filter_map(|d| match &d.error {
            RdraError::UseCasePermissionNoActor { usecase, .. } => Some(usecase.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(no_actor, ["Refund"]);
}
