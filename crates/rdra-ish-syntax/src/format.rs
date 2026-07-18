//! Canonical source formatter for the RDRA-ish DSL.

use chumsky::error::Simple;

use crate::ast::*;
use crate::parser::parse;
use crate::token::Token;

#[derive(Debug)]
pub struct FormatError {
    pub parse_errors: Vec<Simple<Token>>,
}

/// Parse and pretty-print `src` into canonical RDRA-ish source.
pub fn format_source(src: &str) -> Result<String, FormatError> {
    let (ast, errors) = parse(src);
    if !errors.is_empty() {
        return Err(FormatError {
            parse_errors: errors,
        });
    }
    Ok(format_ast(&ast))
}

/// Pretty-print an already-parsed AST.
pub fn format_ast(ast: &Ast) -> String {
    let mut out = String::new();
    for (index, item) in ast.items.iter().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(&format_item(item));
        out.push('\n');
    }
    out
}

fn format_item(item: &Item) -> String {
    match item {
        Item::Module(name, _) => format!("module {}", format_dotted_name(name)),
        Item::Import(import) => format_import(import),
        Item::Instance(instance) => format_instance(instance),
        Item::Predicate(predicate) => format_predicate(predicate),
        Item::Property(prop) => format_property(prop),
    }
}

fn format_property(prop: &PropertyDecl) -> String {
    let formula = match &prop.formula {
        AstTemporalFormula::Always(expr) => format!("always({})", format_expr(expr)),
        AstTemporalFormula::Eventually(expr) => format!("eventually({})", format_expr(expr)),
        AstTemporalFormula::LeadsTo {
            antecedent,
            consequent,
        } => format!(
            "leads_to({}, {})",
            format_expr(antecedent),
            format_expr(consequent)
        ),
    };
    format!(
        "property {} {}\n  {}",
        prop.id,
        quote_string(&prop.label),
        formula
    )
}

fn format_dotted_name(name: &DottedName) -> String {
    name.0.join(".")
}

fn format_import(import: &ImportDecl) -> String {
    let path = format_dotted_name(&import.path);
    match &import.kind {
        ImportKind::All => format!("import {}", path),
        ImportKind::Alias(alias) => format!("import {} as {}", path, alias),
        ImportKind::Select(items) => {
            let items = items
                .iter()
                .map(|item| match &item.alias {
                    Some(alias) => format!("{} as {}", item.name, alias),
                    None => item.name.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("import {}.{{{}}}", path, items)
        }
    }
}

fn format_instance(instance: &InstanceDecl) -> String {
    let mut lines = vec![format!(
        "{} {} {}",
        instance.kind.name(),
        instance.id,
        quote_string(&instance.label)
    )];

    if let Some(description) = &instance.description {
        lines.push(format!("  description {}", quote_string(description)));
    }

    lines.extend(format_requirement_metadata(instance));
    lines.extend(format_adr_metadata(instance));
    lines.extend(format_api_metadata(instance));
    lines.extend(format_nfr_metadata(instance));
    lines.extend(format_field_metadata(instance));
    lines.extend(format_usecase_metadata(instance));

    if !instance.columns.is_empty() {
        lines[0].push_str(" {");
        for column in &instance.columns {
            lines.push(format!("  {}", format_column(column)));
        }
        lines.push("}".to_string());
    }

    lines.join("\n")
}

fn format_requirement_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.requirement.priority {
        lines.push(format!("  priority {}", quote_string(value)));
    }
    for value in &instance.requirement.sources {
        lines.push(format!("  source {}", quote_string(value)));
    }
    for value in &instance.requirement.stakeholders {
        lines.push(format!("  stakeholder {}", quote_string(value)));
    }
    if let Some(value) = &instance.requirement.owner {
        lines.push(format!("  owner {}", quote_string(value)));
    }
    for value in &instance.requirement.acceptance_criteria {
        lines.push(format!("  acceptance criteria {}", quote_string(value)));
    }
    if let Some(value) = &instance.requirement.status {
        lines.push(format!("  status {}", quote_string(value)));
    }
    if let Some(value) = &instance.requirement.risk {
        lines.push(format!("  risk {}", quote_string(value)));
    }
    if let Some(value) = &instance.requirement.rationale {
        lines.push(format!("  rationale {}", quote_string(value)));
    }
    lines
}

fn format_adr_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.adr.status {
        lines.push(format!("  adr_status {}", format_metadata_value(value)));
    }
    for value in &instance.adr.context {
        lines.push(format!("  context {}", quote_string(value)));
    }
    if let Some(value) = &instance.adr.decision {
        lines.push(format!("  decision {}", quote_string(value)));
    }
    for value in &instance.adr.consequences {
        lines.push(format!("  consequence {}", quote_string(value)));
    }
    for value in &instance.adr.accepted_options {
        lines.push(format!("  accepted {}", quote_string(value)));
    }
    for value in &instance.adr.rejected_options {
        lines.push(format!("  rejected {}", quote_string(value)));
    }
    for value in &instance.adr.reasons {
        lines.push(format!("  reason {}", quote_string(value)));
    }
    lines
}

