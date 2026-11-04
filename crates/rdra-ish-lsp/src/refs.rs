//! Reference lookup at a cursor position.

use rdra_ish_core::{DeclSite, LookupResult, SemanticModel};

/// The symbol kind of a `relate` key's role: declared by `relate(..).role(x)`, referred to
/// by the `.role(x)` that picks the key of a join.
pub const ROLE_KIND: &str = "role";
use rdra_ish_syntax::ast::{
    Ast, AstTemporalFormula, ChainCall, Expr, InstanceDecl, Item, Operand, PredicateArg, QRef, Span,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolTarget {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReferenceAt<'a> {
    Symbol(&'a QRef),
    Declaration { kind: &'static str, id: &'a str },
}

pub fn reference_at_offset(ast: &Ast, offset: usize) -> Option<ReferenceAt<'_>> {
    let mut best: Option<(usize, ReferenceAt<'_>)> = None;

    for item in &ast.items {
        if let Item::Instance(inst) = item {
            consider(
                &mut best,
                offset,
                instance_id_span(inst, &ast.source),
                ReferenceAt::Declaration {
                    kind: inst.kind.name(),
                    id: &inst.id,
                },
            );
        }
    }
    for qref in ast_references(ast) {
        consider(
            &mut best,
            offset,
            qref.span.clone(),
            ReferenceAt::Symbol(qref),
        );
    }

    best.map(|(_, reference)| reference)
}

pub fn resolve_decl_site(model: &SemanticModel, reference: ReferenceAt<'_>) -> Option<DeclSite> {
    match reference {
        ReferenceAt::Declaration { kind, id } => model.decl_sites.get(kind, id).cloned(),
        ReferenceAt::Symbol(qref) => {
            let id = qref.parts.last()?;
            let kind = if let Some(kind) = &qref.kind_qualifier {
                kind.name()
            } else {
                let node = match model.symbols.lookup(id) {
                    LookupResult::Found(node) => node,
                    _ => return role_site(model, qref),
                };
                node.kind_tag()
            };
            model.decl_sites.get(kind, id).cloned()
        }
    }
}

/// The `relate` that declares the role `qref` names, when it names one and no symbol.
fn role_site(model: &SemanticModel, qref: &QRef) -> Option<DeclSite> {
    let id = role_name(model, qref)?;
    let (_, fk) = model
        .foreign_keys()
        .find(|(_, fk)| fk.role.as_deref() == Some(id))?;
    let site = fk.site.as_ref()?;
    Some(DeclSite {
        source_id: site.source_id,
        span: site.span.clone(),
    })
}

/// The role an unqualified one-part `qref` names: the `.role(x)` of some `relate`.
fn role_name<'a>(model: &SemanticModel, qref: &'a QRef) -> Option<&'a str> {
    let [id] = qref.parts.as_slice() else {
        return None;
    };
    (qref.kind_qualifier.is_none()
        && model
            .foreign_keys()
            .any(|(_, fk)| fk.role.as_deref() == Some(id.as_str())))
    .then_some(id.as_str())
}

pub fn symbol_target(model: &SemanticModel, reference: ReferenceAt<'_>) -> Option<SymbolTarget> {
    match reference {
        ReferenceAt::Declaration { kind, id } => Some(SymbolTarget {
            kind: kind.to_string(),
            id: id.to_string(),
        }),
        ReferenceAt::Symbol(qref) => {
            let id = qref.parts.last()?.clone();
            let kind = if let Some(kind) = &qref.kind_qualifier {
                kind.name().to_string()
            } else {
                let node = match model.symbols.lookup(&id) {
                    LookupResult::Found(node) => node,
                    _ => {
                        return role_name(model, qref).map(|id| SymbolTarget {
                            kind: ROLE_KIND.to_string(),
                            id: id.to_string(),
                        })
                    }
                };
                node.kind_tag().to_string()
            };
            Some(SymbolTarget { kind, id })
        }
    }
}

pub fn find_symbol_references(ast: &Ast, target: &SymbolTarget) -> Vec<Span> {
    references_to(ast, target)
        .into_iter()
        .map(|qref| qref.span.clone())
        .collect()
}

/// The references in `ast` to `target`. A role is referred to only where a role is written,
/// in `relate(..).role(x)` and in a `.role(x)` that picks a key, so a value or a medium
/// spelled the same is not one of them.
pub fn references_to<'a>(ast: &'a Ast, target: &SymbolTarget) -> Vec<&'a QRef> {
    if target.kind != ROLE_KIND {
        return ast_references(ast)
            .into_iter()
            .filter(|qref| qref_matches_target(qref, target))
            .collect();
    }
    let mut refs = Vec::new();
    for item in &ast.items {
        let Item::Predicate(pred) = item else {
            continue;
        };
        if pred.name == "belongs" {
            continue;
        }
        for call in pred.chain.iter().filter(|call| call.name == "role") {
            for arg in &call.args {
                if let PredicateArg::Ref(qref) = arg {
                    if qref.kind_qualifier.is_none() && qref.parts == [target.id.as_str()] {
                        refs.push(qref);
                    }
                }
            }
        }
    }
    refs
}

