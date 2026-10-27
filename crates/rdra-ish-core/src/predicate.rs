//! Built-in predicate catalog and argument kind signatures.

/// Each predicate's expected argument kind strings per position.
pub fn predicate_signature(pred: &str) -> Option<Vec<Vec<&'static str>>> {
    match pred {
        "performs" => Some(vec![vec!["actor"], vec!["usecase", "buc"]]),
        "uses" => Some(vec![vec!["actor"], vec!["extsystem"]]),
        "reads" | "writes" | "creates" | "updates" | "deletes" => {
            Some(vec![vec!["usecase", "api"], vec!["entity"]])
        }
        "invokes" => Some(vec![vec!["usecase"], vec!["api"]]),
        "request" | "response" | "error_response" => Some(vec![vec!["api"], vec!["dto"]]),
        "applies_to" => Some(vec![vec!["nfr"], vec!["usecase", "api", "system"]]),
        "qualifies" => Some(vec![vec!["nfr", "constraint"], vec!["quality"]]),
        "constrains" => Some(vec![
            vec!["constraint"],
            vec!["usecase", "api", "system", "entity", "dto"],
        ]),
        "owns" => Some(vec![vec!["system"], vec!["entity"]]),
        "displays" => Some(vec![vec!["usecase"], vec!["screen"]]),
        "shows" => Some(vec![vec!["screen"], vec!["entity"]]),
        "maps_field" => Some(vec![vec!["field"], vec!["entity"], vec!["_col"]]),
        "raises" => Some(vec![vec!["usecase"], vec!["event"]]),
        "triggers" => Some(vec![vec!["event"], vec!["usecase", "buc"]]),
        "outbox" => Some(vec![vec!["event"]]),
        "contains" => Some(vec![
            vec!["buc", "system", "flow", "screen"],
            vec!["usecase", "api", "flow", "step", "field"],
        ]),
        "precedes" | "branches" | "excepts" | "repeats" => Some(vec![vec!["step"], vec!["step"]]),
        "covers" => Some(vec![vec!["step"], vec!["usecase", "api", "event"]]),
        "compensates" => Some(vec![vec!["usecase"], vec!["usecase"]]),
        "coordinates" => Some(vec![vec!["usecase"], vec!["entity"], vec!["entity"]]),
        "belongs" => Some(vec![vec!["buc"], vec!["business"]]),
        "has_permission" => Some(vec![vec!["actor"], vec!["permission"]]),
        "requires_permission" => Some(vec![vec!["usecase", "api"], vec!["permission"]]),
        "requires_medium" => Some(vec![vec!["usecase", "api"], vec!["medium"]]),
        "motivates" => Some(vec![vec!["requirement"], vec!["buc"]]),
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
        "belongs" => &["when", "by"],
        "relate" => &["via", "optional", "on_delete", "on_update"],
        "forbidden" => &["along"],
        "invariant" => &["when", "then", "has", "none", "along"],
        "when" => &["has", "none"],
        "after" => &["assert"],
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
    hint
}

/// Built-in predicate names accepted by the semantic model.
pub const KNOWN_PREDICATES: &[&str] = &[
    "performs",
    "uses",
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
    "qualifies",
    "constrains",
    "owns",
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
