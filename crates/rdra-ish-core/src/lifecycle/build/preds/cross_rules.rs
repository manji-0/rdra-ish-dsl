//! Rules over several entities: scoped cross-entity invariants and related-row quantifiers.

use super::display::{cross_display, entity_list, quantifier_display};
use super::guards::{Chain, PathOwner};
use super::{at_of, implies, Binding};
use crate::lifecycle::build::Builder;
use crate::lifecycle::diag::StateDiag;
use crate::lifecycle::types::{At, InvKind, InvText, ModelRef, Pred, Site};
use crate::model::{CrossConstraintScope, EntityKey, FkId, QuantifierKind, RelatedQuantifier};

impl Builder<'_> {
    /// Binders for a cross constraint's scope, plus the relate links its `.along` names.
    fn scope_binding(
        &mut self,
        scope: &[EntityKey],
        semantics: &CrossConstraintScope,
        constraint: &str,
        site: &Site,
    ) -> Option<(Binding, Pred)> {
        let mut binding: Binding = Vec::new();
        for &ek in scope {
            if at_of(&binding, ek).is_none() {
                binding.push((ek, At::Bound(binding.len() as u8)));
            }
        }
        let CrossConstraintScope::RelationPath {
            path, links: keys, ..
        } = semantics
        else {
            return Some((binding, Pred::True));
        };
        if let Some(out) = scope.iter().find(|ek| !path.contains(ek)) {
            let id = self.model.entities[*out].id.clone();
            let reason = format!("along(..) does not cover {id}");
            self.unbindable_scope(scope, constraint, reason, site);
            return None;
        }
        // A path entity the conditions do not read joins the binders. The conditions do not
        // depend on it, so binding it universally under the link is binding it existentially.
        for &ek in path {
            if at_of(&binding, ek).is_none() {
                binding.push((ek, At::Bound(binding.len() as u8)));
            }
        }
        match self.hop_links(path, keys, &binding) {
            Ok(links) => Some((binding, Pred::And(links))),
            Err(reason) => {
                self.unbindable_scope(scope, constraint, reason, site);
                None
            }
        }
    }

    /// `UnbindableScope` on `scope`: the rule `constraint` binds nothing for `reason`.
    pub(super) fn unbindable_scope(
        &mut self,
        scope: &[EntityKey],
        constraint: &str,
        reason: String,
        site: &Site,
    ) {
        self.diag(
            scope.to_vec(),
            StateDiag::UnbindableScope {
                constraint: constraint.to_string(),
                reason,
                site: site.clone(),
            },
        );
    }

    /// The owner links of the hops of an `.along(..)` path, each through the key resolved
    /// for it, over a `binding` that binds every path entity. `Err` says why the path binds
    /// nothing: it is too short, or no key joins a hop.
    pub(super) fn hop_links(
        &self,
        path: &[EntityKey],
        keys: &[Option<FkId>],
        binding: &Binding,
    ) -> Result<Vec<Pred>, String> {
        if path.len() < 2 {
            return Err("along(..) needs at least two entities".to_string());
        }
        let mut links = Vec::new();
        for (i, pair) in path.windows(2).enumerate() {
            let key = keys.get(i).copied().flatten();
            let bound = |ek| at_of(binding, ek).expect("every path entity is bound");
            let (a, b) = (bound(pair[0]), bound(pair[1]));
            match key.and_then(|fk| self.key_link(fk, (pair[0], a), (pair[1], b))) {
                Some(link) => links.push(link),
                None => {
                    let (x, y) = (
                        &self.model.entities[pair[0]].id,
                        &self.model.entities[pair[1]].id,
                    );
                    return Err(format!("no N:1, 1:N, or 1:1 relate binds {x} and {y}"));
                }
            }
        }
        Ok(links)
    }

    pub(super) fn cross_invariants(&mut self) {
        let model = self.model;
        for (i, c) in model.cross_forbidden_constraints.iter().enumerate() {
            let site = Site::new(ModelRef::CrossForbidden(i), &c.site);
            let conditions = cross_display(model, &c.conditions);
            let constraint = format!("forbidden({conditions})");
            let Some((binding, link)) =
                self.scope_binding(&c.scope, &c.scope_semantics, &constraint, &site)
            else {
                continue;
            };
            let conds = self.cross_all(&c.conditions, &binding, &site);
            let body = implies(link, Pred::Not(Box::new(conds)));
            let text = InvText::Cross {
                entities: entity_list(model, &c.scope),
                conditions,
            };
            self.push_invariant(
                "CrossForbidden",
                i,
                InvKind::CrossForbidden,
                &binding,
                body,
                text,
                c.label.clone(),
                site,
            );
        }
        for (i, c) in model.cross_entity_invariants.iter().enumerate() {
            let site = Site::new(ModelRef::CrossInvariant(i), &c.site);
            let guards = cross_display(model, &c.guards);
            let requireds = cross_display(model, &c.requireds);
            let constraint = format!("invariant(when {guards} then {requireds})");
            let Some((binding, link)) =
                self.scope_binding(&c.scope, &c.scope_semantics, &constraint, &site)
            else {
                continue;
            };
            let g = self.cross_all(&c.guards, &binding, &site);
            let r = self.cross_all(&c.requireds, &binding, &site);
            let body = implies(link, implies(g, r));
            let text = InvText::CrossWhenThen {
                entities: entity_list(model, &c.scope),
                guards,
                requireds,
            };
            self.push_invariant(
                "CrossInv",
                i,
                InvKind::CrossInvariant,
                &binding,
                body,
                text,
                c.label.clone(),
                site,
            );
        }
    }

    /// `Has` is a witness among the related rows, `None` a universal over them. The related
    /// instance is `At::Bound(depth)`, linked to every entity of `binding` it has an owner var
    /// with; with no such relate the related rows are quantified independently.
    ///
    /// The quantifier ranges over the rows present in the pre-state. Its conditions read the
    /// quantified row even where `binding` already holds the related entity (the instance a
    /// use case creates or updates, a witness, or the anchor itself), so a guard on a use case
    /// that creates the related entity never reads the instance being created.
    pub(super) fn quantified(
        &mut self,
        q: &RelatedQuantifier,
        binding: &Binding,
        depth: u8,
        site: &Site,
    ) -> Pred {
        let mut inner: Binding = binding
            .iter()
            .filter(|(ek, _)| *ek != q.related)
            .copied()
            .collect();
        inner.push((q.related, At::Bound(depth)));
        let conds = self.cross_all(&q.conditions, &inner, site);
        let statement = quantifier_display(self.model, q);
        let links: Vec<Pred> = binding
            .iter()
            .filter_map(|&owner| {
                let related = (q.related, At::Bound(depth));
                self.owner_link(owner, related, &q.by, &statement, site)
            })
            .collect();
        let link = if links.is_empty() {
            Pred::True
        } else {
            Pred::And(links)
        };
        let entity = self.ent_of[&q.related];
        match q.kind {
            QuantifierKind::Has => Pred::Exists {
                entity,
                body: Box::new(Pred::And(vec![link, conds])),
            },
            QuantifierKind::None => Pred::Forall {
                entity,
                body: Box::new(implies(link, Pred::Not(Box::new(conds)))),
            },
        }
    }

    pub(super) fn quantifiers(&mut self) {
        let model = self.model;
        for (i, c) in model.quantifier_constraints.iter().enumerate() {
            let q = &c.quantifier;
            let site = Site::new(ModelRef::Quantifier(i), &q.site);
            let anchor: Binding = vec![(c.anchor, At::Bound(0))];
            let guards = self.cross_all(&c.guards, &anchor, &site);
            let kind = match q.kind {
                QuantifierKind::Has => "has",
                QuantifierKind::None => "none",
            };
            let mut constraint = format!(
                "{} when ({}) {} {}",
                model.entities[c.anchor].id,
                cross_display(model, &c.guards),
                kind,
                model.entities[q.related].id,
            );
            if !q.conditions.is_empty() {
                constraint.push_str(&format!(" where ({})", cross_display(model, &q.conditions)));
            }
            let inner = match &c.along {
                CrossConstraintScope::RelationPath { path, links, .. } => {
                    constraint.push_str(&format!(" along {}", entity_list(model, path)));
                    let chain = Chain {
                        segment: path,
                        keys: links,
                        owner: PathOwner::Rule {
                            constraint: &constraint,
                            scope: &[c.anchor],
                        },
                        site: &site,
                    };
                    self.quantified_along(q, &anchor, 1, chain, &site)
                }
                CrossConstraintScope::GlobalProduct => self.quantified(q, &anchor, 1, &site),
            };
            let text = InvText::Quantifier {
                anchor: model.entities[c.anchor].id.clone(),
                related: model.entities[q.related].id.clone(),
                constraint,
            };
            self.push_invariant(
                "Quantifier",
                i,
                InvKind::Quantifier,
                &anchor,
                implies(guards, inner),
                text,
                c.label.clone(),
                site,
            );
        }
    }
}
