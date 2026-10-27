//! The RDRA definition sheet's 不整合 columns as one table of lint rules.
//!
//! Each row names the sheet column it comes from, the mechanism that enforces it, a hint, and a
//! witness: a minimal `.rdra` snippet that must trigger the row. `Resolver` rows document a
//! diagnostic the model builder already emits. `BucReach` and `TransitionUseCaseInBuc` rows are
//! enforced here, against one BUC closure computed per lint run.

use std::collections::HashSet;

use crate::lint::{node_id, node_kind_display, push_element_lint, LintIssue};
use crate::model::{node_ref_kind, ColumnType, NodeRef, SemanticModel};
use crate::resolver::reachable_from_bucs;

/// Which RDRA definition sheet a row belongs to. `RdraIsh` marks rows rdra-ish adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sheet {
    Buc,
    Information,
    State,
    Condition,
    RdraIsh,
}

impl Sheet {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Buc => "BUC",
            Self::Information => "情報",
            Self::State => "状態",
            Self::Condition => "条件",
            Self::RdraIsh => "rdra-ish",
        }
    }
}

/// The element kinds a `BucReach` row can own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Actor,
    ExtSystem,
    Entity,
    Condition,
    Variation,
    Timer,
}

impl ElementKind {
    fn nodes(self, model: &SemanticModel) -> Vec<NodeRef> {
        match self {
            Self::Actor => model.actors.keys().map(NodeRef::Actor).collect(),
            Self::ExtSystem => model.ext_systems.keys().map(NodeRef::ExtSystem).collect(),
            Self::Entity => model.entities.keys().map(NodeRef::Entity).collect(),
            Self::Condition => model.conditions.keys().map(NodeRef::Condition).collect(),
            Self::Variation => model.variations.keys().map(NodeRef::Variation).collect(),
            Self::Timer => model.timers.keys().map(NodeRef::Timer).collect(),
        }
    }

