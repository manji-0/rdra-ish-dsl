//! Built-in predicate catalog and argument kind signatures.

use crate::model::RelKind;

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
fn trigger_kinds(position: usize) -> Vec<ArgKind> {
    let mut kinds: Vec<ArgKind> = Vec::new();
    for pair in TRIGGER_PAIRS {
        let kind = ArgKind::Node(if position == 0 { pair.0 } else { pair.1 });
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    kinds
}

macro_rules! predicates {
    ($($variant:ident = $name:literal,)*) => {
        /// A built-in predicate.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Predicate {
            $($variant,)*
        }

        impl Predicate {
            /// Every predicate, in catalog order.
            pub const ALL: &'static [Predicate] = &[$(Predicate::$variant,)*];

            /// The name a predicate call spells.
            pub fn name(self) -> &'static str {
                match self {
                    $(Predicate::$variant => $name,)*
                }
            }
        }

        /// Built-in predicate names accepted by the semantic model.
        pub const KNOWN_PREDICATES: &[&str] = &[$($name,)*];
    };
}

predicates! {
    Performs = "performs",
    Reads = "reads",
    Creates = "creates",
    Updates = "updates",
    Deletes = "deletes",
    Invokes = "invokes",
    Request = "request",
    Response = "response",
    ErrorResponse = "error_response",
    AppliesTo = "applies_to",
    Combines = "combines",
    Holds = "holds",
    Qualifies = "qualifies",
    Constrains = "constrains",
    Owns = "owns",
    Exposes = "exposes",
    Shares = "shares",
    Displays = "displays",
    Shows = "shows",
    Navigates = "navigates",
    MapsField = "maps_field",
    Raises = "raises",
    Triggers = "triggers",
    Outbox = "outbox",
    Contains = "contains",
    Precedes = "precedes",
    Branches = "branches",
    Excepts = "excepts",
    Repeats = "repeats",
    Covers = "covers",
    Compensates = "compensates",
    Coordinates = "coordinates",
    Belongs = "belongs",
    HasPermission = "has_permission",
    RequiresPermission = "requires_permission",
    RequiresMedium = "requires_medium",
    Motivates = "motivates",
    Achieves = "achieves",
    Decides = "decides",
    Transitions = "transitions",
    After = "after",
    Relate = "relate",
    Sets = "sets",
    Drives = "drives",
    Links = "links",
    Relinks = "relinks",
    Unlinks = "unlinks",
    Chooses = "chooses",
    Forbidden = "forbidden",
    Invariant = "invariant",
    Required = "required",
    Exclusive = "exclusive",
}

/// What one argument of a predicate may be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgKind {
    /// A declaration of this kind, resolved by name.
    Node(&'static str),
    /// `Entity.column`, read by the predicate's processor.
    Column,
    /// A column value, read by the predicate's processor.
    Value,
    /// A cardinality, read by the predicate's processor.
    Card,
}

impl ArgKind {
    /// The declaration kind, when the argument names a declaration.
    pub fn node(self) -> Option<&'static str> {
        match self {
            ArgKind::Node(kind) => Some(kind),
            ArgKind::Column | ArgKind::Value | ArgKind::Card => None,
        }
    }
}

/// How many arguments a predicate takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arity {
    Exact(usize),
    AtLeast(usize),
    OneOf(usize, usize),
}

impl Arity {
    pub fn accepts(self, got: usize) -> bool {
        match self {
            Arity::Exact(n) => got == n,
            Arity::AtLeast(n) => got >= n,
            Arity::OneOf(a, b) => got == a || got == b,
        }
    }

    /// The arity as `WrongArity.expected` spells it.
    pub fn expected(self) -> String {
        match self {
            Arity::Exact(n) => n.to_string(),
            Arity::AtLeast(n) => format!("at least {n}"),
            Arity::OneOf(a, b) => format!("{a} or {b}"),
        }
    }
}