/// Every symbol reference written in `ast`: the arguments of each predicate and of each
/// of its chain methods, a property's formula and the arguments of its chain methods,
/// and the entity part of each `Entity.column` among them. Every LSP feature that looks
/// up, lists or renames a reference reads this one list, so a chain method added later
/// is covered without touching the features.
pub fn ast_references(ast: &Ast) -> Vec<&QRef> {
    let mut refs = Vec::new();
    for item in &ast.items {
        match item {
            Item::Predicate(pred) => {
                for arg in &pred.args {
                    collect_arg_references(arg, &mut refs);
                }
                collect_chain_references(&pred.chain, &mut refs);
            }
            Item::Property(prop) => {
                match &prop.formula {
                    AstTemporalFormula::Always(expr) | AstTemporalFormula::Eventually(expr) => {
                        collect_expr_references(expr, &mut refs)
                    }
                    AstTemporalFormula::LeadsTo {
                        antecedent,
                        consequent,
                    } => {
                        collect_expr_references(antecedent, &mut refs);
                        collect_expr_references(consequent, &mut refs);
                    }
                }
                collect_chain_references(&prop.chain, &mut refs);
            }
            Item::Instance(_) | Item::Import(_) | Item::Module(_, _) => {}
        }
    }
    refs
}

fn collect_chain_references<'a>(chain: &'a [ChainCall], refs: &mut Vec<&'a QRef>) {
    for call in chain {
        for arg in &call.args {
            collect_arg_references(arg, refs);
        }
    }
}

fn collect_arg_references<'a>(arg: &'a PredicateArg, refs: &mut Vec<&'a QRef>) {
    match arg {
        PredicateArg::Ref(qref) => refs.push(qref),
        PredicateArg::Expr(expr) => collect_expr_references(expr, refs),
        PredicateArg::Transition { .. } | PredicateArg::Card(_) | PredicateArg::Lit(_) => {}
    }
}

fn collect_expr_references<'a>(expr: &'a Expr, refs: &mut Vec<&'a QRef>) {
    match expr {
        Expr::Cmp(cmp) => {
            for operand in [&cmp.lhs, &cmp.rhs] {
                if let Operand::QualifiedColumn(col) = operand {
                    refs.push(&col.entity);
                }
            }
        }
        Expr::Not(inner) => collect_expr_references(inner, refs),
        Expr::And(a, b) | Expr::Or(a, b) => {
            collect_expr_references(a, refs);
            collect_expr_references(b, refs);
        }
    }
}

pub fn qref_matches_target(qref: &QRef, target: &SymbolTarget) -> bool {
    let Some(id) = qref.parts.last() else {
        return false;
    };
    if id != &target.id {
        return false;
    }
    match &qref.kind_qualifier {
        Some(kind) => kind.name() == target.kind,
        None => true,
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
    if !qref_matches_target(qref, target) {
        return None;
    }
    let id = qref.parts.last()?;
    let slice = source.get(qref.span.clone())?;
    let rel = slice.rfind(id)?;
    let start = qref.span.start + rel;
    Some(start..start + id.len())
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
            kind: "usecase".to_string(),
            id: "Book".to_string(),
        };
        let spans = find_symbol_references(&ast, &target);
        assert_eq!(spans.len(), 2);
    }

    #[test]
    fn finds_references_in_property_formula_and_fair_chain() {
        let src = r#"usecase Pay "Pay"
entity Order "o" {
  id: Int @pk
  status: String
}
property P leads_to(Order.status == open, Order.status == paid).fair(Pay)
"#;
        let (ast, errs) = parse(src);
        assert!(errs.is_empty(), "{errs:?}");

        let fair_at = src.find("fair(Pay)").unwrap() + "fair(".len();
        let reference = reference_at_offset(&ast, fair_at).expect("reference in .fair");
        assert!(matches!(reference, ReferenceAt::Symbol(q) if q.parts == ["Pay"]));

        let uc = SymbolTarget {
            kind: "usecase".to_string(),
            id: "Pay".to_string(),
        };
        assert_eq!(
            find_symbol_references(&ast, &uc),
            vec![fair_at..fair_at + 3]
        );
        let entity = SymbolTarget {
            kind: "entity".to_string(),
            id: "Order".to_string(),
        };
        assert_eq!(find_symbol_references(&ast, &entity).len(), 2);
    }
}
