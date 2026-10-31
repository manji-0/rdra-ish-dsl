//! Built-in predicate catalog and argument kind signatures.

/// Legal `(source, target)` kind pairs of `triggers`. The signature of `triggers` is derived
/// from this table, and the pair check in `process_predicate` rejects the rest of the product.
pub const TRIGGER_PAIRS: &[(&str, &str)] = &[
    ("event", "usecase"),
    ("event", "buc"),
    ("event", "extsystem"),
    ("timer", "usecase"),
    ("timer", "buc"),
];

/// Kinds that may be the source of `raises`. The signature of `raises` is derived from it.
pub const RAISE_SOURCES: &[&str] = &["usecase", "extsystem"];

/// `triggers` argument kinds at `position` (0 = source, 1 = target), without duplicates.
fn trigger_kinds(position: usize) -> Vec<&'static str> {
    let mut kinds: Vec<&'static str> = Vec::new();
    for pair in TRIGGER_PAIRS {
        let kind = if position == 0 { pair.0 } else { pair.1 };
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    kinds
}

/// Each predicate's expected argument kind strings per position.
pub fn predicate_signature(pred: &str) -> Option<Vec<Vec<&'static str>>> {
    match pred {
        "performs" => Some(vec![vec!["actor"], vec!["usecase", "buc", "step"]]),
        "reads" | "writes" | "creates" | "updates" | "deletes" => {
            Some(vec![vec!["usecase", "api"], vec!["entity"]])
        }
        "invokes" => Some(vec![vec!["usecase"], vec!["api"]]),
        "request" | "response" | "error_response" => Some(vec![vec!["api"], vec!["dto"]]),
        "applies_to" => Some(vec![
            vec!["nfr", "condition"],
            vec!["usecase", "api", "system"],
        ]),
        "combines" => Some(vec![vec!["condition"], vec!["variation", "_col"]]),
        "holds" => Some(vec![vec!["condition"]]),
        "qualifies" => Some(vec![vec!["nfr", "constraint"], vec!["quality"]]),
        "constrains" => Some(vec![
            vec!["constraint"],
            vec!["usecase", "api", "system", "entity", "dto"],
        ]),
        "owns" => Some(vec![vec!["system"], vec!["entity"]]),
        "exposes" => Some(vec![vec!["system"], vec!["usecase", "event", "entity"]]),
        "shares" => Some(vec![vec!["system"], vec!["entity"]]),
        "displays" => Some(vec![vec!["usecase"], vec!["screen"]]),
        "shows" => Some(vec![vec!["screen"], vec!["entity"]]),
        "maps_field" => Some(vec![vec!["field"], vec!["entity"], vec!["_col"]]),
        "raises" => Some(vec![RAISE_SOURCES.to_vec(), vec!["event"]]),
        "triggers" => Some(vec![trigger_kinds(0), trigger_kinds(1)]),
        "outbox" => Some(vec![vec!["event"]]),
        "contains" => Some(vec![
            vec!["buc", "system", "flow", "screen"],
            vec!["usecase", "api", "flow", "step", "field"],
        ]),
        "precedes" | "branches" | "excepts" | "repeats" => Some(vec![vec!["step"], vec!["step"]]),
        "covers" => Some(vec![vec!["step"], vec!["usecase", "api", "event", "timer"]]),
        "compensates" => Some(vec![vec!["usecase"], vec!["usecase"]]),
        "coordinates" => Some(vec![vec!["usecase"], vec!["entity"], vec!["entity"]]),
        "belongs" => Some(vec![vec!["buc"], vec!["business"]]),
        "has_permission" => Some(vec![vec!["actor"], vec!["permission"]]),
        "requires_permission" => Some(vec![vec!["usecase", "api"], vec!["permission"]]),
        "requires_medium" => Some(vec![vec!["usecase", "api"], vec!["medium"]]),
        "motivates" => Some(vec![vec!["requirement"], vec!["buc"]]),
        "achieves" => Some(vec![vec!["requirement"], vec!["goal"]]),
        "decides" => Some(vec![
            vec!["adr"],
            vec![
                "buc",
                "usecase",
                "api",
                "system",
                "entity",
                "requirement",
                "nfr",
                "constraint",
                "dto",
            ],
        ]),
        "transitions" => Some(vec![vec!["_col"], vec!["usecase"], vec!["_val"]]),
        "after" => Some(vec![vec!["usecase"]]),
        "relate" => Some(vec![vec!["entity"], vec!["entity"], vec!["_card"]]),
        "sets" => Some(vec![vec!["usecase"], vec!["entity"]]),
        "forbidden" | "invariant" | "required" | "exclusive" => Some(vec![vec!["entity"]]),
        "when" => Some(vec![]),
        _ => None,
    }
}

/// Chain methods (`.name(...)`) each predicate's processor reads. Any other
/// chain name is rejected as `UnknownChainMethod`.
pub fn predicate_chain_methods(pred: &str) -> &'static [&'static str] {
    match pred {
        "belongs" => &["by"],
        "relate" => &["via", "role", "key", "optional", "on_delete", "on_update"],
        "forbidden" => &["along", "by", "label"],
        "invariant" => &["when", "then", "has", "none", "along", "by", "label"],
        "required" | "exclusive" => &["label"],
        "when" => &["has", "none", "label"],
        "holds" => &["has", "none"],
        "after" => &["assert", "label"],
        "exposes" => &["append", "upsert", "caller"],
        _ => &[],
    }
}

/// Hint text for an unknown chain method `name` on `pred`.
pub fn unknown_chain_method_hint(pred: &str, name: &str) -> String {
    let allowed = predicate_chain_methods(pred);
    let mut hint = if allowed.is_empty() {
        format!("`{pred}` takes no chain methods")
    } else {
        let list: Vec<String> = allowed.iter().map(|m| format!("`.{m}(...)`")).collect();
        format!("`{pred}` accepts {}", list.join(", "))
    };
    if pred == "belongs" && name == "where" {
        hint.push_str("; `.where(...)` was removed together with `location`, so delete it");
    }
    if pred == "belongs" && name == "when" {
        hint.push_str(
            "; `.when(...)` was removed with `timing`; declare a `timer` and connect it with `triggers(Timer, UseCase)`",
        );
    }
    hint
}

/// Built-in predicate names accepted by the semantic model.
pub const KNOWN_PREDICATES: &[&str] = &[
    "performs",
    "reads",
    "writes",
    "creates",
    "updates",
    "deletes",
    "invokes",
    "request",
    "response",
    "error_response",
    "applies_to",
    "combines",
    "holds",
    "qualifies",
    "constrains",
    "owns",
    "exposes",
    "shares",
    "displays",
    "shows",
    "maps_field",
    "raises",
    "triggers",
    "outbox",
    "contains",
    "precedes",
    "branches",
    "excepts",
    "repeats",
    "covers",
    "compensates",
    "coordinates",
    "belongs",
    "has_permission",
    "requires_permission",
    "requires_medium",
    "motivates",
    "achieves",
    "decides",
    "transitions",
    "after",
    "relate",
    "sets",
    "forbidden",
    "invariant",
    "required",
    "exclusive",
    "when",
];
