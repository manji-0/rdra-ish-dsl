//! The Rust printer, in the shapes of docs/design/algebraic-codegen.md, "Generated code
//! style": opaque ids and scalars made by `From` / `TryFrom` / `FromStr`, value sets with
//! `FromStr`, sealed state types told apart by `kind()`, flat `<Entity>Row` types with
//! `TryFrom<Row>`, and transitions that return `Result<_, <UseCase>Error>`. No generated code
//! panics. Standard types are written by path, so a model type named `String`, `Option` or
//! `Error` shadows nothing.

use super::ir::{
    DomainModel, Field, FieldType, FnDecl, InputType, RecordKind, Scalar, UseCaseErrorDecl,
};
use super::{header_lines, obligation_lines, one_line, string_literal, Options};

pub(super) const OPTION: &str = "::core::option::Option";
pub(super) const STRING: &str = "::std::string::String";
pub(super) const RESULT: &str = "::core::result::Result";
const FMT: &str = "::core::fmt";

/// The most arguments a function takes before clippy's `too_many_arguments` would flag it.
const MANY_ARGUMENTS: usize = 7;

pub(super) fn print(
    domain: &DomainModel,
    options: &Options,
    traces: Option<&super::conformance::Traces>,
) -> String {
    let mut out = String::new();
    for line in header_lines(options) {
        out.push_str(&comment(&line));
    }
    out.push_str("//\n");
    for line in obligation_lines(domain) {
        out.push_str(&comment(&line));
    }
    for line in super::conformance_lines(domain, traces, "conform", "ConformancePort") {
        out.push_str(&comment(&line));
    }
    out.push('\n');
    out.push_str("//! The data layer and the use cases of the model, as `rdra-ish codegen` generates them.\n\n");

    print_boundary(&mut out);
    let serde = options.serde;

    out.push_str("// ── Scalars: replace with the project's own types if it has them ──\n\n");
    for scalar in Scalar::PRELUDE {
        print_scalar(scalar, serde, &mut out);
    }

    if !domain.ids.is_empty() {
        out.push_str("// ── Ids ──\n\n");
    }
    for id in &domain.ids {
        print_id(id, serde, &mut out);
    }

    if !domain.enums.is_empty() {
        out.push_str("// ── Value sets ──\n\n");
    }
    for decl in &domain.enums {
        print_enum(decl, serde, &mut out);
    }

    for kind in [RecordKind::Entity, RecordKind::Dto] {
        let records: Vec<_> = domain.records.iter().filter(|r| r.kind == kind).collect();
        if records.is_empty() {
            continue;
        }
        out.push_str(match kind {
            RecordKind::Entity => "// ── Entities ──\n\n",
            RecordKind::Dto => "// ── DTOs ──\n\n",
        });
        for record in records {
            let mut lines = record.doc.clone();
            lines.push(match kind {
                RecordKind::Entity => format!(
                    "A record of entity {}: it has no states, so it is its own row, and the types of its fields are what it keeps.",
                    record.id
                ),
                RecordKind::Dto => format!("The fields of DTO {}.", record.id),
            });
            out.push_str(&doc(&lines, ""));
            out.push_str(&derive("Debug, Clone, PartialEq, Eq", serde));
            out.push_str(&allow_names(&record.fields));
            out.push_str(&format!("pub struct {} {{\n", record.name));
            for field in &record.fields {
                out.push_str(&field_doc(field, "    "));
                out.push_str(&serde_rename(&field.wire, serde));
                out.push_str(&format!(
                    "    pub {}: {},\n",
                    ident(&field.wire),
                    typed(field)
                ));
            }
            out.push_str("}\n\n");
        }
    }
    if !domain.sums.is_empty() {
        out.push_str("// ── Entities with states ──\n\n");
    }
    for sum in &domain.sums {
        print_sum(sum, serde, &mut out);
    }
    print_functions(domain, serde, &mut out);
    print_validators(domain, &mut out);
    print_rows(domain, serde, &mut out);
    print_dispatchers(domain, &mut out);
    super::writes::rust::print(domain, &mut out);
    super::rust_port::print_port(domain, &mut out);
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

/// `InvalidValue` and the shape checks of the prelude scalars: every parser of the output
/// refuses text through them.
fn print_boundary(out: &mut String) {
    out.push_str(&format!(
        r#"// ── Boundary: what a parser of this module refuses ──

/// Text that is no value of the type a parser makes: an id, a scalar, or a value set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidValue {{
    /// The type the text was parsed as.
    pub type_name: &'static str,
    /// The text.
    pub value: {STRING},
}}

impl {FMT}::Display for InvalidValue {{
    fn fmt(&self, f: &mut {FMT}::Formatter<'_>) -> {FMT}::Result {{
        write!(f, "{{:?}} is no value of {{}}", self.value, self.type_name)
    }}
}}

impl ::std::error::Error for InvalidValue {{}}

/// Whether `text` is all ASCII digits, and not empty.
fn rdra_is_digits(text: &str) -> bool {{
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}}

/// Whether `text` is groups of ASCII digits of the lengths `widths`, split by `sep`.
fn rdra_is_groups(text: &str, sep: char, widths: &[usize]) -> bool {{
    let parts: ::std::vec::Vec<&str> = text.split(sep).collect();
    parts.len() == widths.len()
        && parts.iter().zip(widths).all(|(p, n)| p.len() == *n && rdra_is_digits(p))
}}

/// Whether `text` is decimal text: an optional `-`, digits, and optionally `.` and digits.
fn rdra_is_decimal(text: &str) -> bool {{
    let digits = text.strip_prefix('-').unwrap_or(text);
    match digits.split_once('.') {{
        {OPTION}::Some((whole, fraction)) => rdra_is_digits(whole) && rdra_is_digits(fraction),
        {OPTION}::None => rdra_is_digits(digits),
    }}
}}

/// Whether `text` has the shape of an ISO 8601 date, `YYYY-MM-DD`; the calendar is not checked.
fn rdra_is_date(text: &str) -> bool {{
    rdra_is_groups(text, '-', &[4, 2, 2])
}}

/// Whether `text` has the shape of an ISO 8601 date and time with an offset:
/// `YYYY-MM-DDTHH:MM:SS`, optionally `.` and digits, then `Z` or `+HH:MM` / `-HH:MM`.
fn rdra_is_date_time(text: &str) -> bool {{
    let {OPTION}::Some((date, time)) = text.split_once('T') else {{
        return false;
    }};
    let (clock, zone_ok) = match time.strip_suffix('Z') {{
        {OPTION}::Some(clock) => (clock, true),
        {OPTION}::None => match time.rfind(['+', '-']) {{
            {OPTION}::Some(at) => (&time[..at], rdra_is_groups(&time[at + 1..], ':', &[2, 2])),
            {OPTION}::None => return false,
        }},
    }};
    let clock_ok = match clock.split_once('.') {{
        {OPTION}::Some((whole, fraction)) => {{
            rdra_is_groups(whole, ':', &[2, 2, 2]) && rdra_is_digits(fraction)
        }}
        {OPTION}::None => rdra_is_groups(clock, ':', &[2, 2, 2]),
    }};
    rdra_is_date(date) && clock_ok && zone_ok
}}

"#
    ));
}

