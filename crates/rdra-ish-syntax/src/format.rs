//! Canonical source formatter for the RDRA-ish DSL.

use chumsky::error::Simple;

use crate::ast::*;
use crate::parser::parse;
use crate::token::Token;
use unicode_width::UnicodeWidthStr;

/// The widest line, in display columns, that keeps a declaration and its description on one
/// line. Columns are counted as a terminal shows them: an East Asian wide or fullwidth
/// character (most Japanese text) is two columns, others one. Trailing comments do not count.
const MAX_WIDTH: usize = 120;

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

/// Pretty-print an already-parsed AST, with its comments.
pub fn format_ast(ast: &Ast) -> String {
    let mut items: Vec<Vec<Line>> = ast.items.iter().map(item_lines).collect();
    let mut end_of_file = Vec::new();
    for comment in &ast.comments {
        place_comment(ast, comment, &mut items, &mut end_of_file);
    }

    let mut out = String::new();
    for (index, lines) in items.iter().enumerate() {
        if index > 0 && blank_line_between(ast, &items, index - 1) {
            out.push('\n');
        }
        for line in lines {
            for above in &line.above {
                out.push_str(line.comment_indent);
                push_comment_line(&mut out, above);
            }
            out.push_str(&line.text);
            for comment in &line.trailing {
                out.push(' ');
                out.push_str(comment);
            }
            out.push('\n');
        }
    }
    if !end_of_file.is_empty() && !items.is_empty() {
        out.push('\n');
    }
    if let Some((last, rest)) = end_of_file.split_last() {
        for comment in rest {
            push_comment_line(&mut out, comment);
        }
        out.push_str(&last.text);
        out.push('\n');
    }
    out
}

/// Whether a blank line separates item `prev` from the item after it.
///
/// Consecutive imports form one block with no blank line inside it, and a blank line
/// follows the module line and the import block. An item that prints on several lines is
/// set off by blank lines. Between two one-line items the author's grouping stands: a
/// blank line there is kept (several collapse to one), and none is added.
fn blank_line_between(ast: &Ast, items: &[Vec<Line>], prev: usize) -> bool {
    let (before, after) = (&ast.items[prev], &ast.items[prev + 1]);
    match (before, after) {
        (Item::Import(_), Item::Import(_)) => false,
        (Item::Module(..) | Item::Import(_), _) => true,
        _ => {
            items[prev].len() > 1
                || items[prev + 1].len() > 1
                || source_blank_after(ast, item_span(before).end)
        }
    }
}

/// Whether the source has a blank line after the item ending at `end`, past the comments
/// that trail it on its last line.
fn source_blank_after(ast: &Ast, end: usize) -> bool {
    let src = &ast.source;
    let mut at = end;
    loop {
        at += src[at..].len() - src[at..].trim_start_matches([' ', '\t']).len();
        match ast.comments.iter().find(|comment| comment.span.start == at) {
            Some(comment) => at = comment.span.end,
            None => return blank_line_follows(src, at),
        }
    }
}

/// One output line of an item.
struct Line {
    text: String,
    /// Source offset of the node this line prints. Comments attach only to anchored lines;
    /// lines without a span of their own (metadata, a property's formula) have none.
    anchor: Option<usize>,
    /// Indentation of a comment on its own line above this one.
    comment_indent: &'static str,
    above: Vec<OwnLineComment>,
    trailing: Vec<String>,
}

impl Line {
    fn new(text: String, anchor: Option<usize>, comment_indent: &'static str) -> Self {
        Line {
            text,
            anchor,
            comment_indent,
            above: Vec::new(),
            trailing: Vec::new(),
        }
    }
}

struct OwnLineComment {
    text: String,
    /// The source has a blank line after the comment. Kept only between top-level items.
    blank_after: bool,
}

fn push_comment_line(out: &mut String, comment: &OwnLineComment) {
    out.push_str(&comment.text);
    out.push('\n');
    if comment.blank_after {
        out.push('\n');
    }
}