fn nodes(kinds: &[&'static str]) -> Vec<ArgKind> {
    kinds.iter().copied().map(ArgKind::Node).collect()
}

impl Predicate {
    pub fn parse(name: &str) -> Option<Predicate> {
        Predicate::ALL.iter().copied().find(|p| p.name() == name)
    }

    /// The kinds each argument position takes. Arguments past the signature, which only a
    /// predicate whose arity exceeds it has, are conditions its processor reads.
    pub fn signature(self) -> Vec<Vec<ArgKind>> {
        use ArgKind::*;
        use Predicate::*;
        match self {
            Performs => vec![nodes(&["actor"]), nodes(&["usecase", "buc", "step"])],
            Reads | Creates | Updates | Deletes => {
                vec![nodes(&["usecase", "api"]), nodes(&["entity"])]
            }
            Invokes => vec![nodes(&["usecase"]), nodes(&["api"])],
            Request | Response | ErrorResponse => vec![nodes(&["api"]), nodes(&["dto"])],
            AppliesTo => vec![
                nodes(&["nfr", "condition"]),
                nodes(&["usecase", "api", "system"]),
            ],
            Combines => vec![nodes(&["condition"]), vec![Node("variation"), Column]],
            Holds => vec![nodes(&["condition"])],
            Qualifies => vec![nodes(&["nfr", "constraint"]), nodes(&["quality"])],
            Constrains => vec![
                nodes(&["constraint"]),
                nodes(&["usecase", "api", "system", "entity", "dto"]),
            ],
            Owns => vec![nodes(&["system"]), nodes(&["entity"])],
            Exposes => vec![nodes(&["system"]), nodes(&["usecase", "event", "entity"])],
            Shares => vec![nodes(&["system"]), nodes(&["entity"])],
            Displays => vec![nodes(&["usecase"]), nodes(&["screen"])],
            Shows => vec![nodes(&["screen"]), nodes(&["entity"])],
            Navigates => vec![nodes(&["screen"]), nodes(&["screen"])],
            MapsField => vec![nodes(&["field"]), vec![Column]],
            Raises => vec![nodes(RAISE_SOURCES), nodes(&["event"])],
            Triggers => vec![trigger_kinds(0), trigger_kinds(1)],
            Outbox => vec![nodes(&["event"])],
            Contains => vec![
                nodes(&["buc", "system", "flow", "screen"]),
                nodes(&["usecase", "api", "flow", "step", "field"]),
            ],
            Precedes | Branches | Excepts | Repeats => vec![nodes(&["step"]), nodes(&["step"])],
            Covers => vec![
                nodes(&["step"]),
                nodes(&["usecase", "api", "event", "timer"]),
            ],
            Compensates => vec![nodes(&["usecase"]), nodes(&["usecase"])],
            Coordinates => vec![nodes(&["usecase"]), nodes(&["entity"]), nodes(&["entity"])],
            Belongs => vec![nodes(&["buc"]), nodes(&["business"])],
            HasPermission => vec![nodes(&["actor"]), nodes(&["permission"])],
            RequiresPermission => vec![nodes(&["usecase", "api"]), nodes(&["permission"])],
            RequiresMedium => vec![nodes(&["usecase", "api"]), nodes(&["medium"])],
            Motivates => vec![nodes(&["requirement"]), nodes(&["buc"])],
            Achieves => vec![nodes(&["requirement"]), nodes(&["goal"])],
            Decides => vec![
                nodes(&["adr"]),
                nodes(&[
                    "buc",
                    "usecase",
                    "api",
                    "system",
                    "entity",
                    "requirement",
                    "nfr",
                    "constraint",
                    "dto",
                ]),
            ],
            Transitions | Chooses => vec![nodes(&["usecase"]), vec![Column], vec![Value]],
            After => vec![nodes(&["usecase"])],
            Relate => vec![nodes(&["entity"]), nodes(&["entity"]), vec![Card]],
            Sets | Drives => vec![nodes(&["usecase"])],
            Links | Relinks | Unlinks => {
                vec![nodes(&["usecase"]), nodes(&["entity"]), nodes(&["entity"])]
            }
            Forbidden | Invariant | Required | Exclusive => vec![nodes(&["entity"])],
        }
    }

    pub fn arity(self) -> Arity {
        use Predicate::*;
        match self {
            Sets => Arity::Exact(2),
            Drives => Arity::Exact(3),
            After | Forbidden | Invariant | Required | Exclusive => Arity::AtLeast(1),
            Holds => Arity::AtLeast(2),
            Chooses => Arity::OneOf(2, 3),
            _ => Arity::Exact(self.signature().len()),
        }
    }

    /// Chain methods (`.name(...)`) the predicate's processor reads. Any other chain name is
    /// rejected as `UnknownChainMethod`.
    pub fn chain_methods(self) -> &'static [&'static str] {
        use Predicate::*;
        match self {
            Belongs => &["by"],
            Relate => &[
                "via",
                "role",
                "key",
                "references",
                "optional",
                "on_delete",
                "on_update",
            ],
            Forbidden => &["when", "along", "role", "label", "global", "lemma"],
            Invariant => &[
                "when", "then", "has", "none", "along", "role", "label", "global", "lemma",
            ],
            Required | Exclusive => &["label"],
            Holds => &["has", "none", "along", "role"],
            Sets => &["role", "when"],
            Links | Relinks | Unlinks => &["role"],
            After => &["assert", "label"],
            Exposes => &["append", "upsert", "caller"],
            Navigates => &["via"],
            Transitions | Chooses => &["when"],
            Creates | Updates | Deletes => &["apart"],
            Triggers => &["counted"],
            Reads => &["by", "order", "page", "count", "sum"],
            _ => &[],
        }
    }

    /// The relation a plain relation predicate records.
    pub fn rel_kind(self) -> Option<RelKind> {
        use Predicate::*;
        let kind = match self {
            Performs => RelKind::Performs,
            Reads => RelKind::Reads,
            Creates => RelKind::Creates,
            Updates => RelKind::Updates,
            Deletes => RelKind::Deletes,
            Displays => RelKind::Displays,
            Shows => RelKind::Shows,
            Navigates => RelKind::Navigates,
            Raises => RelKind::Raises,
            Triggers => RelKind::Triggers,
            Contains => RelKind::Contains,
            Belongs => RelKind::Belongs,
            HasPermission => RelKind::HasPermission,
            RequiresPermission => RelKind::RequiresPermission,
            RequiresMedium => RelKind::RequiresMedium,
            Motivates => RelKind::Motivates,
            Achieves => RelKind::Achieves,
            Decides => RelKind::Decides,
            Invokes => RelKind::Invokes,
            Precedes => RelKind::Precedes,
            Branches => RelKind::Branches,
            Excepts => RelKind::Excepts,
            Repeats => RelKind::Repeats,
            Covers => RelKind::Covers,
            Compensates => RelKind::Compensates,
            Request => RelKind::Request,
            Response => RelKind::Response,
            ErrorResponse => RelKind::ErrorResponse,
            AppliesTo => RelKind::AppliesTo,
            Qualifies => RelKind::Qualifies,
            Constrains => RelKind::Constrains,
            MapsField => RelKind::MapsField,
            Owns => RelKind::Owns,
            _ => return None,
        };
        Some(kind)
    }
}

/// Hint text for an unknown chain method `name` on `pred`.
pub fn unknown_chain_method_hint(pred: Predicate, name: &str) -> String {
    let allowed = pred.chain_methods();
    let pred_name = pred.name();
    let mut hint = if allowed.is_empty() {
        format!("`{pred_name}` takes no chain methods")
    } else {
        let list: Vec<String> = allowed.iter().map(|m| format!("`.{m}(...)`")).collect();
        format!("`{pred_name}` accepts {}", list.join(", "))
    };
    if pred == Predicate::Belongs && name == "where" {
        hint.push_str("; `.where(...)` was removed together with `location`, so delete it");
    }
    if pred == Predicate::Belongs && name == "when" {
        hint.push_str(
            "; `.when(...)` was removed with `timing`; declare a `timer` and connect it with `triggers(Timer, UseCase)`",
        );
    }
    hint
}