fn print_scalar(scalar: Scalar, serde: bool, out: &mut String) {
    let (check, shape) = match scalar {
        Scalar::Money | Scalar::Decimal => (
            "rdra_is_decimal",
            "decimal text (an optional `-`, digits, and optionally `.` and digits)",
        ),
        Scalar::Date => ("rdra_is_date", "an ISO 8601 date, `YYYY-MM-DD`"),
        _ => (
            "rdra_is_date_time",
            "an ISO 8601 date and time with `Z` or an offset",
        ),
    };
    let name = scalar.name();
    out.push_str(&doc(
        &[
            scalar.default_doc().to_string(),
            format!("Made only by `TryFrom<String>` and `FromStr`, which take {shape} and refuse other text with [`InvalidValue`]."),
        ],
        "",
    ));
    out.push_str(&derive(
        "Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord",
        serde,
    ));
    if serde {
        // Deserialized through `TryFrom<String>`, so the wire is parsed as `FromStr` parses it.
        out.push_str(&format!("#[serde(try_from = \"{STRING}\")]\n"));
    }
    out.push_str(&format!("pub struct {name}({STRING});\n\n"));
    out.push_str(&format!(
        r#"impl {name} {{
    /// The text it holds.
    pub fn as_str(&self) -> &str {{
        &self.0
    }}
}}

impl ::core::convert::TryFrom<{STRING}> for {name} {{
    type Error = InvalidValue;

    /// The value of `text`, or [`InvalidValue`] when it is not {shape}.
    fn try_from(text: {STRING}) -> {RESULT}<Self, InvalidValue> {{
        if {check}(&text) {{
            {RESULT}::Ok(Self(text))
        }} else {{
            {RESULT}::Err(InvalidValue {{ type_name: "{name}", value: text }})
        }}
    }}
}}

impl ::core::str::FromStr for {name} {{
    type Err = InvalidValue;

    fn from_str(text: &str) -> {RESULT}<Self, InvalidValue> {{
        <Self as ::core::convert::TryFrom<{STRING}>>::try_from(text.to_string())
    }}
}}

impl {FMT}::Display for {name} {{
    fn fmt(&self, f: &mut {FMT}::Formatter<'_>) -> {FMT}::Result {{
        f.write_str(&self.0)
    }}
}}

"#
    ));
}

fn print_id(id: &super::ir::IdDecl, serde: bool, out: &mut String) {
    let name = &id.name;
    let repr = scalar(id.repr);
    let copy = matches!(id.repr, Scalar::Int | Scalar::Bool);
    out.push_str(&doc(
        &[
            format!("The id of entity {}.", id.entity),
            format!(
                "Any `{}` is one: made by `From` (so `TryFrom`) and `FromStr`, read by `get`.",
                repr.trim_start_matches("::std::string::")
            ),
        ],
        "",
    ));
    let copy_derive = if copy { "Copy, " } else { "" };
    out.push_str(&derive(
        &format!("Debug, Clone, {copy_derive}PartialEq, Eq, Hash, PartialOrd, Ord"),
        serde,
    ));
    if serde {
        out.push_str("#[serde(transparent)]\n");
    }
    out.push_str(&format!("pub struct {name}({repr});\n\n"));
    let get = if copy {
        format!("    /// The value it holds.\n    pub fn get(self) -> {repr} {{\n        self.0\n    }}\n")
    } else {
        format!("    /// The value it holds.\n    pub fn get(&self) -> &{repr} {{\n        &self.0\n    }}\n")
    };
    out.push_str(&format!("impl {name} {{\n{get}}}\n\n"));
    out.push_str(&format!(
        "impl ::core::convert::From<{repr}> for {name} {{\n    fn from(value: {repr}) -> Self {{\n        Self(value)\n    }}\n}}\n\n"
    ));
    let parse = match id.repr {
        Scalar::Int => format!("text.parse::<i64>().map(Self).map_err(|_| InvalidValue {{ type_name: \"{name}\", value: text.to_string() }})"),
        Scalar::Bool => format!("text.parse::<bool>().map(Self).map_err(|_| InvalidValue {{ type_name: \"{name}\", value: text.to_string() }})"),
        Scalar::String => format!("{RESULT}::Ok(Self(text.to_string()))"),
        other => format!(
            "text.parse::<{}>().map(Self).map_err(|e| InvalidValue {{ type_name: \"{name}\", value: e.value }})",
            other.name()
        ),
    };
    out.push_str(&format!(
        "impl ::core::str::FromStr for {name} {{\n    type Err = InvalidValue;\n\n    fn from_str(text: &str) -> {RESULT}<Self, InvalidValue> {{\n        {parse}\n    }}\n}}\n\n"
    ));
    out.push_str(&format!(
        "impl {FMT}::Display for {name} {{\n    fn fmt(&self, f: &mut {FMT}::Formatter<'_>) -> {FMT}::Result {{\n        {FMT}::Display::fmt(&self.0, f)\n    }}\n}}\n\n"
    ));
}