/// Attach `comment` to the output line it annotates.
///
/// A trailing comment stays at the end of the line that prints the node it follows on the
/// same source line. Any other comment goes on its own line above the next anchored line
/// of the item that contains it, or above the item when that item has no later anchored
/// line (its metadata lines carry no spans). Between items it goes above the next item,
/// and after the last item to the end of the file. Each placement reads back to itself,
/// so a second `fmt` changes nothing.
fn place_comment(
    ast: &Ast,
    comment: &Comment,
    items: &mut [Vec<Line>],
    end_of_file: &mut Vec<OwnLineComment>,
) {
    let src = &ast.source;
    let start = comment.span.start;
    let text = src[comment.span.clone()].trim_end().to_string();
    let same_line = |anchor: usize| !src[anchor..start].contains('\n');

    if comment.trailing {
        let preceding = anchored(items)
            .filter(|&(_, _, anchor)| anchor <= start)
            .last();
        if let Some((item, line, anchor)) = preceding {
            // Only comments from the anchor's own source line trail it, in source order, so
            // a `//` comment is always the last on its line.
            if same_line(anchor) {
                items[item][line].trailing.push(text);
            } else {
                items[item][0].above.push(OwnLineComment {
                    text,
                    blank_after: false,
                });
            }
            return;
        }
    }

    let container = ast
        .items
        .iter()
        .position(|item| item_span(item).contains(&start));
    let following = anchored(items).find(|&(_, _, anchor)| anchor > start);
    let (item, line) = match (container, following) {
        (Some(item), Some((next, line, _))) if next == item => (item, line),
        (Some(item), _) => (item, 0),
        (None, Some((item, line, _))) => (item, line),
        (None, None) => {
            let blank_after = blank_line_follows(src, comment.span.end);
            end_of_file.push(OwnLineComment { text, blank_after });
            return;
        }
    };
    let blank_after = container.is_none() && blank_line_follows(src, comment.span.end);
    items[item][line]
        .above
        .push(OwnLineComment { text, blank_after });
}

/// Every anchored line as `(item index, line index, anchor)`, in output order.
fn anchored(items: &[Vec<Line>]) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
    items.iter().enumerate().flat_map(|(item, lines)| {
        lines
            .iter()
            .enumerate()
            .filter_map(move |(line, l)| l.anchor.map(|anchor| (item, line, anchor)))
    })
}

fn blank_line_follows(src: &str, end: usize) -> bool {
    let rest = &src[end..];
    let gap = &rest[..rest.len() - rest.trim_start().len()];
    gap.matches('\n').count() >= 2
}

fn item_span(item: &Item) -> &Span {
    match item {
        Item::Module(_, span) => span,
        Item::Import(import) => &import.span,
        Item::Instance(instance) => &instance.span,
        Item::Predicate(predicate) => &predicate.span,
        Item::Property(prop) => &prop.span,
    }
}

