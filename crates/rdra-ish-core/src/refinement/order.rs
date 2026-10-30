//! The stage order: the only place it lives. Numbers and doc anchors derive from it.
//!
//! Reordering is editing `ORDER`, blessing the generated doc blocks, and moving the
//! hand-written prose sections of `docs/incremental-modeling.md` to match.

use super::{spec, StageId};

pub(crate) const ORDER: &[StageId] = &[
    StageId::Scope,
    StageId::Skeleton,
    StageId::DataTouchpoints,
    StageId::InteractionBoundary,
    StageId::EntityStructure,
    StageId::Lifecycle,
    StageId::LocalGuardrails,
    StageId::LocalObligations,
    StageId::AdvancedRules,
];

impl StageId {
    /// `"0"`, `"5"`, `"6A"`. Consecutive stages that share a concern form one numbered
    /// group; a group of more than one stage adds a letter per member.
    pub fn number(self) -> String {
        let mut group = 0;
        let mut member = 0;
        let mut size = 0;
        let mut previous: Option<&str> = None;
        for (i, &id) in ORDER.iter().enumerate() {
            let concern = spec(id).concern;
            if i > 0 && previous != Some(concern) {
                group += 1;
                member = 0;
            }
            previous = Some(concern);
            if id == self {
                size = ORDER[i..]
                    .iter()
                    .take_while(|&&other| spec(other).concern == concern)
                    .count()
                    + member;
                break;
            }
            member += 1;
        }
        if size > 1 {
            format!("{group}{}", (b'A' + member as u8) as char)
        } else {
            group.to_string()
        }
    }

    /// The anchor of the stage's section in `docs/incremental-modeling.md`: GitHub's slug
    /// of the heading `Stage {number}: {name}`.
    pub fn doc_anchor(self) -> String {
        let text = format!("stage {} {}", self.number(), spec(self).name);
        text.to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect()
    }
}