fn print_enum(decl: &super::ir::EnumDecl, serde: bool, out: &mut String) {
    let mut lines = decl.doc.clone();
    lines.push(
        "Parsed from its wire spelling by `FromStr`, printed by `as_str` and `Display`."
            .to_string(),
    );
    out.push_str(&doc(&lines, ""));
    out.push_str(&derive(
        "Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord",
        serde,
    ));
    if decl
        .values
        .iter()
        .any(|v| !is_upper_camel(&variant_ident(&v.ident)))
    {
        out.push_str("#[allow(non_camel_case_types)] // the model's spelling of a value\n");
    }
    out.push_str(&format!("pub enum {} {{\n", decl.name));
    for value in &decl.values {
        let mut lines = Vec::new();
        if let Some(label) = &value.label {
            lines.push(label.clone());
        }
        lines.push(format!("`{}`", value.wire));
        out.push_str(&doc(&lines, "    "));
        if serde {
            out.push_str(&format!(
                "    #[serde(rename = {})]\n",
                string_literal(&value.wire)
            ));
        }
        out.push_str(&format!("    {},\n", variant_ident(&value.ident)));
    }
    out.push_str("}\n\n");
    out.push_str(&format!("impl {} {{\n", decl.name));
    out.push_str("    /// Every value, in declaration order.\n");
    out.push_str(&format!(
        "    pub const ALL: [Self; {}] = [{}];\n\n",
        decl.values.len(),
        decl.values
            .iter()
            .map(|v| format!("Self::{}", variant_ident(&v.ident)))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.push_str("    /// The value as the model spells it.\n");
    out.push_str("    pub fn as_str(self) -> &'static str {\n        match self {\n");
    for value in &decl.values {
        out.push_str(&format!(
            "            Self::{} => {},\n",
            variant_ident(&value.ident),
            string_literal(&value.wire)
        ));
    }
    out.push_str("        }\n    }\n}\n\n");
    out.push_str(&format!(
        "impl ::core::str::FromStr for {name} {{\n    type Err = InvalidValue;\n\n    /// The value the model spells `text`, or [`InvalidValue`].\n    fn from_str(text: &str) -> {RESULT}<Self, InvalidValue> {{\n        Self::ALL\n            .into_iter()\n            .find(|v| v.as_str() == text)\n            .ok_or_else(|| InvalidValue {{ type_name: {lit}, value: text.to_string() }})\n    }}\n}}\n\n",
        name = decl.name,
        lit = string_literal(&decl.name)
    ));
    out.push_str(&format!(
        "impl {FMT}::Display for {} {{\n    fn fmt(&self, f: &mut {FMT}::Formatter<'_>) -> {FMT}::Result {{\n        f.write_str(self.as_str())\n    }}\n}}\n\n",
        decl.name
    ));
}

fn print_sum(sum: &super::ir::SumDecl, serde: bool, out: &mut String) {
    for variant in &sum.variants {
        let mut lines = variant.doc.clone();
        for field in &variant.always_null {
            lines.push(format!(
                "`{}`: {}",
                field.wire,
                field.doc.last().cloned().unwrap_or_default()
            ));
        }
        lines.push(
            "Sealed: only a transition function and `from_row` make one. Its fields are private, and read by the methods of the same name.".to_string(),
        );
        out.push_str(&doc(&lines, ""));
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&allow_names(&variant.fields));
        out.push_str(&format!("pub struct {} {{\n", variant.name));
        for field in &variant.fields {
            out.push_str(&doc(&field.doc, "    "));
            out.push_str(&format!("    {}: {},\n", ident(&field.name), typed(field)));
        }
        out.push_str("}\n\n");
        if !variant.fields.is_empty() {
            out.push_str(&allow_names(&variant.fields));
            out.push_str(&allow_accessor_names(&variant.fields));
            out.push_str(&format!("impl {} {{\n", variant.name));
            for field in &variant.fields {
                let ty = field_type(&field.ty);
                let (ret, body) = if field.nullable {
                    (
                        format!("{OPTION}<&{ty}>"),
                        format!("self.{}.as_ref()", ident(&field.name)),
                    )
                } else {
                    (format!("&{ty}"), format!("&self.{}", ident(&field.name)))
                };
                out.push_str(&format!(
                    "    /// The column `{}`.\n    pub fn {}(&self) -> {ret} {{\n        {body}\n    }}\n",
                    field.wire,
                    ident(&field.name)
                ));
            }
            out.push_str("}\n\n");
        }
    }
    out.push_str(&doc(
        &[
            format!("Which state a {} is in: its discriminant.", sum.entity),
            format!(
                "The domain's own: the tag column `{}` lives only in [`{}`].",
                sum.tag.wire, sum.row
            ),
        ],
        "",
    ));
    out.push_str(&derive(
        "Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord",
        serde,
    ));
    if sum
        .variants
        .iter()
        .any(|v| !is_upper_camel(&variant_ident(&v.value.ident)))
    {
        out.push_str("#[allow(non_camel_case_types)] // the model's spelling of a state\n");
    }
    out.push_str(&format!("pub enum {} {{\n", sum.kind));
    for variant in &sum.variants {
        out.push_str(&format!("    /// The state `{}`.\n", variant.value.wire));
        if serde {
            out.push_str(&format!(
                "    #[serde(rename = {})]\n",
                string_literal(&variant.value.wire)
            ));
        }
        out.push_str(&format!("    {},\n", variant_ident(&variant.value.ident)));
    }
    out.push_str("}\n\n");

    let mut lines = sum.doc.clone();
    lines.push(format!(
        "One of its states, told apart by `kind()`. A store holds it as [`{}`]: `from_row` (or `TryFrom`) reads one, `into_row` (or `From`) writes one.",
        sum.row
    ));
    out.push_str(&doc(&lines, ""));
    // The state types stay sealed: the sum goes over the wire as its row, and comes back only
    // through `from_row`, which refuses a row that fits no state. Only a sum with a row has it.
    let row_serde = serde && matches!(sum.tag.ty, FieldType::Enum(_));
    out.push_str(&derive("Debug, Clone, PartialEq, Eq", row_serde));
    if row_serde {
        out.push_str(&format!(
            "#[serde(try_from = \"{0}\", into = \"{0}\")]\n",
            sum.row
        ));
    }
    if sum
        .variants
        .iter()
        .any(|v| !is_upper_camel(&variant_ident(&v.value.ident)))
    {
        out.push_str("#[allow(non_camel_case_types)] // the model's spelling of a state\n");
    }
    out.push_str(&format!("pub enum {} {{\n", sum.name));
    for variant in &sum.variants {
        out.push_str(&format!(
            "    /// The state `{}`.\n    {}({}),\n",
            variant.value.wire,
            variant_ident(&variant.value.ident),
            variant.name
        ));
    }
    out.push_str("}\n\n");
    out.push_str(&format!("impl {} {{\n", sum.name));
    out.push_str("    /// The state it is in.\n");
    out.push_str(&format!(
        "    pub fn kind(&self) -> {} {{\n        match self {{\n",
        sum.kind
    ));
    for variant in &sum.variants {
        let v = variant_ident(&variant.value.ident);
        out.push_str(&format!("            Self::{v}(_) => {}::{v},\n", sum.kind));
    }
    out.push_str("        }\n    }\n}\n\n");
    for variant in &sum.variants {
        out.push_str(&format!(
            "impl ::core::convert::From<{0}> for {1} {{\n    fn from(state: {0}) -> Self {{\n        Self::{2}(state)\n    }}\n}}\n\n",
            variant.name,
            sum.name,
            variant_ident(&variant.value.ident)
        ));
    }
}

/// The type of `field` with its nullability.
fn typed(field: &Field) -> String {
    let ty = field_type(&field.ty);
    if field.nullable {
        format!("{OPTION}<{ty}>")
    } else {
        ty
    }
}

/// A field's doc lines, or a line naming its column when it has none.
fn field_doc(field: &Field, indent: &str) -> String {
    if field.doc.is_empty() {
        format!("{indent}/// The column `{}`.\n", field.wire)
    } else {
        doc(&field.doc, indent)
    }
}

/// The derive attribute of `traits`, with serde's two derives after them under `[code.rust] serde`.
fn derive(traits: &str, serde: bool) -> String {
    if serde {
        format!("#[derive({traits}, ::serde::Serialize, ::serde::Deserialize)]\n")
    } else {
        format!("#[derive({traits})]\n")
    }
}

/// `#[serde(rename = ..)]` on a field named `wire` whose Rust name serde would not spell as
/// `wire` (`self_` for `self`; serde drops the `r#` of a raw name itself); nothing otherwise.
fn serde_rename(wire: &str, serde: bool) -> String {
    if serde && ident(wire).trim_start_matches("r#") != wire {
        format!("    #[serde(rename = {})]\n", string_literal(wire))
    } else {
        String::new()
    }
}

/// `#[allow(non_snake_case)]` before a struct or an impl whose field names the model spells
/// otherwise; nothing when they are all snake_case.
fn allow_names(fields: &[Field]) -> String {
    if fields.iter().all(|f| is_snake(&ident(&f.wire))) {
        String::new()
    } else {
        "#[allow(non_snake_case)] // the model's spelling of a column\n".to_string()
    }
}

/// The clippy lints about method names that an accessor named after a column can trip, allowed
/// on the accessors' impl only where a column has such a name.
fn allow_accessor_names(fields: &[Field]) -> String {
    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    let mut lints = Vec::new();
    if names.contains(&"new") {
        lints.push("clippy::new_ret_no_self");
    }
    if names
        .iter()
        .any(|n| ["into_", "from_", "to_"].iter().any(|p| n.starts_with(p)))
    {
        lints.push("clippy::wrong_self_convention");
    }
    if names
        .iter()
        .any(|n| ["deref", "as_ref", "borrow", "not", "neg", "hash", "default"].contains(n))
    {
        lints.push("clippy::should_implement_trait");
    }
    if lints.is_empty() {
        String::new()
    } else {
        format!(
            "#[allow({})] // accessors are named after the model's columns\n",
            lints.join(", ")
        )
    }
}

/// Whether rustc's `non_snake_case` accepts `name` (a raw prefix aside).
fn is_snake(name: &str) -> bool {
    let name = name.trim_start_matches("r#");
    !name.chars().any(|c| c.is_uppercase())
}

/// Whether rustc's `non_camel_case_types` accepts `name`.
fn is_upper_camel(name: &str) -> bool {
    let trimmed = name.trim_matches('_');
    !trimmed.chars().next().is_some_and(|c| c.is_lowercase())
        && !trimmed.contains("__")
        && !trimmed.contains('_')
}

pub(super) fn field_type(ty: &FieldType) -> String {
    match ty {
        FieldType::Scalar(s) => scalar(*s).to_string(),
        FieldType::Enum(name) | FieldType::Id(name) => name.clone(),
    }
}

/// The type of a function input: a value's, or a slice of rows.
fn input_type(ty: &InputType) -> String {
    match ty {
        InputType::Value(ty) => field_type(ty),
        InputType::Rows(name) => format!("&[{name}]"),
    }
}

pub(super) fn scalar(s: Scalar) -> &'static str {
    match s {
        Scalar::Int => "i64",
        Scalar::String => STRING,
        Scalar::Bool => "bool",
        other => other.name(),
    }
}