/// The lines of `item`. The first line is anchored at the item; an instance also anchors
/// each column or variation value line and its closing brace.
fn item_lines(item: &Item) -> Vec<Line> {
    let text = match item {
        Item::Instance(instance) => return instance_lines(instance),
        Item::Module(name, _) => format!("module {}", format_dotted_name(name)),
        Item::Import(import) => format_import(import),
        Item::Predicate(predicate) => format_predicate(predicate),
        Item::Property(prop) => format_property(prop),
    };
    let start = item_span(item).start;
    text.lines()
        .enumerate()
        .map(|(index, line)| Line::new(line.to_string(), (index == 0).then_some(start), ""))
        .collect()
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
    let chain: String = prop.chain.iter().map(format_chain_call).collect();
    let formula = format!("{formula}{chain}");
    match &prop.label {
        Some(label) => format!(
            "property {} {}\n  {}",
            prop.id,
            quote_string(label),
            formula
        ),
        None => format!("property {}\n  {}", prop.id, formula),
    }
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

fn instance_lines(instance: &InstanceDecl) -> Vec<Line> {
    let header = format!(
        "{} {} {}",
        instance.kind.name(),
        instance.id,
        quote_string(&instance.label)
    );
    let mut metadata = Vec::new();
    if let Some(description) = &instance.description {
        metadata.push(format!(
            "  description {}",
            meta_value(instance, "description", description)
        ));
    }
    metadata.extend(format_requirement_metadata(instance));
    metadata.extend(format_adr_metadata(instance));
    metadata.extend(format_api_metadata(instance));
    metadata.extend(format_nfr_metadata(instance));
    metadata.extend(format_field_metadata(instance));
    metadata.extend(format_usecase_metadata(instance));

    let body: Vec<(String, usize)> = if !instance.columns.is_empty() {
        instance
            .columns
            .iter()
            .map(|column| (format_column(column), column.span.start))
            .collect()
    } else {
        instance
            .variation_values
            .iter()
            .map(|value| {
                let text = match &value.label {
                    Some(label) => format!("{} {}", value.value, quote_string(label)),
                    None => value.value.clone(),
                };
                (text, value.span.start)
            })
            .collect()
    };
    let opener = if body.is_empty() { "" } else { " {" };

    // A declaration whose only clause is its description keeps it on its own line while
    // the line fits; any other clause goes on an indented line of its own.
    let mut lines = vec![Line::new(header, Some(instance.span.start), "")];
    match &metadata[..] {
        // `  description …` with one of its two indenting spaces kept as the separator.
        [description]
            if instance.description.is_some()
                && lines[0].text.width() + description[1..].width() + opener.len() <= MAX_WIDTH =>
        {
            lines[0].text.push_str(&description[1..]);
        }
        _ => lines.extend(metadata.into_iter().map(|text| Line::new(text, None, ""))),
    }

    if !body.is_empty() {
        let last = lines.len() - 1;
        lines[last].text.push_str(opener);
        for (text, anchor) in body {
            lines.push(Line::new(format!("  {text}"), Some(anchor), "  "));
        }
        lines.push(Line::new(
            "}".to_string(),
            Some(instance.span.end - 1),
            "  ",
        ));
    }
    lines
}

fn format_requirement_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.requirement.priority {
        lines.push(format!(
            "  priority {}",
            meta_value(instance, "priority", value)
        ));
    }
    for value in &instance.requirement.sources {
        lines.push(format!(
            "  source {}",
            meta_value(instance, "source", value)
        ));
    }
    for value in &instance.requirement.stakeholders {
        lines.push(format!(
            "  stakeholder {}",
            meta_value(instance, "stakeholder", value)
        ));
    }
    if let Some(value) = &instance.requirement.owner {
        lines.push(format!("  owner {}", meta_value(instance, "owner", value)));
    }
    for value in &instance.requirement.acceptance_criteria {
        lines.push(format!(
            "  acceptance_criteria {}",
            meta_value(instance, "acceptance_criteria", value)
        ));
    }
    if let Some(value) = &instance.requirement.status {
        lines.push(format!(
            "  status {}",
            meta_value(instance, "status", value)
        ));
    }
    if let Some(value) = &instance.requirement.risk {
        lines.push(format!("  risk {}", meta_value(instance, "risk", value)));
    }
    if let Some(value) = &instance.requirement.rationale {
        lines.push(format!(
            "  rationale {}",
            meta_value(instance, "rationale", value)
        ));
    }
    lines
}

fn format_adr_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.adr.status {
        lines.push(format!(
            "  adr_status {}",
            meta_value(instance, "adr_status", value)
        ));
    }
    for value in &instance.adr.context {
        lines.push(format!(
            "  context {}",
            meta_value(instance, "context", value)
        ));
    }
    if let Some(value) = &instance.adr.decision {
        lines.push(format!(
            "  decision {}",
            meta_value(instance, "decision", value)
        ));
    }
    for value in &instance.adr.consequences {
        lines.push(format!(
            "  consequence {}",
            meta_value(instance, "consequence", value)
        ));
    }
    for value in &instance.adr.accepted_options {
        lines.push(format!(
            "  accepted {}",
            meta_value(instance, "accepted", value)
        ));
    }
    for value in &instance.adr.rejected_options {
        lines.push(format!(
            "  rejected {}",
            meta_value(instance, "rejected", value)
        ));
    }
    for value in &instance.adr.reasons {
        lines.push(format!(
            "  reason {}",
            meta_value(instance, "reason", value)
        ));
    }
    lines
}

