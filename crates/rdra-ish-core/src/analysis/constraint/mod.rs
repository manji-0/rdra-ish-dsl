//! Entity and cross-entity constraint predicate helpers.

mod cross;
mod entity_conditions;
mod entity_scope;
mod guards;
mod quantifier;
mod rules;
mod temporal;

use crate::analysis_diag::*;
use crate::diagnostics::*;
use crate::location::DiagCtxt;
use rdra_ish_syntax::ast::*;

#[cfg(test)]
pub(crate) use self::cross::cross_scope_semantics_from_chain;
pub(crate) use self::cross::{
    add_condition_entities_to_scope, along_scope, resolve_cross_condition,
};
#[cfg(test)]
pub(crate) use self::entity_conditions::push_unique_entity;
pub(crate) use self::entity_conditions::{
    collect_entity_conditions, context_value_from_arg, resolve_entity_equals_from_comparison,
};
pub(crate) use self::entity_scope::{find_entity_column, resolve_entity_qref};
pub(crate) use self::guards::{
    forbidden_when_conditions, push_rule_guards, resolve_rule_guards, rule_label,
    when_condition_ref,
};
pub(crate) use self::quantifier::{check_quantifier_paths, resolve_related_quantifier};
pub(crate) use self::rules::{process_forbidden, process_invariant};
pub(crate) use self::temporal::process_after_predicate;

// ── 制約述語用ヘルパー ────────────────────────────────────────────────────────

/// `Lit(s)` または kind修飾なし1セグメントの `Ref` から文字列を取り出す。
/// `whenstatus == delivered` の裸ident引数と `sets(...)` の引用符付きリテラル
/// 引数の両方を許容するための統一抽出。
pub(crate) fn arg_as_str(arg: &PredicateArg) -> Option<String> {
    match arg {
        PredicateArg::Lit(s) => Some(s.clone()),
        PredicateArg::Ref(qref) if qref.kind_qualifier.is_none() && qref.parts.len() == 1 => {
            Some(qref.parts[0].clone())
        }
        _ => None,
    }
}

/// The chain call a `.role(role)` at `at` selects the key of: the nearest `.along`, `.has` or
/// `.none` before it, or `None` when it follows none, where it selects the key of the
/// statement itself (a `holds` body, a copy).
fn by_owner(chain: &[ChainCall], at: usize) -> Option<usize> {
    chain[..at]
        .iter()
        .rposition(|cc| matches!(cc.name.as_str(), "along" | "has" | "none"))
}

/// The roles the `.role(..)` calls that select the key of `owner` name (see [`by_owner`]).
pub(crate) fn selectors(chain: &[ChainCall], owner: Option<usize>) -> Vec<String> {
    chain
        .iter()
        .enumerate()
        .filter(|(at, cc)| cc.name == "role" && by_owner(chain, *at) == owner)
        .flat_map(|(_, cc)| cc.args.iter().filter_map(arg_as_str))
        .collect()
}

/// The `.role(..)` calls that follow no `.along`, `.has` or `.none`, on a statement that has
/// no key of its own to select.
pub(crate) fn report_leading_by(chain: &[ChainCall], ctx: DiagCtxt, diags: &mut Vec<Diagnostic>) {
    for (at, cc) in chain.iter().enumerate() {
        if cc.name == "role" && by_owner(chain, at).is_none() {
            push_error(ctx, diags, cc.span.clone(), RdraError::RoleWithoutJoin);
        }
    }
}