/// A field name: a keyword is raw (`r#type`), and the keywords a raw identifier cannot be
/// get a trailing `_`.
pub(super) fn ident(name: &str) -> String {
    const NOT_RAW: [&str; 5] = ["self", "Self", "super", "crate", "_"];
    const KEYWORDS: [&str; 46] = [
        "as", "async", "await", "break", "const", "continue", "dyn", "else", "enum", "extern",
        "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
        "pub", "ref", "return", "static", "struct", "trait", "true", "type", "unsafe", "use",
        "where", "while", "abstract", "become", "box", "do", "final", "macro", "override", "priv",
        "typeof", "unsized", "virtual", "yield",
    ];
    if NOT_RAW.contains(&name) {
        format!("{name}_")
    } else if KEYWORDS.contains(&name) || name == "try" || name == "gen" {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

/// A function name: snake_case of the PascalCase name, escaped as [`ident`] escapes a field.
pub(super) fn fn_ident(pascal: &str) -> String {
    ident(&super::snake(pascal))
}

fn comment(line: &str) -> String {
    if line.is_empty() {
        "//\n".to_string()
    } else {
        format!("// {line}\n")
    }
}

pub(super) fn doc(lines: &[String], indent: &str) -> String {
    lines
        .iter()
        .map(|l| one_line(l))
        .filter(|l| !l.is_empty())
        .map(|l| format!("{indent}/// {l}\n"))
        .collect()
}

/// An enum variant: `Self` is the one PascalCase word Rust does not take.
pub(super) fn variant_ident(name: &str) -> String {
    if name == "Self" {
        "Self_".to_string()
    } else {
        name.to_string()
    }
}

/// `#[allow(clippy::too_many_arguments)]` before a function of more arguments than clippy
/// takes: one per row and input the model gives the use case.
pub(super) fn allow_arguments(count: usize, indent: &str) -> String {
    if count > MANY_ARGUMENTS {
        format!("{indent}#[allow(clippy::too_many_arguments)] // one argument per row and input of the use case\n")
    } else {
        String::new()
    }
}

// ── Errors ───────────────────────────────────────────────────────────────────

/// The error types every use case shares, and one `<UseCase>Error` per use case.
fn print_errors(domain: &DomainModel, out: &mut String) {
    out.push_str(&format!(
        r#"/// The rows are in states the use case does not run from, or a store found a row it writes
/// no longer what the result expects when it committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotEnabled {{
    /// The use case.
    pub usecase: &'static str,
}}

impl {FMT}::Display for NotEnabled {{
    fn fmt(&self, f: &mut {FMT}::Formatter<'_>) -> {FMT}::Result {{
        write!(f, "{{}} does not run from the states of its rows", self.usecase)
    }}
}}

impl ::std::error::Error for NotEnabled {{}}

/// A condition applied to the use case does not hold: the use case does not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejected {{
    /// The condition's id.
    pub condition: &'static str,
    /// The condition's label.
    pub label: &'static str,
    /// Where the condition is declared.
    pub site: {OPTION}<&'static str>,
}}

impl {FMT}::Display for Rejected {{
    fn fmt(&self, f: &mut {FMT}::Formatter<'_>) -> {FMT}::Result {{
        write!(f, "the condition {{}} ({{}}) does not hold", self.condition, self.label)
    }}
}}

impl ::std::error::Error for Rejected {{}}

/// The arguments break what the verified model keeps: a child row that does not name the
/// parent passed with it, a row that is not the event's, or a column its state keeps null or
/// present that is not. The caller's rows are wrong; the use case does not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidArgument {{
    /// The use case.
    pub usecase: &'static str,
    /// What the arguments break.
    pub reason: &'static str,
}}

impl {FMT}::Display for InvalidArgument {{
    fn fmt(&self, f: &mut {FMT}::Formatter<'_>) -> {FMT}::Result {{
        write!(f, "{{}}: {{}}", self.usecase, self.reason)
    }}
}}

impl ::std::error::Error for InvalidArgument {{}}

"#
    ));
    for e in &domain.errors {
        print_error(e, out);
    }
}

/// The cases of `e`: `(variant, payload type, doc)`.
fn error_cases(e: &UseCaseErrorDecl) -> Vec<(&'static str, String)> {
    let mut cases = vec![(
        "NotEnabled",
        format!("The rows are in states {} does not run from, or a commit found a row it writes changed.", e.usecase),
    )];
    if e.rejects {
        cases.push((
            "Rejected",
            format!("A condition applied to {} does not hold.", e.usecase),
        ));
    }
    if e.invalid_argument {
        cases.push((
            "InvalidArgument",
            "The arguments break what the verified model keeps.".to_string(),
        ));
    }
    cases
}

fn print_error(e: &UseCaseErrorDecl, out: &mut String) {
    let name = &e.name;
    out.push_str(&format!(
        "/// Why {} did not run: what its functions, its dispatcher, and the commit of its result return.\n",
        e.usecase
    ));
    out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
    out.push_str(&format!("pub enum {name} {{\n"));
    let cases = error_cases(e);
    for (case, text) in &cases {
        out.push_str(&format!("    /// {text}\n    {case}({case}),\n"));
    }
    out.push_str("}\n\n");
    out.push_str(&format!(
        "impl {FMT}::Display for {name} {{\n    fn fmt(&self, f: &mut {FMT}::Formatter<'_>) -> {FMT}::Result {{\n        match self {{\n"
    ));
    for (case, _) in &cases {
        out.push_str(&format!(
            "            Self::{case}(e) => {FMT}::Display::fmt(e, f),\n"
        ));
    }
    out.push_str("        }\n    }\n}\n\n");
    out.push_str(&format!(
        "impl ::std::error::Error for {name} {{\n    fn source(&self) -> {OPTION}<&(dyn ::std::error::Error + 'static)> {{\n        match self {{\n"
    ));
    for (case, _) in &cases {
        out.push_str(&format!(
            "            Self::{case}(e) => {OPTION}::Some(e),\n"
        ));
    }
    out.push_str("        }\n    }\n}\n\n");
    for (case, _) in &cases {
        out.push_str(&format!(
            "impl ::core::convert::From<{case}> for {name} {{\n    fn from(e: {case}) -> Self {{\n        Self::{case}(e)\n    }}\n}}\n\n"
        ));
    }
}

/// The `Err` that returns `InvalidArgument` of `f`'s use case, as an expression.
fn invalid_argument(domain: &DomainModel, f: &FnDecl, reason: &str) -> String {
    let error = domain.error_name(&f.usecase);
    format!(
        "{error}::InvalidArgument(InvalidArgument {{ usecase: {}, reason: {} }})",
        string_literal(&f.usecase),
        string_literal(reason)
    )
}

// ── Use cases ────────────────────────────────────────────────────────────────

/// Whether a value of `ty` is `Copy`: read by value, never cloned.
pub(super) fn is_copy(domain: &DomainModel, ty: &FieldType) -> bool {
    match ty {
        FieldType::Scalar(s) => matches!(s, Scalar::Int | Scalar::Bool),
        FieldType::Enum(_) => true,
        FieldType::Id(name) => domain
            .ids
            .iter()
            .find(|i| i.name == *name)
            .is_some_and(|i| matches!(i.repr, Scalar::Int | Scalar::Bool)),
    }
}

/// `.clone()` after a place of type `ty` that is not `Copy`.
fn clone_of(domain: &DomainModel, ty: &FieldType) -> &'static str {
    if is_copy(domain, ty) {
        ""
    } else {
        ".clone()"
    }
}

/// The id type of the run mark type `mark`.
fn mark_id(domain: &DomainModel, mark: &str) -> FieldType {
    FieldType::Id(
        domain
            .marks
            .iter()
            .find(|m| m.name == mark)
            .map_or_else(String::new, |m| m.id_ty.clone()),
    )
}

/// Whether `body` reads the variable `name`: a mention that is no field (`.name`), no field
/// of a struct literal (`name: `), and not inside a string literal.
fn mentions(body: &str, name: &str) -> bool {
    let mut code = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for c in body.chars() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            code.push(' ');
        } else {
            if c == '"' {
                in_string = true;
            }
            code.push(c);
        }
    }
    let is_ident = |c: char| c.is_alphanumeric() || c == '_' || c == '#';
    code.match_indices(name).any(|(at, _)| {
        let before = code[..at].chars().next_back();
        let rest = &code[at + name.len()..];
        !before.is_some_and(|c| is_ident(c) || c == '.')
            && !rest.chars().next().is_some_and(is_ident)
            && !(rest.starts_with(": ") || rest.starts_with(":\n"))
    })
}

/// `name` as a parameter: `_name` when `body` does not read it.
fn param_name(name: &str, body: &str) -> String {
    if mentions(body, name) {
        name.to_string()
    } else {
        format!("_{}", name.trim_start_matches("r#"))
    }
}

