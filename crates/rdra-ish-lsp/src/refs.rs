//! Reference lookup at a cursor position.

use rdra_ish_core::{node_ref_kind, DeclSite, LookupResult, SemanticModel};
use rdra_ish_syntax::ast::{
    Ast, Expr, InstanceDecl, Item, Operand, PredicateArg, PredicateCall, QRef, Span,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolTarget {
    pub kind: String,
    pub id: String,
    /// The ids of the external systems bound with `from`. `Ext.Name` under one of them names
    /// another model's element: a rename or goto of `Name` never touches it, and a rename of
    /// the external system `Ext` rewrites the qualifier.
    pub bound_systems: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReferenceAt<'a> {
    Symbol(&'a QRef),
    Declaration { kind: &'static str, id: &'a str },
}

pub fn reference_at_offset(ast: &Ast, offset: usize) -> Option<ReferenceAt<'_>> {
    let mut best: Option<(usize, ReferenceAt<'_>)> = None;

    for item in &ast.items {
        visit_item(item, ast.source.as_str(), offset, &mut best);
    }

    best.map(|(_, reference)| reference)
}

/// `Ext.Name` where `Ext` is an external system bound with `from`.
pub fn is_bound_reference(model: &SemanticModel, qref: &QRef) -> bool {
    matches!(qref.parts.as_slice(), [alias, _] if rdra_ish_core::binding::bound_ext(model, alias).is_some())
}

fn bound_systems(model: &SemanticModel) -> Vec<String> {
    model
        .ext_systems
        .values()
        .filter(|x| x.binding.is_some())
        .map(|x| x.id.clone())
        .collect()
}

pub fn resolve_decl_site(model: &SemanticModel, reference: ReferenceAt<'_>) -> Option<DeclSite> {
    match reference {
        ReferenceAt::Declaration { kind, id } => model.decl_sites.get(kind, id).cloned(),
        ReferenceAt::Symbol(qref) if is_bound_reference(model, qref) => None,
        ReferenceAt::Symbol(qref) => {
            let id = qref.parts.last()?;
            let kind = if let Some(kind) = &qref.kind_qualifier {
                kind.name()
            } else {
                let node = match model.symbols.lookup(id) {
                    LookupResult::Found(node) => node,
                    _ => return None,
                };
                node_ref_kind(node)
            };
            model.decl_sites.get(kind, id).cloned()
        }
    }
}

pub fn symbol_target(model: &SemanticModel, reference: ReferenceAt<'_>) -> Option<SymbolTarget> {
    match reference {
        ReferenceAt::Declaration { kind, id } => Some(SymbolTarget {
            kind: kind.to_string(),
            id: id.to_string(),
            bound_systems: bound_systems(model),
        }),
        ReferenceAt::Symbol(qref) if is_bound_reference(model, qref) => None,
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
            Some(SymbolTarget {
                kind,
                id,
                bound_systems: bound_systems(model),
            })
        }
    }
}

pub fn find_symbol_references(ast: &Ast, target: &SymbolTarget) -> Vec<Span> {
    let mut spans = Vec::new();
    for item in &ast.items {
        if let Item::Predicate(pred) = item {
            collect_predicate_references(pred, target, &mut spans);
        }
    }
    spans
}

fn collect_predicate_references(
    pred: &PredicateCall,
    target: &SymbolTarget,
    spans: &mut Vec<Span>,
) {
    for arg in &pred.args {
        collect_references_in_arg(arg, target, spans);
    }
    for chain in &pred.chain {
        for arg in &chain.args {
            collect_references_in_arg(arg, target, spans);
        }
    }
}

fn collect_references_in_arg(arg: &PredicateArg, target: &SymbolTarget, spans: &mut Vec<Span>) {
    match arg {
        PredicateArg::Ref(qref) => {
            if qref_matches_target(qref, target) {
                spans.push(qref.span.clone());
            }
        }
        PredicateArg::Expr(expr) => collect_references_in_expr(expr, target, spans),
        PredicateArg::Transition { .. } | PredicateArg::Card(_) | PredicateArg::Lit(_) => {}
    }
}

fn collect_references_in_expr(expr: &Expr, target: &SymbolTarget, spans: &mut Vec<Span>) {
    match expr {
        Expr::Cmp(cmp) => {
            collect_references_in_operand(&cmp.lhs, target, spans);
            collect_references_in_operand(&cmp.rhs, target, spans);
        }
        Expr::Not(inner) => collect_references_in_expr(inner, target, spans),
        Expr::And(a, b) | Expr::Or(a, b) => {
            collect_references_in_expr(a, target, spans);
            collect_references_in_expr(b, target, spans);
        }
    }
}

fn collect_references_in_operand(operand: &Operand, target: &SymbolTarget, spans: &mut Vec<Span>) {
    if let Operand::QualifiedColumn(col) = operand {
        if qref_matches_target(&col.entity, target) {
            spans.push(col.entity.span.clone());
        }
    }
}

/// The segment of `qref` that names `target`: the qualifier of `Ext.Name` when the target is
/// the bound external system `Ext`, else the last segment.
pub fn qref_segment_for<'a>(qref: &'a QRef, target: &SymbolTarget) -> Option<&'a str> {
    if let [alias, _] = qref.parts.as_slice() {
        if target.bound_systems.contains(alias) {
            return (target.kind == "extsystem" && *alias == target.id).then_some(alias.as_str());
        }
    }
    let id = qref.parts.last()?;
    if id != &target.id {
        return None;
    }
    match &qref.kind_qualifier {
        Some(kind) => kind.name() == target.kind,
        None => true,
    }
    .then_some(id.as_str())
}