fn format_api_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.api.method {
        lines.push(format!(
            "  method {}",
            meta_value(instance, "method", value)
        ));
    }
    if let Some(value) = &instance.api.path {
        lines.push(format!("  path {}", meta_value(instance, "path", value)));
    }
    if let Some(value) = &instance.api.idempotency {
        lines.push(format!(
            "  idempotency {}",
            meta_value(instance, "idempotency", value)
        ));
    }
    if let Some(value) = &instance.api.mode {
        lines.push(format!("  mode {}", meta_value(instance, "mode", value)));
    }
    if let Some(value) = &instance.api.auth_scheme {
        lines.push(format!("  auth {}", meta_value(instance, "auth", value)));
    }
    lines
}

fn format_nfr_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.nfr.metric {
        lines.push(format!(
            "  metric {}",
            meta_value(instance, "metric", value)
        ));
    }
    if let Some(value) = &instance.nfr.baseline {
        lines.push(format!(
            "  baseline {}",
            meta_value(instance, "baseline", value)
        ));
    }
    if let Some(value) = &instance.nfr.target {
        lines.push(format!(
            "  target {}",
            meta_value(instance, "target", value)
        ));
    }
    if let Some(value) = &instance.nfr.window {
        lines.push(format!(
            "  window {}",
            meta_value(instance, "window", value)
        ));
    }
    if let Some(value) = &instance.nfr.slo {
        lines.push(format!("  slo {}", meta_value(instance, "slo", value)));
    }
    if let Some(value) = &instance.nfr.availability {
        lines.push(format!(
            "  availability {}",
            meta_value(instance, "availability", value)
        ));
    }
    if let Some(value) = &instance.nfr.resilience {
        lines.push(format!(
            "  resilience {}",
            meta_value(instance, "resilience", value)
        ));
    }
    if let Some(value) = &instance.nfr.audit {
        lines.push(format!("  audit {}", meta_value(instance, "audit", value)));
    }
    if let Some(value) = &instance.nfr.logging {
        lines.push(format!(
            "  logging {}",
            meta_value(instance, "logging", value)
        ));
    }
    if let Some(value) = &instance.nfr.retention {
        lines.push(format!(
            "  retention {}",
            meta_value(instance, "retention", value)
        ));
    }
    if let Some(value) = &instance.nfr.privacy {
        lines.push(format!(
            "  privacy {}",
            meta_value(instance, "privacy", value)
        ));
    }
    lines
}

fn format_field_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(value) = &instance.field.access {
        lines.push(format!(
            "  access {}",
            meta_value(instance, "access", value)
        ));
    }
    if let Some(value) = instance.field.required {
        lines.push(format!("  required {}", bool_cell(value)));
    }
    if let Some(value) = &instance.field.source {
        lines.push(format!(
            "  source {}",
            meta_value(instance, "source", value)
        ));
    }
    lines
}

