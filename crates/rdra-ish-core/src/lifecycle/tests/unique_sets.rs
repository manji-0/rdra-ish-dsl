//! `@unique` column sets and N:M junction pairs: `Unique_<E>_<cols>`, which two present rows
//! break by agreeing on every column of the set.

use super::*;
use crate::lifecycle::Trace;

/// The trace of the `Unique_*` violation named `name`, if `check` at two slots finds one.
fn violation(src: &str, name: &str) -> Option<Trace> {
    let d = derive_scoped(&model_from(src), Bounds::VERIFY, &Scope::ALL);
    d.diags.into_iter().find_map(|x| match x.diag {
        StateDiag::ConstraintViolated { name: n, trace, .. } if n == name => Some(trace),
        _ => None,
    })
}

/// Every finding of `check` at one slot that names a `Unique_*` rule, its violation or its
/// `Vacuous_` warning.
fn unique_findings_at_one_slot(src: &str) -> Vec<String> {
    derive(&model_from(src))
        .diags
        .iter()
        .map(|x| x.diag.name(&[]))
        .filter(|n| n.contains("Unique_"))
        .collect()
}

const SINGLE: &str = r#"
entity Booking "予約" { id: Int @pk  slot: Enum(am, pm) @unique @default(pm) }
usecase BookAm "午前予約"
creates(BookAm, Booking)
sets(BookAm, Booking.slot == am)
"#;

const COMPOSITE: &str = r#"
entity Room "部屋" { id: Int @pk  open: Bool @default(true) }
entity Booking "予約" {
  id: Int @pk
  slot: Enum(am, pm) @default(pm)
  @unique(room_id, slot)
}
relate(Booking, Room, N:1)
usecase Book "予約"
creates(Book, Booking)
sets(Book, Booking.slot == am)
"#;

const JUNCTION: &str = r#"
entity Student "学生" { id: Int @pk  active: Bool @default(true) }
entity Course "科目" { id: Int @pk  open: Bool @default(true) }
entity Enrollment "履修" { id: Int @pk  status: Enum(on, off) @default(on) }
relate(Student, Course, N:M).via(Enrollment)
usecase Enroll "履修登録"
creates(Enroll, Enrollment)
"#;

#[test]
fn two_rows_at_one_unique_enum_value_break_the_column() {
    let trace = violation(SINGLE, "Unique_Booking_slot").expect("violated at two slots");
    assert_eq!(trace.steps.len(), 2, "two BookAm runs: {trace:?}");
}

#[test]
fn two_bookings_of_one_room_and_slot_break_the_composite_set() {
    let trace = violation(COMPOSITE, "Unique_Booking_room_id_slot").expect("double booking");
    assert_eq!(trace.steps.len(), 2, "{trace:?}");
}

#[test]
fn a_junction_holding_one_pair_twice_breaks_its_key() {
    let trace =
        violation(JUNCTION, "Unique_Enrollment_course_id_student_id").expect("same pair twice");
    assert_eq!(trace.steps.len(), 2, "{trace:?}");
}

/// The guard a user adds to fix each: the parent the use case binds has no row at the value.
#[test]
fn a_guard_that_keeps_the_pair_free_holds() {
    let composite = format!(
        "{COMPOSITE}condition AmFree \"午前が空いている部屋\"\n\
         holds(AmFree, Room.open == true).none(Booking.slot == am)\n\
         applies_to(AmFree, Book)\n"
    );
    assert!(violation(&composite, "Unique_Booking_room_id_slot").is_none());
    let junction = format!(
        "{JUNCTION}condition NotYet \"未履修\"\n\
         holds(NotYet, Student.active == true, Course.open == true).none(Enrollment)\n\
         applies_to(NotYet, Enroll)\n"
    );
    assert!(violation(&junction, "Unique_Enrollment_course_id_student_id").is_none());
}

/// At one slot per entity there are no two rows: the rule holds and, like the 1:1
/// `Unique_*`, has no premise there, so it draws no `Vacuous_` warning either.
#[test]
fn at_one_slot_the_rule_holds_without_a_vacuity_warning() {
    for src in [SINGLE, COMPOSITE, JUNCTION] {
        assert_eq!(
            unique_findings_at_one_slot(src),
            Vec::<String>::new(),
            "{src}"
        );
    }
}

/// An unset optional column exempts the row, as SQL's NULLs are distinct: two rows with no
/// seat break nothing, two rows at seat 1 do.
#[test]
fn a_null_column_exempts_the_row() {
    let src = |sets: &str| {
        format!(
            "entity Ticket \"券\" {{ id: Int @pk  seat: Int @null @unique }}\n\
             usecase Issue \"発行\"\ncreates(Issue, Ticket)\n{sets}\n\
             forbidden(Ticket, seat > 3)\n"
        )
    };
    assert!(violation(&src(""), "Unique_Ticket_seat").is_none());
    assert!(violation(&src("sets(Issue, Ticket.seat == 1)"), "Unique_Ticket_seat").is_some());
}

/// A column whose value the lifecycle does not hold, such as a `String` (or a nullable one,
/// held as null or present), gives no rule: equal presence is no equal value.
#[test]
fn a_set_with_a_column_the_lifecycle_does_not_hold_is_not_checked() {
    let src = "entity Member \"会員\" { id: Int @pk  email: String @null @unique @default(uuid())  rank: Enum(a, b) @default(a)  @unique(email, rank) }\n\
               usecase Join \"入会\"\ncreates(Join, Member)\n";
    let ts = TransitionSystem::build(&model_from(src), Bounds::VERIFY, &Scope::ALL).ts;
    let names: Vec<&str> = ts.invariants().iter().map(|i| i.name.as_str()).collect();
    assert!(!names.iter().any(|n| n.starts_with("Unique_")), "{names:?}");
}

/// A declared set on an entity with a `@soft_delete` column is a partial index over live
/// rows, which the lifecycle does not tell apart, so it gives no rule.
#[test]
fn a_set_on_a_soft_deleted_entity_is_not_checked() {
    let src = "entity Booking \"予約\" { id: Int @pk  slot: Enum(am, pm) @unique  deleted_at: DateTime @null @soft_delete }\n\
               usecase Book \"予約\"\ncreates(Book, Booking)\n";
    let ts = TransitionSystem::build(&model_from(src), Bounds::VERIFY, &Scope::ALL).ts;
    let names: Vec<&str> = ts.invariants().iter().map(|i| i.name.as_str()).collect();
    assert!(!names.iter().any(|n| n.starts_with("Unique_")), "{names:?}");
}
