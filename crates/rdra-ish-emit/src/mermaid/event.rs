//! The event diagram (`--kind event`).

use crate::diagram_common::{class_defs, contract_id, render_event_flows, Header};
use crate::{EmitError, Emitter, View};
use rdra_ish_core::model::{NodeRef, SemanticModel};

/// The system-to-system event links: `raises` into each event and `triggers` out of it, and
/// each timer's `triggers`. The transitions a triggered use case drives are the state
/// diagram's.
pub struct EventMermaidEmitter;

impl Emitter for EventMermaidEmitter {
    fn emit(&self, model: &SemanticModel, view: &View) -> Result<String, EmitError> {
        let mut body = String::new();
        let declared = render_event_flows(&mut body, model, &view.scope);
        body.push_str(&class_defs(&declared.kinds));
        let drops_transitions = model.state_transitions.iter().any(|t| {
            contract_id(model, &NodeRef::UseCase(t.usecase))
                .is_some_and(|id| declared.ids.contains(&id))
        });
        let header = Header {
            diagram: "event",
            view,
            layers: None,
            kinds: declared.kinds,
            omitted: if drops_transitions {
                vec!["state transitions".to_string()]
            } else {
                Vec::new()
            },
        };
        Ok(format!("{}flowchart LR\n{body}", header.render()))
    }
}