fn format_api_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.api.method {
        lines.push(format!("  method {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.api.path {
        lines.push(format!("  path {}", quote_string(value)));
    }
    if let Some(value) = &instance.api.idempotency {
        lines.push(format!("  idempotency {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.api.mode {
        lines.push(format!("  mode {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.api.auth_scheme {
        lines.push(format!("  auth {}", format_metadata_value(value)));
    }
    lines
}

fn format_nfr_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.nfr.metric {
        lines.push(format!("  metric {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.target {
        lines.push(format!("  target {}", quote_string(value)));
    }
    if let Some(value) = &instance.nfr.window {
        lines.push(format!("  window {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.slo {
        lines.push(format!("  slo {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.availability {
        lines.push(format!("  availability {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.resilience {
        lines.push(format!("  resilience {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.audit {
        lines.push(format!("  audit {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.logging {
        lines.push(format!("  logging {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.retention {
        lines.push(format!("  retention {}", format_metadata_value(value)));
    }
    if let Some(value) = &instance.nfr.privacy {
        lines.push(format!("  privacy {}", format_metadata_value(value)));
    }
    lines
}

fn format_field_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.field.access {
        lines.push(format!("  access {}", format_metadata_value(value)));
    }
    if let Some(value) = instance.field.required {
        lines.push(format!("  required {}", bool_cell(value)));
    }
    if let Some(value) = &instance.field.source {
        lines.push(format!("  source {}", format_metadata_value(value)));
    }
    lines
}

fn format_usecase_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    for value in &instance.usecase.preconditions {
        lines.push(format!("  precondition {}", quote_string(value)));
    }
    for value in &instance.usecase.guards {
        lines.push(format!("  guard {}", quote_string(value)));
    }
    for value in &instance.usecase.postconditions {
        lines.push(format!("  postcondition {}", quote_string(value)));
    }
    for value in &instance.usecase.alternatives {
        lines.push(format!("  alternative {}", quote_string(value)));
    }
    for value in &instance.usecase.errors {
        lines.push(format!("  error {}", quote_string(value)));
    }
    lines
}

fn format_column(column: &Column) -> String {
    let mut out = format!("{}: {}", column.name, format_col_type(&column.col_type));
    for annotation in &column.annotations {
        out.push(' ');
        out.push_str(&format_annotation(annotation));
    }
    out
}

fn format_col_type(col_type: &ColType) -> String {
    match col_type {
        ColType::Int => "Int".to_string(),
        ColType::String => "String".to_string(),
        ColType::Money => "Money".to_string(),
        ColType::DateTime => "DateTime".to_string(),
        ColType::Date => "Date".to_string(),
        ColType::Bool => "Bool".to_string(),
        ColType::Decimal => "Decimal".to_string(),
        ColType::Enum(values) => format!("Enum({})", values.join(", ")),
    }
}

fn format_annotation(annotation: &Annotation) -> String {
    match annotation {
        Annotation::Pk => "@pk".to_string(),
        Annotation::PkComposite(columns) => format!("@pk({})", columns.join(", ")),
        Annotation::Unique => "@unique".to_string(),
        Annotation::UniqueComposite(columns) => format!("@unique({})", columns.join(", ")),
        Annotation::Index => "@index".to_string(),
        Annotation::IndexComposite(columns) => format!("@index({})", columns.join(", ")),
        Annotation::Check(expr) => format!("@check({})", quote_string(expr)),
        Annotation::Null => "@null".to_string(),
        Annotation::Default(value) => format!("@default({})", format_metadata_value(value)),
        Annotation::Label(value) => format!("@label({})", quote_string(value)),
        Annotation::SoftDelete => "@soft_delete".to_string(),
        Annotation::History => "@history".to_string(),
        Annotation::Tenant => "@tenant".to_string(),
        Annotation::Derived(expr) => format!("@derived({})", quote_string(expr)),
    }
}

fn format_predicate(predicate: &PredicateCall) -> String {
    let args = predicate
        .args
        .iter()
        .map(format_predicate_arg)
        .collect::<Vec<_>>()
        .join(", ");
    let mut out = format!("{}({})", predicate.name, args);
    for chain in &predicate.chain {
        out.push_str(&format_chain_call(chain));
    }
    out
}

fn format_chain_call(chain: &ChainCall) -> String {
    let args = chain
        .args
        .iter()
        .map(format_predicate_arg)
        .collect::<Vec<_>>()
        .join(", ");
    format!(".{}({})", chain.name, args)
}

fn format_predicate_arg(arg: &PredicateArg) -> String {
    match arg {
        PredicateArg::Ref(qref) => format_qref(qref),
        PredicateArg::Lit(value) => quote_string(value),
        PredicateArg::Expr(expr) => format_expr(expr),
        PredicateArg::Transition { from, to } => format!("{from} -> {to}"),
        PredicateArg::Card(card) => card.clone(),
    }
}

fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Cmp(cmp) => format!(
            "{} {} {}",
            format_operand(&cmp.lhs),
            format_cmp_op(&cmp.op),
            format_operand(&cmp.rhs)
        ),
        Expr::Not(inner) => format!("not ({})", format_expr(inner)),
        Expr::And(a, b) => format!("({}) and ({})", format_expr(a), format_expr(b)),
        Expr::Or(a, b) => format!("({}) or ({})", format_expr(a), format_expr(b)),
    }
}

fn format_operand(operand: &Operand) -> String {
    match operand {
        Operand::Column(value) => value.clone(),
        Operand::QualifiedColumn(column) => {
            format!("{}.{}", format_qref(&column.entity), column.column)
        }
        Operand::IntLit(value) => value.clone(),
        Operand::Now => "now".to_string(),
    }
}

fn format_cmp_op(op: &CmpOp) -> &'static str {
    match op {
        CmpOp::Lt => "<",
        CmpOp::Gt => ">",
        CmpOp::Le => "<=",
        CmpOp::Ge => ">=",
        CmpOp::Eq => "==",
        CmpOp::Ne => "!=",
    }
}

fn format_qref(qref: &QRef) -> String {
    if let Some(kind) = &qref.kind_qualifier {
        format!("{}::{}", kind.name(), qref.parts.join("."))
    } else {
        qref.parts.join(".")
    }
}

fn format_metadata_value(value: &str) -> String {
    if is_identish(value) {
        value.to_string()
    } else {
        quote_string(value)
    }
}

fn quote_string(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "'"))
}

fn bool_cell(value: bool) -> String {
    if value { "true" } else { "false" }.to_string()
}

fn is_identish(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_source_and_preserves_parseability() {
        let src = r#"module shop.checkout
import shared.actors.{Customer as Buyer, Staff}
requirement ReqCheckout "Checkout reliable" priority "must" source "Interview"
adr AdrOutbox "Use outbox" adr_status accepted decision "Use transactional outbox." reason "Avoid synchronous callbacks."
api CreateOrder "Create order" method POST path "/orders" auth bearer
dto CreateOrderRequest "Create order request" {customer_id:Int note:String @null}
invariant(Order).when(status == paid).then(total > 0)
"#;

        let formatted = format_source(src).unwrap();

        assert_eq!(
            formatted,
            r#"module shop.checkout

import shared.actors.{Customer as Buyer, Staff}

requirement ReqCheckout "Checkout reliable"
  priority "must"
  source "Interview"

adr AdrOutbox "Use outbox"
  adr_status accepted
  decision "Use transactional outbox."
  reason "Avoid synchronous callbacks."

api CreateOrder "Create order"
  method POST
  path "/orders"
  auth bearer

dto CreateOrderRequest "Create order request" {
  customer_id: Int
  note: String @null
}

invariant(Order).when(status == paid).then(total > 0)
"#
        );

        let (_ast, errors) = parse(&formatted);
        assert!(
            errors.is_empty(),
            "formatted output should parse: {errors:?}"
        );
    }
}