pub(super) fn print_functions(domain: &DomainModel, serde: bool, out: &mut String) {
    print_events(domain, serde, out);
    if domain.fns.is_empty() {
        // The use cases with no printed function still have their errors (#335).
        if !domain.errors.is_empty() {
            out.push_str("// ── Use cases ──\n\n");
            print_errors(domain, out);
        }
        return;
    }
    print_marks(domain, serde, out);
    out.push_str("// ── Use cases ──\n\n");
    print_errors(domain, out);
    out.push_str("/// What a row a result writes must still be when the write commits, so the store can write\n/// it conditionally and take a write that matches no row as `NotEnabled` (see the obligations).\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("pub enum ExpectedSource {\n    /// The row is created: no row has its id.\n    Absent,\n    /// The row exists.\n    Present,\n    /// The row exists, and its `column` holds `value`.\n    In {\n        /// The column, as the row spells it.\n        column: &'static str,\n        /// The value, as the model spells it.\n        value: &'static str,\n    },\n}\n\n");
    for f in &domain.fns {
        print_result(f, out);
        print_function(domain, f, out);
    }
}

/// The event types: one per event a use case raises or a triggered use case takes.
fn print_events(domain: &DomainModel, serde: bool, out: &mut String) {
    if !domain.events.is_empty() {
        out.push_str("// ── Events ──\n\n");
    }
    for event in &domain.events {
        let mut lines = event.doc.clone();
        if lines.is_empty() {
            lines.push(format!("The event {}.", event.name));
        }
        out.push_str(&doc(&lines, ""));
        if event.fields.is_empty() {
            out.push_str(&derive("Debug, Clone, Copy, PartialEq, Eq", serde));
            out.push_str(&format!("pub struct {};\n\n", event.name));
        } else {
            out.push_str(&derive("Debug, Clone, PartialEq, Eq", serde));
            out.push_str(&format!("pub struct {} {{\n", event.name));
            for field in &event.fields {
                out.push_str(&format!(
                    "    /// The {} row the event was raised on.\n    pub {}: {},\n",
                    field.entity,
                    ident(&field.name),
                    field.id_ty
                ));
            }
            out.push_str("}\n\n");
        }
    }
}

/// The run marks of the use cases split into transactions.
fn print_marks(domain: &DomainModel, serde: bool, out: &mut String) {
    if !domain.marks.is_empty() {
        out.push_str("// ── Run marks of use cases split into transactions ──\n\n");
    }
    for m in &domain.marks {
        out.push_str(&format!(
            "/// The mark step {} of {} leaves on a {} row: the next step takes it. Store it with the row.\n",
            m.step, m.usecase, m.entity
        ));
        out.push_str(&derive("Debug, Clone, PartialEq, Eq", serde));
        out.push_str(&format!(
            "pub struct {} {{\n    /// The id of the marked row.\n    pub id: {},\n}}\n\n",
            m.name, m.id_ty
        ));
    }
}

/// The result type of `f`.
fn print_result(f: &FnDecl, out: &mut String) {
    use super::ir::OutKind;
    use super::snake;
    out.push_str(&format!("/// What `{}` returns.\n", fn_ident(&f.name)));
    out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
    out.push_str(&format!("pub struct {} {{\n", f.result));
    for o in &f.outputs {
        match &o.kind {
            OutKind::Row { ty, .. } => out.push_str(&format!(
                "    /// The row `{}` to write.\n    pub {}: {ty},\n",
                o.name,
                ident(&o.name)
            )),
            OutKind::Deleted {
                id: Some((ty, _)), ..
            } => out.push_str(&format!(
                "    /// The id of the row `{}` to delete.\n    pub {}: {ty},\n",
                o.name,
                ident(&o.name)
            )),
            OutKind::Deleted { id: None, .. } => {}
        }
    }
    for o in &f.outputs {
        out.push_str(&format!(
            "    /// What `{}` must still be when it is written.\n    pub {}: ExpectedSource,\n",
            o.name,
            ident(&o.expected_name())
        ));
    }
    for m in &f.marks_out {
        out.push_str(&format!(
            "    /// The run mark to store for the next step.\n    pub {}: {},\n",
            ident(&m.name),
            m.ty
        ));
    }
    for event in &f.raises {
        out.push_str(&format!(
            "    /// The event raised.\n    pub {}: {},\n",
            ident(&snake(&event.ty)),
            event.ty
        ));
    }
    for (m, field) in f.marks_ended() {
        out.push_str(&format!(
            "    /// The run mark the step took and ends: delete it in the same transaction.\n    pub {}: {},\n",
            ident(&field),
            m.ty
        ));
    }
    out.push_str("}\n\n");
}

/// The function `f`: its checks, conditions, and the result it builds.
fn print_function(domain: &DomainModel, f: &FnDecl, out: &mut String) {
    use super::ir::{Adapt, ArgCheck, Assigned, OutKind};
    use super::snake;
    let error = domain.error_name(&f.usecase);
    let row = |r: usize| ident(&f.row(r).name);
    let input = |i: usize| ident(&f.inputs[i].name);
    let invalid = |reason: &str| invalid_argument(domain, f, reason);
    let mut body = String::new();
    for check in &f.checks {
        let (test, message) = match check {
            ArgCheck::Link {
                child,
                child_field,
                parent,
                parent_field,
                message,
            } => {
                let (c, p) = (
                    format!("{}.{}", row(*child), ident(&child_field.name)),
                    format!("{}.{}", row(*parent), ident(&parent_field.name)),
                );
                let differs = if child_field.nullable {
                    format!("{c}.as_ref() != {OPTION}::Some(&{p})")
                } else {
                    format!("{c} != {p}")
                };
                (differs, message)
            }
            ArgCheck::EventRow {
                row: r,
                field,
                event_field,
                message,
            } => {
                let ev = f
                    .event_in
                    .as_ref()
                    .map_or_else(String::new, |e| ident(&e.name));
                (
                    format!(
                        "{}.{} != {ev}.{}",
                        row(*r),
                        ident(&field.name),
                        ident(event_field)
                    ),
                    message,
                )
            }
            ArgCheck::MarkRow {
                row: r,
                field,
                mark,
                message,
            } => (
                format!(
                    "{}.{} != {}.id",
                    row(*r),
                    ident(&field.name),
                    ident(&f.marks_in[*mark].name)
                ),
                message,
            ),
            ArgCheck::MustBeNull {
                row: r,
                field,
                message,
            } => (
                format!("{}.{}.is_some()", row(*r), ident(&field.name)),
                message,
            ),
        };
        body.push_str(&format!(
            "    if {test} {{\n        return {RESULT}::Err({});\n    }}\n",
            invalid(message)
        ));
    }
    for c in &f.conditions {
        body.push_str(&format!(
                "    if {} {{\n        return {RESULT}::Err({error}::Rejected(Rejected {{\n            condition: {},\n            label: {},\n            site: {},\n        }}));\n    }}\n",
                expr(&c.expr.fails(), &row, &input),
                string_literal(&c.id),
                string_literal(&c.label),
                c.site.as_ref().map_or(format!("{OPTION}::None"), |s| format!("{OPTION}::Some({})", string_literal(s)))
            ));
    }
    let source = |s: &super::ir::MarkSource| match s {
        super::ir::MarkSource::Row { row: r, field } => {
            format!(
                "{}.{}{}",
                row(*r),
                ident(&field.name),
                clone_of(domain, &field.ty)
            )
        }
        super::ir::MarkSource::Input(i) => {
            let clone = match &f.inputs[*i].ty {
                InputType::Value(ty) => clone_of(domain, ty),
                InputType::Rows(_) => ".clone()",
            };
            format!("{}{clone}", input(*i))
        }
        super::ir::MarkSource::Mark(k) => {
            let m = &f.marks_in[*k];
            format!(
                "{}.id{}",
                ident(&m.name),
                clone_of(domain, &mark_id(domain, &m.ty))
            )
        }
    };
    // The marks first: the result below moves the rows' fields.
    for m in &f.marks_out {
        body.push_str(&format!(
            "    let out_{} = {} {{ id: {} }};\n",
            m.name,
            m.ty,
            source(&m.source)
        ));
    }
    for event in &f.raises {
        let parts: Vec<String> = event
            .ids
            .iter()
            .map(|(field, s)| init(&ident(&field.name), &source(s)))
            .collect();
        let value = if parts.is_empty() {
            event.ty.clone()
        } else {
            format!("{} {{ {} }}", event.ty, parts.join(", "))
        };
        body.push_str(&format!("    let out_{} = {value};\n", snake(&event.ty)));
    }
    // The derived columns too: their expressions read the rows the result moves.
    let derived = |o: &str, field: &str| ident(&format!("derived_{o}_{field}"));
    for o in &f.outputs {
        let OutKind::Row { fields, .. } = &o.kind else {
            continue;
        };
        for (target, assigned) in fields {
            if let Assigned::Derived { expr: e, .. } = assigned {
                body.push_str(&format!(
                    "    let {} = {};\n",
                    derived(&o.name, &target.name),
                    expr(e, &row, &input)
                ));
            }
        }
    }
    let mut result = format!("{} {{\n", f.result);
    for o in &f.outputs {
        match &o.kind {
            OutKind::Row { ty, fields, .. } => {
                result.push_str(&format!("        {}: {ty} {{\n", ident(&o.name)));
                for (target, assigned) in fields {
                    let wrap = |v: String, w: bool| {
                        if w {
                            format!("{OPTION}::Some({v})")
                        } else {
                            v
                        }
                    };
                    let v = match assigned {
                        Assigned::From {
                            row: r,
                            field,
                            adapt,
                        } => {
                            let read = format!("{}.{}", row(*r), ident(&field.name));
                            match adapt {
                                Adapt::Same => read,
                                Adapt::Wrap => format!("{OPTION}::Some({read})"),
                                Adapt::Unwrap(message) => {
                                    format!("{read}.ok_or({})?", invalid(message))
                                }
                                Adapt::AbsentNull => format!("{OPTION}::None"),
                            }
                        }
                        Assigned::Lit { lit, wrap: w } => wrap(lit_rust(lit, &target.ty), *w),
                        Assigned::Input { input: i, wrap: w } => wrap(input(*i), *w),
                        Assigned::ParentId {
                            row: r,
                            field,
                            wrap: w,
                        } => wrap(
                            format!(
                                "{}.{}{}",
                                row(*r),
                                ident(&field.name),
                                clone_of(domain, &field.ty)
                            ),
                            *w,
                        ),
                        Assigned::Switch {
                            row: r,
                            field,
                            enum_ty,
                            arms,
                        } => {
                            let mut v = format!("match {}.{} {{ ", row(*r), ident(&field.name));
                            for (from, to) in arms {
                                v.push_str(&format!(
                                    "{enum_ty}::{} => {enum_ty}::{}, ",
                                    variant_ident(&from.ident),
                                    variant_ident(&to.ident)
                                ));
                            }
                            let all = domain
                                .enums
                                .iter()
                                .find(|e| e.name == *enum_ty)
                                .map_or(0, |e| e.values.len());
                            if arms.len() < all {
                                v.push_str(&format!(
                                    "_ => return {RESULT}::Err({}), ",
                                    invalid(&format!(
                                        "{} is in no state {} moves it from",
                                        field.wire, f.usecase
                                    ))
                                ));
                            }
                            v.push('}');
                            v
                        }
                        Assigned::EventId { field, wrap: w, .. } => {
                            let ev = f.event_in.as_ref();
                            let id_ty = ev
                                .and_then(|ev| domain.events.iter().find(|e| e.name == ev.ty))
                                .and_then(|e| e.fields.iter().find(|x| x.name == *field))
                                .map_or_else(String::new, |x| x.id_ty.clone());
                            wrap(
                                format!(
                                    "{}.{}{}",
                                    ev.map_or_else(String::new, |e| ident(&e.name)),
                                    ident(field),
                                    clone_of(domain, &FieldType::Id(id_ty))
                                ),
                                *w,
                            )
                        }
                        Assigned::MarkId { mark, wrap: w } => {
                            let m = &f.marks_in[*mark];
                            wrap(
                                format!(
                                    "{}.id{}",
                                    ident(&m.name),
                                    clone_of(domain, &mark_id(domain, &m.ty))
                                ),
                                *w,
                            )
                        }
                        Assigned::Null => format!("{OPTION}::None"),
                        Assigned::Derived { wrap: w, .. } => {
                            wrap(derived(&o.name, &target.name), *w)
                        }
                    };
                    result.push_str(&format!(
                        "            {},\n",
                        init(&ident(&target.name), &v)
                    ));
                }
                result.push_str("        },\n");
            }
            OutKind::Deleted {
                row: r,
                id: Some((_, field)),
            } => {
                result.push_str(&format!(
                    "        {}: {}.{},\n",
                    ident(&o.name),
                    row(*r),
                    ident(&field.name)
                ));
            }
            OutKind::Deleted { id: None, .. } => {}
        }
    }
    for o in &f.outputs {
        let expected = match f.expected(o) {
            super::ir::Expected::Absent => "ExpectedSource::Absent".to_string(),
            super::ir::Expected::Present => "ExpectedSource::Present".to_string(),
            super::ir::Expected::In { column, value } => format!(
                "ExpectedSource::In {{ column: {}, value: {} }}",
                string_literal(&column),
                string_literal(&value)
            ),
        };
        result.push_str(&format!(
            "        {}: {expected},\n",
            ident(&o.expected_name())
        ));
    }
    for m in &f.marks_out {
        result.push_str(&format!("        {}: out_{},\n", ident(&m.name), m.name));
    }
    for event in &f.raises {
        result.push_str(&format!(
            "        {}: out_{},\n",
            ident(&snake(&event.ty)),
            snake(&event.ty)
        ));
    }
    for (m, field) in f.marks_ended() {
        result.push_str(&format!(
            "        {},\n",
            init(&ident(&field), &ident(&m.name))
        ));
    }
    result.push_str("    }");
    body.push_str(&format!("    {RESULT}::Ok({result})\n"));

    let mut params: Vec<String> = f
        .rows
        .iter()
        .map(|r| format!("{}: {}", param_name(&ident(&r.name), &body), r.ty))
        .collect();
    for m in &f.marks_in {
        params.push(format!("{}: {}", param_name(&ident(&m.name), &body), m.ty));
    }
    if let Some(ev) = &f.event_in {
        params.push(format!(
            "{}: {}",
            param_name(&ident(&ev.name), &body),
            ev.ty
        ));
    }
    for i in &f.inputs {
        let ty = input_type(&i.ty);
        let ty = if i.nullable {
            format!("{OPTION}<{ty}>")
        } else {
            ty
        };
        params.push(format!("{}: {ty}", param_name(&ident(&i.name), &body)));
    }

    let mut lines = f.doc.clone();
    if lines.is_empty() {
        lines.push(format!("Runs {}.", f.usecase));
    }
    for c in &f.conditions {
        lines.push(format!(
            "Requires the condition {} ({}){}.",
            c.id,
            one_line(&c.label),
            c.site
                .as_ref()
                .map(|s| format!(" at {s}"))
                .unwrap_or_default()
        ));
    }
    let mut errors = Vec::new();
    if !f.conditions.is_empty() {
        errors.push("`Rejected` when a condition does not hold");
    }
    if domain.invalid_argument_possible(f) {
        errors.push("`InvalidArgument` when the arguments break what the model keeps");
    }
    if !errors.is_empty() {
        lines.push(String::new());
        lines.push("# Errors".to_string());
        lines.push(String::new());
        lines.push(format!("{}.", errors.join("; ")));
    }
    out.push_str(&doc_block(&lines, ""));
    out.push_str(&allow_arguments(params.len(), ""));
    out.push_str(&format!(
        "pub fn {}({}) -> {RESULT}<{}, {error}> {{\n{body}}}\n\n",
        fn_ident(&f.name),
        params.join(", "),
        f.result,
    ));
}

/// A field of a struct literal: `name: value`, or `name` alone when the value is a variable of
/// that name.
fn init(name: &str, value: &str) -> String {
    if name == value {
        name.to_string()
    } else {
        format!("{name}: {value}")
    }
}

/// Doc lines with blank lines kept, as paragraph breaks.
fn doc_block(lines: &[String], indent: &str) -> String {
    lines
        .iter()
        .map(|l| {
            let l = one_line(l);
            if l.is_empty() {
                format!("{indent}///\n")
            } else {
                format!("{indent}/// {l}\n")
            }
        })
        .collect()
}

/// A literal of the model, of the type `ty` of the field it is written to.
fn lit_rust(lit: &super::ir::Lit, ty: &FieldType) -> String {
    match (lit, ty) {
        (super::ir::Lit::Enum { ty, value }, _) => {
            format!("{ty}::{}", variant_ident(&value.ident))
        }
        (super::ir::Lit::Bool(b), _) => b.to_string(),
        // A number written to a text scalar: the module makes it directly, as its parser would.
        (super::ir::Lit::Int(n), FieldType::Scalar(s @ (Scalar::Money | Scalar::Decimal))) => {
            format!("{}({STRING}::from(\"{n}\"))", s.name())
        }
        (super::ir::Lit::Int(n), _) => n.to_string(),
    }
}

fn expr(
    e: &super::ir::Expr,
    row: &dyn Fn(usize) -> String,
    input: &dyn Fn(usize) -> String,
) -> String {
    use super::ir::Expr;
    let go = |e: &Expr| expr(e, row, input);
    let place = |r: &usize, field: &Field| format!("{}.{}", row(*r), ident(&field.name));
    match e {
        Expr::True => "true".to_string(),
        Expr::False => "false".to_string(),
        Expr::Not(inner) => format!("!({})", go(inner)),
        Expr::And(items) if items.is_empty() => "true".to_string(),
        Expr::Or(items) if items.is_empty() => "false".to_string(),
        Expr::And(items) => items
            .iter()
            .map(|i| format!("({})", go(i)))
            .collect::<Vec<_>>()
            .join(" && "),
        Expr::Or(items) => items
            .iter()
            .map(|i| format!("({})", go(i)))
            .collect::<Vec<_>>()
            .join(" || "),
        Expr::IsNull { row: r, field } => format!("{}.is_none()", place(r, field)),
        Expr::EnumEq {
            row: r,
            field,
            value,
        } if field.nullable => format!(
            "{} == {OPTION}::Some({}::{})",
            place(r, field),
            field.enum_ty(),
            variant_ident(&value.ident)
        ),
        Expr::EnumEq {
            row: r,
            field,
            value,
        } => format!(
            "{} == {}::{}",
            place(r, field),
            field.enum_ty(),
            variant_ident(&value.ident)
        ),
        // Null satisfies no comparison (#359): a nullable field equals only a present value.
        Expr::BoolEq {
            row: r,
            field,
            value,
        } if field.nullable => format!("{} == {OPTION}::Some({value})", place(r, field)),
        Expr::BoolEq {
            row: r,
            field,
            value,
        } => {
            if *value {
                place(r, field)
            } else {
                format!("!{}", place(r, field))
            }
        }
        Expr::IntCmp {
            row: r,
            field,
            op,
            value,
        } => format!("{} {} {value}", place(r, field), op.symbol()),
        Expr::InputIsNull { input: i } => format!("{}.is_none()", input(*i)),
        Expr::InputEnumEq {
            input: i,
            nullable,
            enum_ty,
            value,
        } => {
            let v = format!("{enum_ty}::{}", variant_ident(&value.ident));
            if *nullable {
                format!("{} == {OPTION}::Some({v})", input(*i))
            } else {
                format!("{} == {v}", input(*i))
            }
        }
        Expr::InputBoolEq {
            input: i,
            nullable: true,
            value,
        } => format!("{} == {OPTION}::Some({value})", input(*i)),
        Expr::InputBoolEq {
            input: i, value, ..
        } => {
            if *value {
                input(*i)
            } else {
                format!("!{}", input(*i))
            }
        }
        Expr::KeyEq {
            child,
            child_field,
            parent,
            parent_field,
        } => {
            let c = place(child, child_field);
            let p = place(parent, parent_field);
            if child_field.nullable {
                format!("{c}.as_ref() == {OPTION}::Some(&{p})")
            } else {
                format!("{c} == {p}")
            }
        }
        Expr::Exists {
            input: i,
            elem,
            body,
        } => format!("{}.iter().any(|{}| {})", input(*i), row(*elem), go(body)),
        Expr::Forall {
            input: i,
            elem,
            body,
        } => format!("{}.iter().all(|{}| {})", input(*i), row(*elem), go(body)),
        Expr::Count {
            input: i,
            elem,
            body,
            op,
            n,
        } => format!(
            "{}.iter().filter(|{}| {}).count() {} {n}",
            input(*i),
            row(*elem),
            go(body),
            op.symbol()
        ),
    }
}

// ── Rule checks ──────────────────────────────────────────────────────────────

pub(super) fn print_validators(domain: &DomainModel, out: &mut String) {
    if domain.validators.is_empty() {
        return;
    }
    out.push_str("// ── Rule checks ──\n\n");
    let row = |_: usize| "self".to_string();
    for v in &domain.validators {
        out.push_str(&format!("impl {} {{\n", v.ty));
        out.push_str(
            "    /// The rules this row breaks, by the names `check` reports: empty when it keeps them all.\n",
        );
        out.push_str("    pub fn rule_violations(&self) -> ::std::vec::Vec<&'static str> {\n");
        if v.rules.is_empty() {
            out.push_str("        ::std::vec::Vec::new()\n    }\n}\n\n");
            continue;
        }
        out.push_str("        let mut out = ::std::vec::Vec::new();\n");
        for rule in &v.rules {
            out.push_str(&format!(
                "        // {}\n        if {} {{\n            out.push({});\n        }}\n",
                one_line(&rule.text),
                expr(&rule.expr.fails(), &row, &|_| String::new()),
                string_literal(&rule.name)
            ));
        }
        out.push_str("        out\n    }\n}\n\n");
    }
    for sum in &domain.sums {
        out.push_str(&format!("impl {} {{\n", sum.name));
        out.push_str(
            "    /// The rules this row breaks, by its state: empty when it keeps them all.\n",
        );
        out.push_str("    pub fn rule_violations(&self) -> ::std::vec::Vec<&'static str> {\n        match self {\n");
        for variant in &sum.variants {
            out.push_str(&format!(
                "            Self::{}(state) => state.rule_violations(),\n",
                variant_ident(&variant.value.ident)
            ));
        }
        out.push_str("        }\n    }\n}\n\n");
    }
}

// ── Rows and dispatch ────────────────────────────────────────────────────────

pub(super) fn print_rows(domain: &DomainModel, serde: bool, out: &mut String) {
    if domain.sums.is_empty() {
        return;
    }
    out.push_str("// ── Rows: the flat shape a store or the wire holds ──\n\n");
    out.push_str(&format!(
        r#"/// A row that fits no state of its entity: `from_row` refuses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidRow {{
    /// The entity.
    pub entity: &'static str,
    /// The column that breaks what the state requires.
    pub column: &'static str,
    /// What the state requires of it.
    pub reason: &'static str,
}}

impl {FMT}::Display for InvalidRow {{
    fn fmt(&self, f: &mut {FMT}::Formatter<'_>) -> {FMT}::Result {{
        write!(f, "{{}}.{{}}: {{}}", self.entity, self.column, self.reason)
    }}
}}

impl ::std::error::Error for InvalidRow {{}}

"#
    ));
    for sum in &domain.sums {
        let FieldType::Enum(tag_enum) = &sum.tag.ty else {
            continue;
        };
        out.push_str(&format!(
            "/// Every column of {} as the data layer declares it: the shape a store or the wire holds. The\n/// scalars it holds were parsed when it was built.\n",
            sum.entity
        ));
        out.push_str(&derive("Debug, Clone, PartialEq, Eq", serde));
        let wires: Vec<Field> = sum
            .row_fields
            .iter()
            .map(|f| Field {
                name: f.wire.clone(),
                ..f.clone()
            })
            .collect();
        out.push_str(&allow_names(&wires));
        out.push_str(&format!("pub struct {} {{\n", sum.row));
        for f in &sum.row_fields {
            out.push_str(&format!("    /// The column `{}`.\n", f.wire));
            out.push_str(&serde_rename(&f.wire, serde));
            out.push_str(&format!("    pub {}: {},\n", ident(&f.wire), typed(f)));
        }
        out.push_str("}\n\n");

        // TryFrom<Row>. A tag column marked `@null` holds null in no state: such a row is an
        // `InvalidRow`, and the row of a state holds its tag.
        let tag_nullable = sum
            .row_fields
            .iter()
            .any(|f| f.wire == sum.tag.wire && f.nullable);
        let tag_value = |vi: &str| {
            if tag_nullable {
                format!("{OPTION}::Some({tag_enum}::{vi})")
            } else {
                format!("{tag_enum}::{vi}")
            }
        };
        let invalid = |column: &str, reason: String| {
            format!(
                "InvalidRow {{ entity: {}, column: {}, reason: {} }}",
                string_literal(&sum.entity),
                string_literal(column),
                string_literal(&reason)
            )
        };
        out.push_str(&format!(
            "impl ::core::convert::TryFrom<{row}> for {sum} {{\n    type Error = InvalidRow;\n\n    /// The state of a row, or why the row fits none.\n    fn try_from(row: {row}) -> {RESULT}<Self, InvalidRow> {{\n        match row.{tag} {{\n",
            row = sum.row,
            sum = sum.name,
            tag = ident(&sum.tag.wire)
        ));
        for v in &sum.variants {
            let vi = variant_ident(&v.value.ident);
            out.push_str(&format!("            {} => {{\n", tag_value(&vi)));
            for f in &v.always_null {
                out.push_str(&format!(
                    "                if row.{}.is_some() {{\n                    return {RESULT}::Err({});\n                }}\n",
                    ident(&f.wire),
                    invalid(&f.wire, format!("null in the state {}", v.value.wire))
                ));
            }
            let mut inits = Vec::new();
            for f in &v.fields {
                let declared = sum.row_fields.iter().find(|r| r.wire == f.wire);
                if declared.is_some_and(|d| d.nullable) && !f.nullable {
                    let local = format!("present_{}", f.wire);
                    out.push_str(&format!(
                        "                let {OPTION}::Some({local}) = row.{} else {{\n                    return {RESULT}::Err({});\n                }};\n",
                        ident(&f.wire),
                        invalid(&f.wire, format!("present in the state {}", v.value.wire))
                    ));
                    inits.push(format!("{}: {local}", ident(&f.name)));
                } else {
                    inits.push(format!("{}: row.{}", ident(&f.name), ident(&f.wire)));
                }
            }
            let state = if inits.is_empty() {
                format!("{} {{}}", v.name)
            } else {
                format!("{} {{ {} }}", v.name, inits.join(", "))
            };
            out.push_str(&format!(
                "                {RESULT}::Ok(Self::{vi}({state}))\n            }}\n"
            ));
        }
        if tag_nullable {
            out.push_str(&format!(
                "            {OPTION}::None => {RESULT}::Err({}),\n",
                invalid(&sum.tag.wire, "null, which is no state".to_string())
            ));
        }
        out.push_str("        }\n    }\n}\n\n");

        // From<Sum> for Row.
        out.push_str(&format!(
            "impl ::core::convert::From<{sum}> for {row} {{\n    /// The flat row of a state.\n    fn from(state: {sum}) -> Self {{\n        match state {{\n",
            row = sum.row,
            sum = sum.name
        ));
        for v in &sum.variants {
            let vi = variant_ident(&v.value.ident);
            let mut inits = Vec::new();
            for f in &sum.row_fields {
                let name = ident(&f.wire);
                if f.wire == sum.tag.wire {
                    inits.push(format!("{name}: {}", tag_value(&vi)));
                } else if v.always_null.iter().any(|n| n.wire == f.wire) {
                    inits.push(format!("{name}: {OPTION}::None"));
                } else if let Some(vf) = v.fields.iter().find(|x| x.wire == f.wire) {
                    let value = format!("s.{}", ident(&vf.name));
                    if f.nullable && !vf.nullable {
                        inits.push(format!("{name}: {OPTION}::Some({value})"));
                    } else {
                        inits.push(format!("{name}: {value}"));
                    }
                }
            }
            let bind = if inits.iter().any(|i| i.contains("s.")) {
                "s"
            } else {
                "_"
            };
            out.push_str(&format!(
                "            {}::{vi}({bind}) => Self {{ {} }},\n",
                sum.name,
                inits.join(", ")
            ));
        }
        out.push_str("        }\n    }\n}\n\n");

        out.push_str(&format!(
            "impl {sum} {{\n    /// The state of a row, or why the row fits none: [`TryFrom`] by name.\n    ///\n    /// # Errors\n    ///\n    /// [`InvalidRow`] when the row fits no state.\n    pub fn from_row(row: {row}) -> {RESULT}<Self, InvalidRow> {{\n        <Self as ::core::convert::TryFrom<{row}>>::try_from(row)\n    }}\n\n    /// The flat row of this state: [`From`] by name.\n    pub fn into_row(self) -> {row} {{\n        {row}::from(self)\n    }}\n}}\n\n",
            row = sum.row,
            sum = sum.name
        ));
    }
}

