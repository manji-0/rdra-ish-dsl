//! Hover documentation for symbols and predicates.

use rdra_ish_core::model::NodeRef;
use rdra_ish_core::{node_ref_kind, LookupResult, SemanticModel};
use rdra_ish_syntax::ast::{Ast, Kind};
use tower_lsp::lsp_types::{
    Hover, HoverContents, MarkupContent, MarkupKind, SignatureHelp, SignatureInformation,
};

use crate::predicates::{
    format_predicate_signature, predicate_call_context, predicate_signature_parameters,
};
use crate::refs::{reference_at_offset, ReferenceAt};

pub fn hover_content(model: &SemanticModel, ast: &Ast, offset: usize) -> Option<Hover> {
    if let Some(markup) = symbol_hover(model, ast, offset) {
        return Some(Hover {
            contents: HoverContents::Markup(markup),
            range: None,
        });
    }

    if let Some((pred, _)) = predicate_call_context(&ast.source, offset) {
        if let Some(signature) = format_predicate_signature(&pred) {
            let markup = MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```rdra\n{signature}\n```"),
            };
            return Some(Hover {
                contents: HoverContents::Markup(markup),
                range: None,
            });
        }
    }

    None
}

pub fn signature_help(source: &str, offset: usize) -> Option<SignatureHelp> {
    let (pred_name, active_param) = predicate_call_context(source, offset)?;
    let label = format_predicate_signature(&pred_name)?;
    let parameters = predicate_signature_parameters(&pred_name).map(|params| {
        params
            .into_iter()
            .map(|documentation| tower_lsp::lsp_types::ParameterInformation {
                label: tower_lsp::lsp_types::ParameterLabel::Simple(documentation.clone()),
                documentation: None,
            })
            .collect()
    });

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: None,
            parameters,
            active_parameter: active_param.map(|index| index as u32),
        }],
        active_signature: Some(0),
        active_parameter: active_param.map(|index| index as u32),
    })
}

fn symbol_hover(model: &SemanticModel, ast: &Ast, offset: usize) -> Option<MarkupContent> {
    let reference = reference_at_offset(ast, offset)?;
    let (kind, id) = match reference {
        ReferenceAt::Declaration { kind, id } => (kind.to_string(), id.to_string()),
        ReferenceAt::Symbol(qref) => {
            let id = qref.parts.last()?.clone();
            let kind = if let Some(kind) = &qref.kind_qualifier {
                kind.name().to_string()
            } else {
                let node = match model.symbols.lookup(&id) {
                    LookupResult::Found(node) => node,
                    _ => return None,
                };
                node_ref_kind(node).to_string()
            };
            (kind, id)
        }
    };

    let label = instance_label(model, &kind, &id);
    let mut value = format!("**{kind}** `{id}`");
    if let Some(label) = label.filter(|text| !text.is_empty()) {
        value.push_str(&format!("\n\n{label}"));
    }

    Some(MarkupContent {
        kind: MarkupKind::Markdown,
        value,
    })
}

fn instance_label(model: &SemanticModel, kind: &str, id: &str) -> Option<String> {
    let kind = kind_from_name(kind)?;
    let node = model.symbols.lookup_qualified(&kind, id)?;
    Some(node_ref_label(model, node))
}

fn kind_from_name(name: &str) -> Option<Kind> {
    Some(match name {
        "actor" => Kind::Actor,
        "extsystem" => Kind::ExtSystem,
        "system" => Kind::System,
        "requirement" => Kind::Requirement,
        "adr" => Kind::Adr,
        "nfr" => Kind::Nfr,
        "quality" => Kind::Quality,
        "constraint" => Kind::Constraint,
        "business" => Kind::Business,
        "buc" => Kind::Buc,
        "flow" => Kind::Flow,
        "step" => Kind::Step,
        "usecase" => Kind::UseCase,
        "screen" => Kind::Screen,
        "field" => Kind::Field,
        "event" => Kind::Event,
        "entity" => Kind::Entity,
        "api" => Kind::Api,
        "dto" => Kind::Dto,
        "timer" => Kind::Timer,
        "medium" => Kind::Medium,
        "permission" => Kind::Permission,
        "variation" => Kind::Variation,
        "condition" => Kind::Condition,
        _ => return None,
    })
}