pub fn qref_matches_target(qref: &QRef, target: &SymbolTarget) -> bool {
    qref_segment_for(qref, target).is_some()
}

fn visit_item<'a>(
    item: &'a Item,
    source: &str,
    offset: usize,
    best: &mut Option<(usize, ReferenceAt<'a>)>,
) {
    match item {
        Item::Instance(inst) => {
            let id_span = instance_id_span(inst, source);
            consider(
                best,
                offset,
                id_span,
                ReferenceAt::Declaration {
                    kind: inst.kind.name(),
                    id: &inst.id,
                },
            );
        }
        Item::Predicate(pred) => visit_predicate(pred, offset, best),
        Item::Import(_) | Item::Module(_, _) | Item::Property(_) => {}
    }
}

fn visit_predicate<'a>(
    pred: &'a PredicateCall,
    offset: usize,
    best: &mut Option<(usize, ReferenceAt<'a>)>,
) {
    for arg in &pred.args {
        visit_predicate_arg(arg, offset, best);
    }
}

fn visit_predicate_arg<'a>(
    arg: &'a PredicateArg,
    offset: usize,
    best: &mut Option<(usize, ReferenceAt<'a>)>,
) {
    match arg {
        PredicateArg::Ref(qref) => {
            consider(best, offset, qref.span.clone(), ReferenceAt::Symbol(qref))
        }
        PredicateArg::Expr(expr) => visit_expr(expr, offset, best),
        PredicateArg::Transition { .. } | PredicateArg::Card(_) | PredicateArg::Lit(_) => {}
    }
}

fn visit_expr<'a>(expr: &'a Expr, offset: usize, best: &mut Option<(usize, ReferenceAt<'a>)>) {
    match expr {
        Expr::Cmp(cmp) => {
            visit_operand(&cmp.lhs, offset, best);
            visit_operand(&cmp.rhs, offset, best);
        }
        Expr::Not(inner) => visit_expr(inner, offset, best),
        Expr::And(a, b) | Expr::Or(a, b) => {
            visit_expr(a, offset, best);
            visit_expr(b, offset, best);
        }
    }
}

fn visit_operand<'a>(
    operand: &'a Operand,
    offset: usize,
    best: &mut Option<(usize, ReferenceAt<'a>)>,
) {
    if let Operand::QualifiedColumn(col) = operand {
        consider(
            best,
            offset,
            col.entity.span.clone(),
            ReferenceAt::Symbol(&col.entity),
        );
    }
}

pub fn instance_id_span(inst: &InstanceDecl, source: &str) -> Span {
    let slice = source.get(inst.span.clone()).unwrap_or_default();
    if let Some(rel) = slice.find(&inst.id) {
        let start = inst.span.start + rel;
        start..start + inst.id.len()
    } else {
        inst.span.clone()
    }
}

pub fn id_span_in_qref(qref: &QRef, source: &str, target: &SymbolTarget) -> Option<Span> {
    let segment = qref_segment_for(qref, target)?;
    let slice = source.get(qref.span.clone())?;
    let rel = if qref.parts.first().is_some_and(|first| first == segment) && qref.parts.len() > 1 {
        slice.find(segment)?
    } else {
        slice.rfind(segment)?
    };
    let start = qref.span.start + rel;
    Some(start..start + segment.len())
}

fn consider<'a>(
    best: &mut Option<(usize, ReferenceAt<'a>)>,
    offset: usize,
    span: Span,
    reference: ReferenceAt<'a>,
) {
    if !span.contains(&offset) {
        return;
    }
    let size = span.end.saturating_sub(span.start);
    match best {
        Some((best_size, _)) if *best_size <= size => {}
        _ => *best = Some((size, reference)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_syntax::parse;

    #[test]
    fn finds_qref_in_predicate() {
        let src = r#"usecase Book "Book"
actor Staff "Staff"
performs(Staff, Book)
"#;
        let (ast, errs) = parse(src);
        assert!(errs.is_empty());
        let offset = src.find(", Book)").unwrap() + 2;
        let reference = reference_at_offset(&ast, offset).expect("reference");
        assert!(matches!(reference, ReferenceAt::Symbol(q) if q.parts == ["Book"]));
    }

    #[test]
    fn finds_declaration_id() {
        let src = r#"usecase BookAppointment "Book"
"#;
        let (ast, errs) = parse(src);
        assert!(errs.is_empty());
        let offset = src.find("BookAppointment").unwrap();
        let reference = reference_at_offset(&ast, offset).expect("reference");
        assert!(matches!(
            reference,
            ReferenceAt::Declaration {
                kind: "usecase",
                id: "BookAppointment"
            }
        ));
    }

    #[test]
    fn finds_all_symbol_references() {
        let src = r#"usecase Book "Book"
actor Staff "Staff"
usecase Cancel "Cancel"
performs(Staff, Book)
invokes(Cancel, BookingApi)
api BookingApi "API"
invokes(Book, BookingApi)
"#;
        let (ast, errs) = parse(src);
        assert!(errs.is_empty());
        let target = SymbolTarget {
            bound_systems: Vec::new(),
            kind: "usecase".to_string(),
            id: "Book".to_string(),
        };
        let spans = find_symbol_references(&ast, &target);
        assert_eq!(spans.len(), 2);
    }
}