    fn owns(self, node: &NodeRef) -> bool {
        matches!(
            (self, node),
            (Self::Actor, NodeRef::Actor(_))
                | (Self::ExtSystem, NodeRef::ExtSystem(_))
                | (Self::Entity, NodeRef::Entity(_))
                | (Self::Condition, NodeRef::Condition(_))
                | (Self::Variation, NodeRef::Variation(_))
                | (Self::Timer, NodeRef::Timer(_))
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mechanism {
    /// Already an error at model build time. Names the `RdraError` variant the witness produces.
    Resolver(&'static str),
    /// Every node of `kind` must be in the BUC closure or point into it.
    BucReach { kind: ElementKind },
    /// Every use case a `transitions` names must be in the BUC closure.
    TransitionUseCaseInBuc,
}

/// A minimal model that must trigger its row. For `BucReach` and `TransitionUseCaseInBuc` rows
/// `subject` is the id of the element the finding must name. For `Resolver` rows it is empty.
#[derive(Debug, Clone, Copy)]
pub struct Witness {
    pub src: &'static str,
    pub subject: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct RdraRule {
    pub id: &'static str,
    pub sheet: Sheet,
    /// The sheet column, e.g. `アクター`. Empty for rdra-ish rows with no sheet column.
    pub column: &'static str,
    pub mechanism: Mechanism,
    pub hint: &'static str,
    pub witness: Witness,
}

const fn w(src: &'static str, subject: &'static str) -> Witness {
    Witness { src, subject }
}

macro_rules! with_buc {
    ($rest:literal) => {
        concat!(
            "buc B \"B\"\nusecase U \"U\"\ncontains(B, U)\nactor A \"A\"\nperforms(A, U)\n",
            $rest
        )
    };
}

pub const RDRA_RULES: &[RdraRule] = &[
    // BUC sheet, referenced but undefined.
    RdraRule {
        id: "rdra-buc-actor-undefined",
        sheet: Sheet::Buc,
        column: "アクター",
        mechanism: Mechanism::Resolver("UndefinedSymbol"),
        hint: "declare the actor or fix the name in performs(Actor, Buc)",
        witness: w(
            "buc B \"B\"\nperforms(Ghost, B)\n",
            "",
        ),
    },
    RdraRule {
        id: "rdra-buc-extsystem-undefined",
        sheet: Sheet::Buc,
        column: "外部システム",
        mechanism: Mechanism::Resolver("UndefinedSymbol"),
        hint: "declare the extsystem or fix the name in raises/triggers",
        witness: w(
            "usecase U \"U\"\nevent Ev \"Ev\"\nraises(Ghost, Ev)\n",
            "",
        ),
    },
    RdraRule {
        id: "rdra-buc-information-undefined",
        sheet: Sheet::Buc,
        column: "情報",
        mechanism: Mechanism::Resolver("UndefinedSymbol"),
        hint: "declare the entity or fix the name in reads/writes/creates/updates",
        witness: w(
            "api Ap \"Ap\"\nreads(Ap, Ghost)\n",
            "",
        ),
    },
    RdraRule {
        id: "rdra-buc-condition-undefined",
        sheet: Sheet::Buc,
        column: "条件",
        mechanism: Mechanism::Resolver("UndefinedSymbol"),
        hint: "declare the condition or fix the name in applies_to",
        witness: w(
            "usecase U \"U\"\napplies_to(Ghost, U)\n",
            "",
        ),
    },
    // 情報 sheet, referenced but undefined.
    RdraRule {
        id: "rdra-info-related-undefined",
        sheet: Sheet::Information,
        column: "関連情報",
        mechanism: Mechanism::Resolver("UndefinedSymbol"),
        hint: "declare the entity or fix the name in relate(A, B, ...)",
        witness: w(
            "entity E \"E\" {\n  id: Int @pk\n}\nrelate(E, Ghost, 1:N)\n",
            "",
        ),
    },
    RdraRule {
        id: "rdra-info-statemodel-undefined",
        sheet: Sheet::Information,
        column: "状態モデル",
        mechanism: Mechanism::Resolver("UnknownColumn"),
        hint: "name an existing Enum column of the entity in transitions(Entity.column, ...)",
        witness: w(
            "usecase U \"U\"\nentity E \"E\" {\n  id: Int @pk\n  status: Enum(a, b)\n}\ntransitions(E.ghost, U, a -> b)\n",
            "",
        ),
    },
    RdraRule {
        id: "rdra-info-variation-undefined",
        sheet: Sheet::Information,
        column: "バリエーション",
        mechanism: Mechanism::Resolver("UnknownColumnType"),
        hint: "declare the variation, import it, or use an inline Enum(...)",
        witness: w(
            "entity E \"E\" {\n  id: Int @pk\n  method: Ghost\n}\n",
            "",
        ),
    },
    // 状態 sheet, referenced but undefined.
    RdraRule {
        id: "rdra-state-uc-undefined",
        sheet: Sheet::State,
        column: "遷移UC",
        mechanism: Mechanism::Resolver("UndefinedSymbol"),
        hint: "declare the use case or fix the name in transitions(Entity.column, UseCase, ...)",
        witness: w(
            "entity E \"E\" {\n  id: Int @pk\n  status: Enum(a, b)\n}\ntransitions(E.status, Ghost, a -> b)\n",
            "",
        ),
    },
    RdraRule {
        id: "rdra-state-target-undefined",
        sheet: Sheet::State,
        column: "遷移先状態",
        mechanism: Mechanism::Resolver("InvalidEnumVariant"),
        hint: "use a variant declared in the column's Enum(...)",
        witness: w(
            "usecase U \"U\"\nentity E \"E\" {\n  id: Int @pk\n  status: Enum(a, b)\n}\ntransitions(E.status, U, a -> ghost)\n",
            "",
        ),
    },
    // 条件 sheet, referenced but undefined.
    RdraRule {
        id: "rdra-cond-variation-undefined",
        sheet: Sheet::Condition,
        column: "バリエーション",
        mechanism: Mechanism::Resolver("UndefinedSymbol"),
        hint: "declare the variation or fix the name in combines(Condition, Variation)",
        witness: w(
            "condition C \"C\"\ncombines(C, Ghost)\n",
            "",
        ),
    },
    RdraRule {
        id: "rdra-cond-statemodel-undefined",
        sheet: Sheet::Condition,
        column: "状態モデル",
        mechanism: Mechanism::Resolver("ConditionCombinesAttribute"),
        hint: "combine a column some `transitions` moves, or a variation",
        witness: w(
            "condition C \"C\"\nentity E \"E\" {\n  id: Int @pk\n  kind: Enum(a, b)\n}\ncombines(C, E.kind)\n",
            "",
        ),
    },
    // Defined but connected to no BUC.
    RdraRule {
        id: "rdra-actor-no-buc",
        sheet: Sheet::Buc,
        column: "アクター 未接続",
        mechanism: Mechanism::BucReach {
            kind: ElementKind::Actor,
        },
        hint: "connect it with performs(Actor, UseCase|Buc), or delete the actor",
        witness: w(with_buc!("actor Lonely \"Lonely\"\n"), "Lonely"),
    },
    RdraRule {
        id: "rdra-extsystem-no-buc",
        sheet: Sheet::Buc,
        column: "外部システム 未接続",
        mechanism: Mechanism::BucReach {
            kind: ElementKind::ExtSystem,
        },
        hint: "connect it with raises(ExtSystem, Event) or triggers(Event, ExtSystem) on an event a use case in a BUC raises or consumes, or delete the extsystem",
        witness: w(with_buc!("extsystem Lonely \"Lonely\"\n"), "Lonely"),
    },
    RdraRule {
        id: "rdra-entity-no-buc",
        sheet: Sheet::Buc,
        column: "情報 未接続",
        mechanism: Mechanism::BucReach {
            kind: ElementKind::Entity,
        },
        hint: "reach it from a use case: invokes(UseCase, Api) then reads/creates/updates(Api, Entity), or delete the entity",
        witness: w(
            with_buc!("entity Lonely \"Lonely\" {\n  id: Int @pk\n}\n"),
            "Lonely",
        ),
    },
    RdraRule {
        id: "rdra-condition-no-buc",
        sheet: Sheet::Buc,
        column: "条件 未接続",
        mechanism: Mechanism::BucReach {
            kind: ElementKind::Condition,
        },
        hint: "attach it with applies_to(Condition, UseCase) on a use case in a BUC, or delete the condition",
        witness: w(with_buc!("condition Lonely \"Lonely\"\n"), "Lonely"),
    },
    // rdra-ish additions.
    RdraRule {
        id: "rdra-variation-no-buc",
        sheet: Sheet::RdraIsh,
        column: "",
        mechanism: Mechanism::BucReach {
            kind: ElementKind::Variation,
        },
        hint: "type a column of an entity a BUC reaches with it, or combine it into a condition a BUC applies, or delete the variation",
        witness: w(
            with_buc!("variation Lonely \"Lonely\" {\n  v \"V\"\n}\n"),
            "Lonely",
        ),
    },
    RdraRule {
        id: "rdra-timer-no-buc",
        sheet: Sheet::RdraIsh,
        column: "",
        mechanism: Mechanism::BucReach {
            kind: ElementKind::Timer,
        },
        hint: "connect it with triggers(Timer, UseCase|Buc) to a use case in a BUC, or delete the timer",
        witness: w(with_buc!("timer Lonely \"Lonely\"\n"), "Lonely"),
    },
    RdraRule {
        id: "rdra-transition-uc-no-buc",
        sheet: Sheet::RdraIsh,
        column: "遷移UC は BUC シートで定義",
        mechanism: Mechanism::TransitionUseCaseInBuc,
        hint: "add contains(Buc, UseCase) for the use case that transitions the state, or move the transition to a use case a BUC contains",
        witness: w(
            with_buc!("usecase Stray \"Stray\"\napi Ap \"Ap\"\ninvokes(U, Ap)\nentity E \"E\" {\n  id: Int @pk\n  status: Enum(a, b)\n}\nupdates(Ap, E)\ntransitions(E.status, Stray, a -> b)\n"),
            "Stray",
        ),
    },
];

/// Forward closure of every BUC, computed once per lint run.
pub struct BucClosure {
    members: HashSet<NodeRef>,
    /// Nodes that are the `from` of a relation whose `to` is a member.
    pointing_in: HashSet<NodeRef>,
}

impl BucClosure {
    pub fn of(model: &SemanticModel) -> Self {
        let buc_ids: Vec<String> = model.bucs.values().map(|buc| buc.id.clone()).collect();
        let members = reachable_from_bucs(model, &buc_ids);
        let pointing_in = model
            .relations
            .iter()
            .filter(|rel| members.contains(&rel.to))
            .map(|rel| rel.from.clone())
            .collect();
        Self {
            members,
            pointing_in,
        }
    }

    pub fn contains(&self, node: &NodeRef) -> bool {
        self.members.contains(node)
    }

    /// In the closure, or the `from` of a relation into it. A variation is also anchored when it
    /// types a column of an anchored entity.
    pub fn anchors(&self, model: &SemanticModel, node: &NodeRef) -> bool {
        if self.contains(node) || self.pointing_in.contains(node) {
            return true;
        }
        let NodeRef::Variation(variation) = node else {
            return false;
        };
        model.entities.iter().any(|(entity, data)| {
            data.columns.iter().any(
                |column| matches!(column.col_type, ColumnType::Variation(v) if v == *variation),
            ) && self.anchors(model, &NodeRef::Entity(entity))
        })
    }
}

/// Findings for every `BucReach` and `TransitionUseCaseInBuc` row. `Resolver` rows produce none.
pub fn rdra_findings(model: &SemanticModel) -> Vec<LintIssue> {
    let closure = BucClosure::of(model);
    let mut findings = Vec::new();
    for rule in RDRA_RULES {
        match rule.mechanism {
            Mechanism::Resolver(_) => {}
            Mechanism::BucReach { kind } => {
                for node in kind.nodes(model) {
                    if closure.anchors(model, &node) {
                        continue;
                    }
                    let Some(id) = node_id(model, &node) else {
                        continue;
                    };
                    let subject_kind = node_kind_display(&node);
                    push_element_lint(
                        model,
                        &mut findings,
                        node_ref_kind(&node),
                        &subject_kind,
                        &id,
                        rule.id,
                        format!("{subject_kind} {id} is defined but connected to no BUC"),
                        rule.hint,
                    );
                }
            }
            Mechanism::TransitionUseCaseInBuc => {
                let mut seen = HashSet::new();
                for transition in &model.state_transitions {
                    let node = NodeRef::UseCase(transition.usecase);
                    if closure.contains(&node) || !seen.insert(transition.usecase) {
                        continue;
                    }
                    let Some(id) = node_id(model, &node) else {
                        continue;
                    };
                    push_element_lint(
                        model,
                        &mut findings,
                        node_ref_kind(&node),
                        "usecase",
                        &id,
                        rule.id,
                        format!("use case {id} transitions a state model but belongs to no BUC"),
                        rule.hint,
                    );
                }
            }
        }
    }
    findings
}

/// Kinds `orphan-node` must skip because a `BucReach` row already reports them.
pub fn kinds_owned_by_registry() -> impl Iterator<Item = ElementKind> {
    RDRA_RULES.iter().filter_map(|rule| match rule.mechanism {
        Mechanism::BucReach { kind } => Some(kind),
        _ => None,
    })
}

/// True when a `BucReach` row owns `node`'s kind.
pub fn owned_by_registry(node: &NodeRef) -> bool {
    kinds_owned_by_registry().any(|kind| kind.owns(node))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_model;
    use rdra_ish_syntax::parse;

    fn build(src: &str) -> (SemanticModel, Vec<crate::Diagnostic>) {
        let (ast, parse_errors) = parse(src);
        assert!(
            parse_errors.is_empty(),
            "parse errors: {parse_errors:?}\n{src}"
        );
        build_model(&ast)
    }

    fn variant_name(err: &crate::RdraError) -> String {
        format!("{err:?}")
            .chars()
            .take_while(|c| c.is_alphanumeric())
            .collect()
    }

    #[test]
    fn every_witness_triggers_its_row() {
        for rule in RDRA_RULES {
            let (model, diags) = build(rule.witness.src);
            match rule.mechanism {
                Mechanism::Resolver(name) => {
                    let names: Vec<String> = diags.iter().map(|d| variant_name(&d.error)).collect();
                    assert!(
                        names.iter().any(|n| n == name),
                        "{}: expected diagnostic {name}, got {names:?}",
                        rule.id
                    );
                }
                Mechanism::BucReach { .. } | Mechanism::TransitionUseCaseInBuc => {
                    let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
                    assert!(
                        errors.is_empty(),
                        "{}: witness has errors {errors:?}",
                        rule.id
                    );
                    let hits: Vec<_> = rdra_findings(&model)
                        .into_iter()
                        .filter(|issue| issue.rule == rule.id)
                        .collect();
                    assert_eq!(hits.len(), 1, "{}: {hits:?}", rule.id);
                    assert_eq!(hits[0].subject_id, rule.witness.subject, "{}", rule.id);
                }
            }
        }
    }

    #[test]
    fn witness_for_one_row_leaves_the_other_rows_quiet() {
        for rule in RDRA_RULES {
            if matches!(rule.mechanism, Mechanism::Resolver(_)) {
                continue;
            }
            let (model, _) = build(rule.witness.src);
            let others: Vec<_> = rdra_findings(&model)
                .into_iter()
                .filter(|issue| issue.rule != rule.id)
                .map(|issue| (issue.rule, issue.subject_id))
                .collect();
            assert!(others.is_empty(), "{}: {others:?}", rule.id);
        }
    }

    const CLEAN: &str = r#"
buc Checkout "Checkout"
usecase Place "Place"
actor Customer "Customer"
extsystem Gateway "Gateway"
extsystem Inbound "Inbound"
event EvPlaced "Placed"
event EvIn "In"
timer Nightly "Nightly"
variation Method "Method" {
  card "Card"
  cash "Cash"
}
condition Eligible "Eligible"
api PlaceApi "Place API"
entity Order "Order" {
  id: Int @pk
  status: Enum(pending, paid)
  method: Method
}
contains(Checkout, Place)
performs(Customer, Place)
invokes(Place, PlaceApi)
creates(PlaceApi, Order)
transitions(Order.status, Place, pending -> paid)
raises(Place, EvPlaced)
triggers(EvPlaced, Gateway)
raises(Inbound, EvIn)
triggers(EvIn, Place)
triggers(Nightly, Place)
applies_to(Eligible, Place)
combines(Eligible, Method)
"#;

    #[test]
    fn a_connected_model_produces_no_rdra_finding() {
        let (model, diags) = build(CLEAN);
        let errors: Vec<_> = diags.iter().filter(|d| !d.is_warning).collect();
        assert!(errors.is_empty(), "{errors:?}");
        let findings = rdra_findings(&model);
        assert!(findings.is_empty(), "{findings:?}");
        let all = crate::lint::lint_issues(&model, &[]);
        let rdra: Vec<_> = all.iter().filter(|i| i.rule.starts_with("rdra-")).collect();
        assert!(rdra.is_empty(), "{rdra:?}");
    }

    #[test]
    fn the_buc_closure_holds_inbound_events_and_their_external_systems() {
        let (model, _) = build(CLEAN);
        let closure = reachable_from_bucs(&model, &["Checkout".to_string()]);
        let ids: HashSet<String> = closure
            .iter()
            .filter_map(|node| node_id(&model, node))
            .collect();
        assert!(ids.contains("EvIn"), "inbound event missing: {ids:?}");
        assert!(
            ids.contains("Inbound"),
            "raising extsystem missing: {ids:?}"
        );
    }

    #[test]
    fn an_entity_linked_only_by_relate_is_not_in_a_buc() {
        let (model, _) = build(&format!(
            "{CLEAN}entity Account \"Account\" {{\n  id: Int @pk\n}}\nrelate(Order, Account, N:1)\n"
        ));
        let subjects: Vec<_> = rdra_findings(&model)
            .into_iter()
            .map(|issue| (issue.rule, issue.subject_id))
            .collect();
        assert_eq!(
            subjects,
            vec![("rdra-entity-no-buc", "Account".to_string())]
        );
    }

    #[test]
    fn an_owned_element_gets_one_finding_not_two() {
        let rule = RDRA_RULES
            .iter()
            .find(|rule| rule.id == "rdra-actor-no-buc")
            .unwrap();
        let (model, _) = build(rule.witness.src);
        let all = crate::lint::lint_issues(&model, &[]);
        let lonely: Vec<_> = all
            .iter()
            .filter(|i| i.subject_id == "Lonely" && i.rule != "naming-id")
            .map(|i| i.rule)
            .collect();
        assert_eq!(lonely, vec!["rdra-actor-no-buc"]);
    }

    #[test]
    fn rule_ids_are_unique_and_in_the_docs() {
        let reference = include_str!("../../../docs/language-reference.md");
        let interpretation = include_str!("../../../docs/rdra-ish-interpretation.md");
        let mut seen = HashSet::new();
        for rule in RDRA_RULES {
            assert!(seen.insert(rule.id), "duplicate id {}", rule.id);
            assert!(rule.id.starts_with("rdra-"));
            assert!(
                reference.contains(&format!("`{}`", rule.id)),
                "{} missing from docs/language-reference.md",
                rule.id
            );
            if rule.sheet != Sheet::RdraIsh {
                assert!(
                    interpretation.contains(&format!("`{}`", rule.id)),
                    "{} missing from docs/rdra-ish-interpretation.md",
                    rule.id
                );
            }
        }
        let sheet_rows = RDRA_RULES
            .iter()
            .filter(|rule| rule.sheet != Sheet::RdraIsh)
            .count();
        assert_eq!(sheet_rows, 15, "the RDRA sheet has 15 不整合 columns");
    }
}