pub(super) fn print_dispatchers(domain: &DomainModel, out: &mut String) {
    use super::snake;
    if domain.dispatchers.is_empty() {
        return;
    }
    out.push_str("// ── Dispatch: run a use case from whatever states its rows are in ──\n\n");
    for d in &domain.dispatchers {
        let error = domain.error_name(&d.usecase);
        out.push_str(&format!(
            "/// What `{}` returns when the use case runs.\n",
            snake(&d.name)
        ));
        out.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
        out.push_str(&format!("pub enum {} {{\n", d.outcome));
        for c in &d.cases {
            out.push_str(&format!(
                "    /// `{}` ran.\n    {}({}),\n",
                fn_ident(&c.function),
                c.case,
                c.result
            ));
        }
        out.push_str("}\n\n");
        out.push_str(&format!(
            "/// Runs {} from whatever states its rows are in: the transition function of those states.\n///\n/// # Errors\n///\n/// `NotEnabled` when the rows are in states it does not run from, and what the transition\n/// function returns.\n",
            d.usecase
        ));
        let mut params: Vec<String> = d
            .rows
            .iter()
            .map(|(n, ty)| format!("{}: {ty}", ident(n)))
            .collect();
        for (n, ty) in &d.marks {
            params.push(format!("{}: {ty}", ident(n)));
        }
        for i in &d.inputs {
            let ty = input_type(&i.ty);
            let ty = if i.nullable {
                format!("{OPTION}<{ty}>")
            } else {
                ty
            };
            params.push(format!("{}: {ty}", ident(&i.name)));
        }
        for (n, ty) in &d.choices {
            params.push(format!("{}: {ty}", ident(n)));
        }
        out.push_str(&allow_arguments(params.len(), ""));
        out.push_str(&format!(
            "pub fn {}({}) -> {RESULT}<{}, {error}> {{\n",
            snake(&d.name),
            params.join(", "),
            d.outcome
        ));
        let scrutinee: Vec<String> = d.rows.iter().map(|(n, _)| ident(n)).collect();
        out.push_str(&format!("    match ({},) {{\n", scrutinee.join(", ")));
        for c in &d.cases {
            let pats: Vec<String> = c
                .states
                .iter()
                .zip(&d.rows)
                .map(|(s, (n, ty))| match s {
                    Some((_, _, value)) => {
                        format!("{ty}::{}({})", variant_ident(&value.ident), ident(n))
                    }
                    None => ident(n),
                })
                .collect();
            let mut args: Vec<String> = d.rows.iter().map(|(n, _)| ident(n)).collect();
            args.extend(d.marks.iter().map(|(n, _)| ident(n)));
            args.extend(c.inputs.iter().map(|i| ident(&d.inputs[*i].name)));
            let call = format!("{}({})", fn_ident(&c.function), args.join(", "));
            let body = format!("{call}.map({}::{})", d.outcome, c.case);
            let guard: Vec<String> = c
                .choice
                .iter()
                .map(|(i, v)| {
                    let (n, ty) = &d.choices[*i];
                    format!("{} == {ty}::{}", ident(n), variant_ident(&v.ident))
                })
                .collect();
            let guard = if guard.is_empty() {
                String::new()
            } else {
                format!(" if {}", guard.join(" && "))
            };
            out.push_str(&format!(
                "        ({},){guard} => {body},\n",
                pats.join(", ")
            ));
        }
        // A catch-all arm only where the cases leave some combination of states out.
        let combos: usize = d
            .rows
            .iter()
            .enumerate()
            .map(|(r, (_, ty))| {
                if d.cases.iter().any(|c| c.states[r].is_some()) {
                    domain
                        .sums
                        .iter()
                        .find(|s| s.name == *ty)
                        .map_or(1, |s| s.variants.len())
                } else {
                    1
                }
            })
            .product();
        // A guarded arm never counts as covering its states.
        let guarded = d.cases.iter().any(|c| !c.choice.is_empty());
        if guarded || d.cases.len() < combos {
            out.push_str(&format!(
                "        _ => {RESULT}::Err({error}::NotEnabled(NotEnabled {{ usecase: {} }})),\n",
                string_literal(&d.usecase)
            ));
        }
        out.push_str("    }\n}\n\n");
    }
}