fn format_usecase_metadata(instance: &InstanceDecl) -> Vec<String> {
    let mut lines = Vec::new();
    for value in &instance.usecase.preconditions {
        lines.push(format!(
            "  precondition {}",
            meta_value(instance, "precondition", value)
        ));
    }
    for value in &instance.usecase.guards {
        lines.push(format!("  guard {}", meta_value(instance, "guard", value)));
    }
    for value in &instance.usecase.postconditions {
        lines.push(format!(
            "  postcondition {}",
            meta_value(instance, "postcondition", value)
        ));
    }
    for value in &instance.usecase.alternatives {
        lines.push(format!(
            "  alternative {}",
            meta_value(instance, "alternative", value)
        ));
    }
    for value in &instance.usecase.errors {
        lines.push(format!("  error {}", meta_value(instance, "error", value)));
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
        ColType::Enum(values) => format!(
            "Enum({})",
            values
                .iter()
                .map(|value| match &value.label {
                    Some(label) => format!("{} {}", value.value, quote_string(label)),
                    None => value.value.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ColType::Named(name) => name.clone(),
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
        Annotation::Check(expr) => format!("@check({})", format_column_expr(expr)),
        Annotation::Null => "@null".to_string(),
        Annotation::Default { value, .. } => {
            format!("@default({})", format_metadata_value(value))
        }
        Annotation::Label(value) => format!("@label({})", quote_string(value)),
        Annotation::SoftDelete => "@soft_delete".to_string(),
        Annotation::History => "@history".to_string(),
        Annotation::Tenant => "@tenant".to_string(),
        Annotation::Derived(expr) => format!("@derived({})", format_column_expr(expr)),
    }
}

pub(crate) fn format_predicate(predicate: &PredicateCall) -> String {
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
        PredicateArg::Transition { from, to } => format!("{} -> {to}", format_sources(from)),
        PredicateArg::Card(card) => card.clone(),
    }
}

/// The sources of a transition arm as written: `a`, or `(a, b)` for a list.
pub fn format_sources(from: &[String]) -> String {
    match from {
        [one] => one.clone(),
        many => format!("({})", many.join(", ")),
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

fn format_column_expr(expr: &ColumnExpr) -> String {
    match expr {
        ColumnExpr::Logical(expr) => format_expr(expr),
        ColumnExpr::Arith(arith) => format!(
            "{} {} {}",
            format_operand(&arith.lhs),
            arith.op.as_str(),
            format_operand(&arith.rhs)
        ),
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
        Operand::Values(values) => format!("({})", values.join(", ")),
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
        CmpOp::In => "in",
        CmpOp::NotIn => "not in",
    }
}

fn format_qref(qref: &QRef) -> String {
    if let Some(kind) = &qref.kind_qualifier {
        format!("{}::{}", kind.name(), qref.parts.join("."))
    } else {
        qref.parts.join(".")
    }
}

/// A metadata value as it was written: bare when the source wrote it bare, else quoted.
fn meta_value(instance: &InstanceDecl, keyword: &str, value: &str) -> String {
    let bare = instance
        .bare_values
        .iter()
        .any(|(k, v)| k == keyword && v == value);
    if bare {
        value.to_string()
    } else {
        quote_string(value)
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

    /// `src` formats to `expected`, and formatting `expected` again changes nothing.
    fn assert_formats_to(src: &str, expected: &str) {
        let formatted = format_source(src).unwrap();
        assert_eq!(formatted, expected);
        assert_eq!(
            format_source(&formatted).unwrap(),
            formatted,
            "not idempotent"
        );
    }

    #[test]
    fn keeps_the_use_case_first_and_entity_column_forms() {
        assert_formats_to(
            "transitions( Pay,Order.status, pending->paid )\n",
            "transitions(Pay, Order.status, pending -> paid)\n",
        );
        assert_formats_to(
            "transitions(Cancel, Order.status, ( pending,held )->cancelled)\n",
            "transitions(Cancel, Order.status, (pending, held) -> cancelled)\n",
        );
        assert_formats_to(
            "maps_field( Total ,Order.total )\n",
            "maps_field(Total, Order.total)\n",
        );
        assert_formats_to(
            "requirement R \"R\"\n  acceptance_criteria \"Paid orders ship.\"\n  priority must\n",
            "requirement R \"R\"\n  priority must\n  acceptance_criteria \"Paid orders ship.\"\n",
        );
    }

    #[test]
    fn a_removed_form_does_not_format() {
        for src in [
            "transitions(Order.status, Pay, pending -> paid)\n",
            "maps_field(Total, Order, \"total\")\n",
            "requirement R \"R\"\n  acceptance criteria \"x\"\n",
            "writes(Pay, Order)\n",
        ] {
            assert!(format_source(src).is_err(), "{src}");
        }
    }

    #[test]
    fn formats_a_goal_and_keeps_its_measure_in_order() {
        assert_formats_to(
            "goal   ShortWait \"待ちを短く\"\n  description \"患者が帰る。\"\n  window   monthly\n  target \"30分\"\n  baseline \"45分\"\n  metric \"中央値\"\n",
            "goal ShortWait \"待ちを短く\"\n  description \"患者が帰る。\"\n  metric \"中央値\"\n  baseline \"45分\"\n  target \"30分\"\n  window monthly\n",
        );
    }

    #[test]
    fn prints_consecutive_imports_as_one_block() {
        assert_formats_to(
            "module m\nimport a\n\nimport b.{X}\n// why c\nimport c as C\nactor A \"a\"\n",
            "module m\n\nimport a\nimport b.{X}\n// why c\nimport c as C\n\nactor A \"a\"\n",
        );
    }

    #[test]
    fn keeps_a_description_on_the_line_of_its_declaration_while_it_fits() {
        let fits = format!("usecase U \"u\" description \"{}\"\n", "x".repeat(92));
        assert_eq!(fits.trim_end().len(), MAX_WIDTH);
        assert_formats_to(&fits.replace(" description", "\n  description"), &fits);

        let over = format!("usecase U \"u\" description \"{}\"\n", "x".repeat(93));
        assert_formats_to(&over, &over.replace(" description", "\n  description"));
    }

    #[test]
    fn counts_a_wide_character_as_two_columns() {
        // 46 wide characters are 92 columns: the line is exactly MAX_WIDTH wide.
        let fits = format!("usecase U \"u\" description \"{}\"\n", "会".repeat(46));
        assert_formats_to(&fits, &fits);
        let over = format!("usecase U \"u\" description \"{}\"\n", "会".repeat(47));
        assert_formats_to(&over, &over.replace(" description", "\n  description"));
    }

    #[test]
    fn keeps_a_clause_beside_another_on_its_own_line_and_opens_a_body_after_it() {
        assert_formats_to(
            "requirement R \"r\" description \"d\" priority must\n",
            "requirement R \"r\"\n  description \"d\"\n  priority must\n",
        );
        assert_formats_to(
            "entity E \"e\" description \"d\" {\n  id: Int @pk\n}\n",
            "entity E \"e\" description \"d\" {\n  id: Int @pk\n}\n",
        );
        let long = "x".repeat(110);
        assert_formats_to(
            &format!("entity E \"e\" description \"{long}\" {{\n  id: Int @pk\n}}\n"),
            &format!("entity E \"e\"\n  description \"{long}\" {{\n  id: Int @pk\n}}\n"),
        );
    }

    #[test]
    fn keeps_runs_of_one_line_declarations_and_their_citations_together() {
        assert_formats_to(
            "// a.txt:1\nusecase A \"a\" description \"x\"\n// b.txt:2\nusecase B  \"b\"\n\n\nscreen S \"s\"\nusecase C \"c\" // c.txt:3\n  description \"y\"\nrequirement R \"r\"\n  priority must\n  status draft\nperforms(P, A)\n",
            "// a.txt:1\nusecase A \"a\" description \"x\"\n// b.txt:2\nusecase B \"b\"\n\nscreen S \"s\"\nusecase C \"c\" description \"y\" // c.txt:3\n\nrequirement R \"r\"\n  priority must\n  status draft\n\nperforms(P, A)\n",
        );
    }

    #[test]
    fn keeps_a_trailing_comment_on_the_header_of_a_wrapped_description() {
        let long = "x".repeat(110);
        assert_formats_to(
            &format!("actor A \"a\" description \"{long}\" // note\nactor B \"b\"\n"),
            &format!("actor A \"a\" // note\n  description \"{long}\"\n\nactor B \"b\"\n"),
        );
    }

    #[test]
    fn keeps_an_own_line_comment_above_the_item_it_precedes() {
        assert_formats_to(
            "module a\n// heading comment\nactor A \"a\"\n",
            "module a\n\n// heading comment\nactor A \"a\"\n",
        );
    }

    #[test]
    fn keeps_a_trailing_comment_on_the_line_it_ends() {
        assert_formats_to(
            "module a // the module\nactor A \"a\"   // trailing  \n",
            "module a // the module\n\nactor A \"a\" // trailing\n",
        );
    }

    #[test]
    fn keeps_block_comments_inline_and_on_their_own_lines() {
        assert_formats_to(
            "/* file\n   header */\nactor A \"a\" /* inline */\nactor B \"b\" /* both */ // kinds\n",
            "/* file\n   header */\nactor A \"a\" /* inline */\nactor B \"b\" /* both */ // kinds\n",
        );
    }

    #[test]
    fn keeps_comments_between_predicates() {
        assert_formats_to(
            "performs(A, U1)\n// sources/a.txt:12\nperforms(A, U2) // sources/b.txt:3\n// sources/c.txt:4\nperforms(A, U3)\n",
            "performs(A, U1)\n// sources/a.txt:12\nperforms(A, U2) // sources/b.txt:3\n// sources/c.txt:4\nperforms(A, U3)\n",
        );
    }

    #[test]
    fn keeps_comments_at_the_end_of_the_file() {
        assert_formats_to(
            "actor A \"a\"\n// footer one\n\n// footer two",
            "actor A \"a\"\n\n// footer one\n\n// footer two\n",
        );
    }

    #[test]
    fn keeps_a_file_of_only_comments() {
        assert_formats_to("// nothing yet\n", "// nothing yet\n");
    }

    #[test]
    fn keeps_comments_inside_an_entity_body() {
        assert_formats_to(
            "entity E \"e\" { // columns\n  id: Int @pk // key\n// a status\n  status: Enum(a, b)\n  // closing note\n} // end\n",
            "entity E \"e\" { // columns\n  id: Int @pk // key\n  // a status\n  status: Enum(a, b)\n  // closing note\n} // end\n",
        );
    }

    #[test]
    fn keeps_comments_between_variation_values() {
        assert_formats_to(
            "variation V \"v\" {\n  a \"A\"\n  // why b\n  b \"B\" // b note\n}\n",
            "variation V \"v\" {\n  a \"A\"\n  // why b\n  b \"B\" // b note\n}\n",
        );
    }

    #[test]
    fn keeps_the_blank_line_between_a_detached_comment_and_the_next_item() {
        assert_formats_to(
            "// section\n\nactor A \"a\"\n\n// group\n// of two\n\n\nactor B \"b\"\n",
            "// section\n\nactor A \"a\"\n\n// group\n// of two\n\nactor B \"b\"\n",
        );
    }

    #[test]
    fn moves_a_comment_from_a_line_without_its_own_anchor_above_its_item() {
        assert_formats_to(
            "requirement R \"r\"\n  priority \"must\"\n  // why the source\n  source \"Interview\" // who said it\n\ninvariant(Order)\n  // only when paid\n  .when(status == paid).then(total > 0) // wrapped call\n",
            "// why the source\n// who said it\nrequirement R \"r\"\n  priority \"must\"\n  source \"Interview\"\n\n// only when paid\n// wrapped call\ninvariant(Order).when(status == paid).then(total > 0)\n",
        );
    }

    #[test]
    fn does_not_read_a_slash_pair_inside_a_string_as_a_comment() {
        assert_formats_to(
            "actor A \"see http://x /* y */\"\n",
            "actor A \"see http://x /* y */\"\n",
        );
    }

    #[test]
    fn formats_set_membership() {
        assert_formats_to(
            "forbidden(Order,status  in(a,b))\n",
            "forbidden(Order, status in (a, b))\n",
        );
        assert_formats_to(
            "holds(C, Order.status not   in (a), Order.qty in (1,2))\n",
            "holds(C, Order.status not in (a), Order.qty in (1, 2))\n",
        );
    }

    #[test]
    fn formats_check_and_derived_unquoted() {
        let src = "entity E \"E\" {\n  qty: Int @check(qty >= 1 and not qty > 4)\n  net: Int @derived(qty * 2)\n}\n";
        let formatted = format_source(src).unwrap();
        assert!(
            formatted.contains("@check((qty >= 1) and (not (qty > 4)))"),
            "{formatted}"
        );
        assert!(formatted.contains("@derived(qty * 2)"), "{formatted}");
        assert_eq!(format_source(&formatted).unwrap(), formatted);
    }

    #[test]
    fn formats_a_property_fair_chain() {
        let src = "property GrantEnds\n  leads_to(A.status == granted, A.status == registered).fair(Pay, Register)\n";
        let formatted = format_source(src).unwrap();
        assert_eq!(formatted, src);
    }

    #[test]
    fn formats_nm_via_chain() {
        let src = "relate(Order, Product, N:M).via(OrderProduct)\n";
        let formatted = format_source(src).unwrap();
        assert!(
            formatted.contains("relate(Order, Product, N:M).via(OrderProduct)"),
            "{formatted}"
        );
        let (_ast, errors) = parse(&formatted);
        assert!(
            errors.is_empty(),
            "formatted output should parse: {errors:?}"
        );
    }

    #[test]
    fn formats_a_holds_quantifier_chain() {
        let src = "holds(C, A.status == active).none(B.status == active).has(B.status == ended)\n";
        let formatted = format_source(src).unwrap();
        assert_eq!(formatted, src);
    }

    #[test]
    fn formats_a_rule_label_as_written() {
        let src = "invariant(Appointment).when(status == checkedin).then(checked_in_at == present).label(\"受付済の予約には受付時刻がある\")\n\nforbidden(A, B, A.status == x).along(A, B).label(\"x\")\n\nafter(Pay).assert(Order.status == paid).label(\"支払後は入金済\")\n\ninvariant(Cert).when(status == revoked).none(Assign, status == active).label(\"失効後は割当なし\")\n";
        let formatted = format_source(src).unwrap();
        assert_eq!(formatted, src);
        let (_ast, errors) = parse(&formatted);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn formats_a_screen_transition_as_written() {
        let src =
            "navigates(CheckInScreen, EligibilityScreen).via(CheckInPatient)\n\nnavigates(A, B)\n";
        let formatted = format_source(src).unwrap();
        assert_eq!(formatted, src);
    }

    #[test]
    fn formats_a_cardinality_range_as_written() {
        let formatted =
            format_source("relate(Line, Order, 1 .. *:1)\nrelate(Note, Order, 0..1:1)\n").unwrap();
        assert_eq!(
            formatted,
            "relate(Line, Order, 1..*:1)\nrelate(Note, Order, 0..1:1)\n"
        );
    }

    #[test]
    fn formats_variation_values_and_named_column_types() {
        let src = "variation PaymentMethod \"Payment Method\" {cash \"Cash\" card\n adjustment \"Adj\"}\nentity E \"E\" {method:PaymentMethod @default(card)}\n";
        let formatted = format_source(src).unwrap();
        assert_eq!(
            formatted,
            "variation PaymentMethod \"Payment Method\" {\n  cash \"Cash\"\n  card\n  adjustment \"Adj\"\n}\n\nentity E \"E\" {\n  method: PaymentMethod @default(card)\n}\n"
        );
        let (_ast, errors) = parse(&formatted);
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn keeps_the_labels_of_enum_values() {
        let formatted =
            format_source("entity E \"E\" {status:Enum(open \"受付中\",closed)}\n").unwrap();
        assert_eq!(
            formatted,
            "entity E \"E\" {\n  status: Enum(open \"受付中\", closed)\n}\n"
        );
    }

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