fn node_ref_label(model: &SemanticModel, node: &NodeRef) -> String {
    match node {
        NodeRef::Actor(k) => model.actors.get(*k).map(|v| v.label.clone()),
        NodeRef::ExtSystem(k) => model.ext_systems.get(*k).map(|v| v.label.clone()),
        NodeRef::System(k) => model.systems.get(*k).map(|v| v.label.clone()),
        NodeRef::Requirement(k) => model.requirements.get(*k).map(|v| v.label.clone()),
        NodeRef::Adr(k) => model.adrs.get(*k).map(|v| v.label.clone()),
        NodeRef::Nfr(k) => model.nfrs.get(*k).map(|v| v.label.clone()),
        NodeRef::Quality(k) => model.qualities.get(*k).map(|v| v.label.clone()),
        NodeRef::Constraint(k) => model.constraints.get(*k).map(|v| v.label.clone()),
        NodeRef::Business(k) => model.businesses.get(*k).map(|v| v.label.clone()),
        NodeRef::Buc(k) => model.bucs.get(*k).map(|v| v.label.clone()),
        NodeRef::Flow(k) => model.flows.get(*k).map(|v| v.label.clone()),
        NodeRef::Step(k) => model.steps.get(*k).map(|v| v.label.clone()),
        NodeRef::UseCase(k) => model.use_cases.get(*k).map(|v| v.label.clone()),
        NodeRef::Screen(k) => model.screens.get(*k).map(|v| v.label.clone()),
        NodeRef::Field(k) => model.fields.get(*k).map(|v| v.label.clone()),
        NodeRef::Event(k) => model.events.get(*k).map(|v| v.label.clone()),
        NodeRef::Entity(k) => model.entities.get(*k).map(|v| v.label.clone()),
        NodeRef::State(k) => model.states.get(*k).map(|v| v.label.clone()),
        NodeRef::Api(k) => model.apis.get(*k).map(|v| v.label.clone()),
        NodeRef::Dto(k) => model.dtos.get(*k).map(|v| v.label.clone()),
        NodeRef::Timer(k) => model.timers.get(*k).map(|v| v.label.clone()),
        NodeRef::Medium(k) => model.media.get(*k).map(|v| v.label.clone()),
        NodeRef::Permission(k) => model.permissions.get(*k).map(|v| v.label.clone()),
        NodeRef::Variation(k) => model.variations.get(*k).map(|v| v.label.clone()),
        NodeRef::Condition(k) => model.conditions.get(*k).map(|v| v.label.clone()),
    }
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use rdra_ish_core::build_model;
    use rdra_ish_syntax::parse;

    use super::*;

    #[test]
    fn hovers_symbol_declaration() {
        let src = r#"usecase Book "Book appointment"
"#;
        let (ast, errs) = parse(src);
        assert!(errs.is_empty());
        let (model, _) = build_model(&ast);
        let offset = src.find("Book").unwrap();
        let hover = hover_content(&model, &ast, offset).expect("hover");
        let markup = match hover.contents {
            HoverContents::Markup(markup) => markup,
            _ => panic!("expected markup"),
        };
        assert!(markup.value.contains("usecase"));
        assert!(markup.value.contains("Book appointment"));
    }

    #[test]
    fn hovers_condition_declaration_and_combines_signature() {
        let src = r#"condition CopayDue "A copay is due"
variation Method "Method" {
  cash
}
combines(CopayDue, Method)
"#;
        let (ast, errs) = parse(src);
        assert!(errs.is_empty());
        let (model, _) = build_model(&ast);
        let offset = src.find("CopayDue").unwrap();
        let hover = hover_content(&model, &ast, offset).expect("hover");
        let HoverContents::Markup(markup) = hover.contents else {
            panic!("expected markup");
        };
        assert!(markup.value.contains("condition"), "{}", markup.value);
        assert!(markup.value.contains("A copay is due"), "{}", markup.value);
        let offset = src.find("combines").unwrap();
        let hover = hover_content(&model, &ast, offset).expect("hover");
        let HoverContents::Markup(markup) = hover.contents else {
            panic!("expected markup");
        };
        assert!(
            markup.value.contains("combines(condition, variation)"),
            "{}",
            markup.value
        );
    }

    #[test]
    fn hovers_predicate_signature() {
        let program = r#"usecase Book "Book"
actor Staff "Staff"
performs(Staff, Book)
"#;
        let (ast, errs) = parse(program);
        assert!(errs.is_empty());
        let (model, _) = build_model(&ast);
        let offset = program.find("performs").unwrap();
        let hover = hover_content(&model, &ast, offset).expect("hover");
        let markup = match hover.contents {
            HoverContents::Markup(markup) => markup,
            _ => panic!("expected markup"),
        };
        assert!(markup.value.contains("performs(actor, usecase|buc|step)"));
    }

    #[test]
    fn signature_help_highlights_active_argument() {
        let src = r#"usecase Book "Book"
actor Staff "Staff"
performs(Staff, )
"#;
        let offset = src.find(", )").unwrap() + 2;
        let help = signature_help(src, offset).expect("signature help");
        assert_eq!(help.active_parameter, Some(1));
    }
}
