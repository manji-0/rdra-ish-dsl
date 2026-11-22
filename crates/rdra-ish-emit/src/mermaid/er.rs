//! The ER diagram (`--kind er`).

use super::entity_neighbourhood;
use crate::diagram_common::{col_type_str, Diagram, Header, NodeScope};
use crate::{is_relate, EmitError, Emitter, Scope, View};
use rdra_ish_core::model::{NodeRef, SemanticModel};

pub struct ErMermaidEmitter;

impl Emitter for ErMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let reachable = match &view.scope {
            Scope::Bucs(buc_ids) => {
                NodeScope::Only(rdra_ish_core::reachable_from_bucs(model, buc_ids))
            }
            Scope::Whole | Scope::UseCases(_) => entity_neighbourhood(model, &view.state.entities),
        };

        let is_visible = |nr: &NodeRef| reachable.contains(nr);

        let mut ents: Vec<_> = model.entities.iter().collect();
        ents.sort_by_key(|(_, e)| &e.id);

        let header = Header {
            diagram: Diagram::Er,
            view,
            layers: None,
            kinds: if ents.iter().any(|(k, _)| is_visible(&NodeRef::Entity(*k))) {
                vec!["entity"]
            } else {
                Vec::new()
            },
            omitted: Vec::new(),
        };
        let mut out = header.render();
        out.push_str("erDiagram\n");

        for (k, ent) in &ents {
            if !is_visible(&NodeRef::Entity(*k)) {
                continue;
            }
            out.push_str(&format!("  {} {{\n", ent.id));

            for col in &ent.columns {
                let type_str = col_type_str(model, &col.col_type);
                if col.is_pk {
                    out.push_str(&format!("    {} {} PK\n", type_str, col.name));
                } else if model.column_fk(*k, &col.name).is_some() {
                    out.push_str(&format!("    {} {} FK\n", type_str, col.name));
                } else {
                    out.push_str(&format!("    {} {}\n", type_str, col.name));
                }
            }

            out.push_str("  }\n");
        }

        let mut er_rels: Vec<_> = model
            .relations
            .iter()
            .enumerate()
            .filter(|(_, r)| is_relate(&r.kind))
            .collect();
        er_rels.sort_by_key(|(_, r)| (model.node_id(&r.from), model.node_id(&r.to)));

        for &(index, rel) in &er_rels {
            if !is_visible(&rel.from) || !is_visible(&rel.to) {
                continue;
            }
            if let (Some(from_id), Some(to_id)) = (model.node_id(&rel.from), model.node_id(&rel.to))
            {
                let Some(glyph) = crate::er_glyph(model, index) else {
                    continue;
                };
                out.push_str(&format!("  {from_id} {glyph} {to_id} : \"\"\n"));
            }
        }

        Ok(out)
    }
}
