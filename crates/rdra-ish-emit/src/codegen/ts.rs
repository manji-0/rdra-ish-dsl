//! The TypeScript printer, in the Kamae shapes of docs/design/algebraic-codegen.md
//! ("Generated code style"): `type` aliases with `Readonly` properties, state unions
//! discriminated by `kind`, companion objects, branded scalars and ids made only by `parse`,
//! a self-contained `Result`, errors as values, and no `as` but `as const`.

use super::ir::{DomainModel, Field, FieldType, RecordKind, Scalar, SumDecl};
use super::{header_lines, obligation_lines, one_line, string_literal, Options};

/// What the printers below read of the model.
struct Cx<'a> {
    domain: &'a DomainModel,
    /// `--seal`: the state types carry the module's brand.
    seal: bool,
}

/// The property a sealed state has and only this module can write (`--seal`).
const SEAL: &str = "[$sealed]: true";

impl Cx<'_> {
    /// The property a column `wire` has on a value of type `ty`: its domain name
    /// (`Field.name`) on a state type, its name otherwise.
    fn prop_of(&self, ty: &str, wire: &str) -> String {
        for sum in &self.domain.sums {
            if sum.name == ty || sum.variants.iter().any(|v| v.name == ty) {
                return sum_prop(sum, wire);
            }
        }
        property(wire)
    }
}

/// The property of a field on the type that declares it: its domain name, which is the
/// column's except `kind_` for a column `kind` of a state type (`kind` tells the states apart).
fn dprop(field: &Field) -> String {
    property(&field.name)
}

/// The property of the column `wire` on the states of `sum`.
fn sum_prop(sum: &SumDecl, wire: &str) -> String {
    sum.variants
        .iter()
        .flat_map(|v| v.fields.iter().chain(&v.always_null))
        .find(|f| f.wire == wire)
        .map_or_else(|| property(wire), dprop)
}

pub(super) fn print(
    domain: &DomainModel,
    options: &Options,
    traces: Option<&super::conformance::Traces>,
) -> String {
    let cx = Cx {
        domain,
        seal: options.seal,
    };
    let mut head = String::new();
    for line in header_lines(options) {
        head.push_str(&comment(&line));
    }
    head.push_str("//\n");
    for line in obligation_lines(domain) {
        head.push_str(&comment(&line));
    }
    for line in super::conformance_lines(domain, traces, "conform", "ConformancePort") {
        head.push_str(&comment(&line));
    }
    head.push('\n');

    let mut out = String::new();
    print_results(&mut out);
    let mut body = String::new();
    print_scalars(domain, &mut body);
    print_records(&cx, &mut body);
    print_sums(&cx, &mut body);
    print_rows(&cx, &mut body);
    print_functions(&cx, &mut body);
    print_validators(&cx, &mut body);
    print_dispatchers(&cx, &mut body);
    super::ts_port::print_contracts(domain, &mut body);
    super::ts_port::print_port(domain, &mut body);
    out.push_str(&helpers(&body));
    out.push_str(&body);
    while out.ends_with("\n\n") {
        out.pop();
    }
    head + &out
}

pub(super) fn field_type(ty: &FieldType) -> String {
    match ty {
        FieldType::Scalar(s) => scalar(*s).to_string(),
        FieldType::Enum(name) | FieldType::Id(name) => name.clone(),
        FieldType::Rows(name) => format!("readonly {name}[]"),
    }
}

/// The type of a column, with `| null` when it is nullable.
pub(super) fn column_type(ty: &FieldType, nullable: bool) -> String {
    let ty = field_type(ty);
    if nullable {
        format!("{ty} | null")
    } else {
        ty
    }
}

fn scalar(s: Scalar) -> &'static str {
    match s {
        Scalar::Int => "number",
        Scalar::String => "string",
        Scalar::Bool => "boolean",
        other => other.name(),
    }
}

/// A property name as written: bare when it is an ASCII identifier, quoted otherwise.
pub(super) fn property(name: &str) -> String {
    let mut chars = name.chars();
    let plain = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if plain {
        name.to_string()
    } else {
        string_literal(name)
    }
}

/// A function name: camelCase of the PascalCase name, with a reserved word given a trailing `_`.
pub(super) fn fn_name(pascal: &str) -> String {
    binding(super::camel(pascal))
}

/// A parameter or local name: camelCase of the snake_case name, with a reserved word given
/// a trailing `_`.
pub(super) fn local(snake_name: &str) -> String {
    binding(super::camel(&super::type_ident(snake_name)))
}

/// A name a declaration binds: the words strict-mode TypeScript reserves there take a
/// trailing `_`. A property name needs no escape (see [`property`]).
pub(super) fn binding(name: String) -> String {
    const RESERVED: [&str; 46] = [
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "debugger",
        "default",
        "delete",
        "do",
        "else",
        "enum",
        "export",
        "extends",
        "false",
        "finally",
        "for",
        "function",
        "if",
        "import",
        "in",
        "instanceof",
        "new",
        "null",
        "return",
        "super",
        "switch",
        "this",
        "throw",
        "true",
        "try",
        "typeof",
        "var",
        "void",
        "while",
        "with",
        // Reserved in strict mode, which a module is.
        "implements",
        "interface",
        "let",
        "package",
        "private",
        "protected",
        "public",
        "static",
        "yield",
        "await",
    ];
    // Strict mode forbids binding these two, though neither is a keyword.
    if RESERVED.contains(&name.as_str()) || name == "arguments" || name == "eval" {
        format!("{name}_")
    } else {
        name
    }
}

fn comment(line: &str) -> String {
    if line.is_empty() {
        "//\n".to_string()
    } else {
        format!("// {line}\n")
    }
}

pub(super) fn doc(lines: &[String], indent: &str) -> String {
    let lines: Vec<String> = lines
        .iter()
        .map(|l| one_line(l).replace("*/", "* /"))
        .filter(|l| !l.is_empty())
        .collect();
    match lines.as_slice() {
        [] => String::new(),
        [one] => format!("{indent}/** {one} */\n"),
        many => {
            let mut out = format!("{indent}/**\n");
            for line in many {
                out.push_str(&format!("{indent} * {line}\n"));
            }
            out.push_str(&format!("{indent} */\n"));
            out
        }
    }
}

/// The cases of a union type, one per line, each with its trailing comment, the last ended by `;`.
pub(super) fn union(cases: impl Iterator<Item = (String, String)>) -> String {
    let cases: Vec<_> = cases.collect();
    let mut out = String::new();
    for (i, (case, comment)) in cases.iter().enumerate() {
        let end = if i + 1 == cases.len() { ";" } else { "" };
        out.push_str(&format!("  | {case}{end}{comment}\n"));
    }
    out
}

pub(super) fn doc1(line: &str) -> String {
    doc(&[line.to_string()], "")
}

/// `return { ok: false, error: <error> };` at `indent`.
fn fail(indent: &str, error: &str) -> String {
    format!("{indent}return {{ ok: false, error: {error} }};\n")
}

// ── Results and errors ───────────────────────────────────────────────────────

fn print_results(out: &mut String) {
    out.push_str("// ── Results and errors ──\n\n");
    out.push_str(&doc(
        &[
            "The outcome of a call that can fail: its value, or an error value. No generated function throws.".to_string(),
        ],
        "",
    ));
    out.push_str(
        "export type Result<T, E> = Readonly<{ ok: true; value: T }> | Readonly<{ ok: false; error: E }>;\n\n",
    );
    out.push_str(&doc1(
        "Makes a Result: `Result.ok(value)`, `Result.err(error)`.",
    ));
    out.push_str("export const Result = {\n");
    out.push_str("  ok: <T>(value: T): Result<T, never> => ({ ok: true, value }),\n");
    out.push_str("  err: <E>(error: E): Result<never, E> => ({ ok: false, error }),\n");
    out.push_str("} as const;\n\n");
    out.push_str(&doc(
        &[
            "Ends a `switch` that has a case for every value: the compiler fails it when a value has none.".to_string(),
            "It throws only if a value outside the type gets in, which the types rule out.".to_string(),
        ],
        "",
    ));
    out.push_str("export function assertNever(value: never): never {\n");
    out.push_str("  throw new globalThis.Error(`rdra-ish: a value outside its type: ${globalThis.JSON.stringify(value)}`);\n");
    out.push_str("}\n\n");
    out.push_str(&doc1(
        "A value that is not one of its type's: the type's name, and why.",
    ));
    out.push_str(
        "export type InvalidValue = Readonly<{ kind: \"InvalidValue\"; type: string; reason: string }>;\n\n",
    );
    out.push_str(&doc(
        &[
            "A row that is not one of its type's: the entity, the column (null when the value is no".to_string(),
            "object at all), and why, such as a column its state rules out.".to_string(),
        ],
        "",
    ));
    out.push_str(
        "export type InvalidRow = Readonly<{ kind: \"InvalidRow\"; entity: string; column: string | null; reason: string }>;\n\n",
    );
}

/// The private helpers `body` uses, so a strict `noUnusedLocals` build has none unused.
fn helpers(body: &str) -> String {
    const HELPERS: [(&str, &str); 7] = [
        (
            "$invalidValue",
            "function $invalidValue(type: string, reason: string): Result<never, InvalidValue> {\n  return { ok: false, error: { kind: \"InvalidValue\", type, reason } };\n}\n\n",
        ),
        (
            "$isObject",
            "function $isObject(value: unknown): value is Readonly<Record<string, unknown>> {\n  return typeof value === \"object\" && value !== null;\n}\n\n",
        ),
        (
            "$int",
            "function $int(value: unknown): Result<number, InvalidValue> {\n  return typeof value === \"number\" && globalThis.Number.isSafeInteger(value)\n    ? { ok: true, value }\n    : $invalidValue(\"Int\", \"not an integer\");\n}\n\n",
        ),
        (
            "$string",
            "function $string(value: unknown): Result<string, InvalidValue> {\n  return typeof value === \"string\" ? { ok: true, value } : $invalidValue(\"String\", \"not a string\");\n}\n\n",
        ),
        (
            "$boolean",
            "function $boolean(value: unknown): Result<boolean, InvalidValue> {\n  return typeof value === \"boolean\" ? { ok: true, value } : $invalidValue(\"Bool\", \"not a boolean\");\n}\n\n",
        ),
        (
            "$nullable",
            "function $nullable<T>(read: (value: unknown) => Result<T, InvalidValue>): (value: unknown) => Result<T | null, InvalidValue> {\n  return (value) => (value === null ? { ok: true, value: null } : read(value));\n}\n\n",
        ),
        (
            "$column",
            "function $column(entity: string, column: string, error: InvalidValue): Result<never, InvalidRow> {\n  return { ok: false, error: { kind: \"InvalidRow\", entity, column, reason: `${error.type}: ${error.reason}` } };\n}\n\n",
        ),
    ];
    let mut out = String::new();
    for (name, text) in HELPERS {
        // `$invalidValue` is also used by the helpers that read a column.
        let used = body.contains(name)
            || (name == "$invalidValue"
                && ["$int", "$string", "$boolean"]
                    .iter()
                    .any(|h| body.contains(h)));
        if used {
            out.push_str(text);
        }
    }
    if out.is_empty() {
        return out;
    }
    format!("// ── Reading values from outside (private) ──\n\n{out}")
}

// ── Scalars, ids, value sets ─────────────────────────────────────────────────

/// The shape check of a prelude scalar's text.
fn scalar_pattern(s: Scalar) -> (&'static str, &'static str) {
    match s {
        Scalar::Money | Scalar::Decimal => (
            "/^-?[0-9]+(\\.[0-9]+)?$/",
            "not decimal text such as \\\"1200.50\\\"",
        ),
        Scalar::Date => (
            "/^[0-9]{4}-[0-9]{2}-[0-9]{2}$/",
            "not a date such as \\\"2026-11-07\\\"",
        ),
        Scalar::DateTime => (
            "/^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}(:[0-9]{2}(\\.[0-9]+)?)?(Z|[+-][0-9]{2}:[0-9]{2})$/",
            "not a date and time with an offset such as \\\"2026-11-07T09:00:00+09:00\\\"",
        ),
        Scalar::Int | Scalar::String | Scalar::Bool => ("", ""),
    }
}

/// The test that `value` (an `unknown`) holds a value of the scalar `s`.
fn is_scalar(s: Scalar, value: &str) -> String {
    match s {
        Scalar::Int => {
            format!("typeof {value} === \"number\" && globalThis.Number.isSafeInteger({value})")
        }
        Scalar::String => format!("typeof {value} === \"string\""),
        Scalar::Bool => format!("typeof {value} === \"boolean\""),
        other => format!("{}.is({value})", other.name()),
    }
}

/// A companion object with `is` and `parse` for the branded type `name`.
fn branded_companion(name: &str, test: &str, reason: &str) -> String {
    let mut out = doc1(&format!(
        "Makes {name} values: `is` tells whether a value is one, `parse` returns it or why it is not."
    ));
    out.push_str(&format!("export const {name} = {{\n"));
    out.push_str(&format!(
        "  is: (value: unknown): value is {name} => {test},\n"
    ));
    out.push_str(&format!(
        "  parse: (value: unknown): Result<{name}, InvalidValue> =>\n    {name}.is(value) ? {{ ok: true, value }} : $invalidValue({}, \"{reason}\"),\n",
        string_literal(name)
    ));
    out.push_str("} as const;\n\n");
    out
}

fn print_scalars(domain: &DomainModel, out: &mut String) {
    out.push_str("// ── Scalars: replace with the project's own types if it has them ──\n\n");
    for s in Scalar::PRELUDE {
        out.push_str(&doc(&[s.default_doc().to_string()], ""));
        out.push_str(&format!(
            "export type {0} = string & {{ readonly __brand: \"{0}\" }};\n",
            s.name()
        ));
        let (pattern, reason) = scalar_pattern(s);
        out.push_str(&branded_companion(
            s.name(),
            &format!("typeof value === \"string\" && {pattern}.test(value)"),
            reason,
        ));
    }

    if !domain.ids.is_empty() {
        out.push_str("// ── Ids ──\n\n");
    }
    for id in &domain.ids {
        out.push_str(&doc1(&format!("The id of entity {}.", id.entity)));
        out.push_str(&format!(
            "export type {0} = {1} & {{ readonly __brand: \"{0}\" }};\n",
            id.name,
            scalar(id.repr)
        ));
        let reason = match id.repr {
            Scalar::Int => "not an integer".to_string(),
            Scalar::String => "not a string".to_string(),
            Scalar::Bool => "not a boolean".to_string(),
            other => format!("not a {}", other.name()),
        };
        out.push_str(&branded_companion(
            &id.name,
            &is_scalar(id.repr, "value"),
            &reason,
        ));
    }

    if !domain.enums.is_empty() {
        out.push_str("// ── Value sets ──\n\n");
    }
    for decl in &domain.enums {
        out.push_str(&doc(&decl.doc, ""));
        let values: Vec<String> = decl
            .values
            .iter()
            .map(|v| string_literal(&v.wire))
            .collect();
        out.push_str(&format!("export type {} =\n", decl.name));
        out.push_str(&union(decl.values.iter().map(|value| {
            (
                string_literal(&value.wire),
                value
                    .label
                    .as_ref()
                    .map(|l| format!(" // {}", one_line(l)))
                    .unwrap_or_default(),
            )
        })));
        out.push_str(&doc1(&format!(
            "The values of {}, and `parse`, which returns one or why a value is none.",
            decl.name
        )));
        out.push_str(&format!("export const {} = {{\n", decl.name));
        out.push_str(&format!(
            "  values: [{}] as const satisfies readonly {}[],\n",
            values.join(", "),
            decl.name
        ));
        out.push_str(&format!(
            "  parse: (value: unknown): Result<{0}, InvalidValue> => {{\n    const found = {0}.values.find((v) => v === value);\n    return found === undefined ? $invalidValue({1}, \"not one of its values\") : {{ ok: true, value: found }};\n  }},\n",
            decl.name,
            string_literal(&decl.name)
        ));
        out.push_str("} as const;\n\n");
    }
}

// ── Records, states and rows ─────────────────────────────────────────────────

/// `name: type;` lines of a type's properties, each with its doc.
fn properties(fields: &[Field]) -> String {
    let mut out = String::new();
    for field in fields {
        out.push_str(&doc(&field.doc, "  "));
        out.push_str(&format!(
            "  {}: {};\n",
            dprop(field),
            column_type(&field.ty, field.nullable)
        ));
    }
    out
}

/// The reader of a column's value from outside: `(value: unknown) => Result<T, InvalidValue>`.
fn reader(ty: &FieldType, nullable: bool) -> String {
    let read = match ty {
        FieldType::Scalar(Scalar::Int) => "$int".to_string(),
        FieldType::Scalar(Scalar::String) => "$string".to_string(),
        FieldType::Scalar(Scalar::Bool) => "$boolean".to_string(),
        FieldType::Scalar(s) => format!("{}.parse", s.name()),
        FieldType::Enum(name) | FieldType::Id(name) => format!("{name}.parse"),
        FieldType::Rows(name) => format!("{name}.parse"),
    };
    if nullable {
        format!("$nullable({read})")
    } else {
        read
    }
}

/// `parse<Row>`: the name `derived_names` claimed for the parser of the row type `ty`.
pub(super) fn parser_name(domain: &DomainModel, ty: &str) -> String {
    fn_name(
        domain
            .derived(&format!("ts:parse:{ty}"))
            .unwrap_or(&format!("Parse{ty}")),
    )
}

/// `parse<Type>(value: unknown): Result<Type, InvalidRow>`: the boundary check of a row from a
/// store or the wire, each column read by its value type's parser.
fn row_parser(cx: &Cx<'_>, ty: &str, entity: &str, fields: &[Field]) -> String {
    let mut out = doc(
        &[format!(
            "Reads a {ty} from outside (a store, the wire): the row, or InvalidRow naming the first column that is not of its type."
        )],
        "",
    );
    let name = parser_name(cx.domain, ty);
    out.push_str(&format!(
        "export function {name}(value: unknown): Result<{ty}, InvalidRow> {{\n"
    ));
    out.push_str("  if (!$isObject(value)) {\n");
    out.push_str(&fail(
        "    ",
        &format!(
            "{{ kind: \"InvalidRow\", entity: {}, column: null, reason: \"not an object\" }}",
            string_literal(entity)
        ),
    ));
    out.push_str("  }\n");
    for f in fields {
        let var = format!("${}", f.wire);
        out.push_str(&format!(
            "  const {var} = {}(value[{}]);\n  if (!{var}.ok) {{\n    return $column({}, {}, {var}.error);\n  }}\n",
            reader(&f.ty, f.nullable),
            string_literal(&f.wire),
            string_literal(entity),
            string_literal(&f.wire),
        ));
    }
    out.push_str("  return {\n    ok: true,\n    value: {\n");
    for f in fields {
        out.push_str(&format!(
            "      {}: ${}.value,\n",
            property(&f.wire),
            f.wire
        ));
    }
    out.push_str("    },\n  };\n}\n\n");
    out
}

fn print_records(cx: &Cx<'_>, out: &mut String) {
    for kind in [RecordKind::Entity, RecordKind::Dto] {
        let records: Vec<_> = cx
            .domain
            .records
            .iter()
            .filter(|r| r.kind == kind)
            .collect();
        if records.is_empty() {
            continue;
        }
        out.push_str(match kind {
            RecordKind::Entity => "// ── Entities ──\n\n",
            RecordKind::Dto => "// ── DTOs ──\n\n",
        });
        for record in records {
            out.push_str(&doc(&record.doc, ""));
            out.push_str(&format!("export type {} = Readonly<{{\n", record.name));
            out.push_str(&properties(&record.fields));
            out.push_str("}>;\n\n");
            out.push_str(&row_parser(cx, &record.name, &record.id, &record.fields));
        }
    }
}

/// `$<sum>Tag`: the tag column's value of each state.
fn tag_table(sum: &SumDecl) -> String {
    format!("${}Tag", super::camel(&sum.name))
}

fn print_sums(cx: &Cx<'_>, out: &mut String) {
    if cx.domain.sums.is_empty() {
        return;
    }
    out.push_str("// ── Entities with states ──\n\n");
    if cx.seal {
        out.push_str(&doc(
            &[
                "The brand of the state types (`--seal`): the module does not export it, so code outside".to_string(),
                "cannot write a state as an object literal; only `fromRow` and the transition functions".to_string(),
                "make one. A type assertion (`as`) still gets around it.".to_string(),
            ],
            "",
        ));
        out.push_str("const $sealed: unique symbol = Symbol(\"rdra-ish state\");\n\n");
    }
    for sum in &cx.domain.sums {
        let kind = &sum.kind;
        out.push_str(&doc1(&format!(
            "The states of {}, one per value of its column {}.",
            sum.entity, sum.tag.wire
        )));
        out.push_str(&format!("export type {kind} =\n"));
        out.push_str(&union(
            sum.variants
                .iter()
                .map(|v| (string_literal(&v.value.ident), String::new())),
        ));
        out.push('\n');
        for variant in &sum.variants {
            out.push_str(&doc(&variant.doc, ""));
            out.push_str(&format!("export type {} = Readonly<{{\n", variant.name));
            out.push_str(&format!(
                "  kind: {};\n",
                string_literal(&variant.value.ident)
            ));
            if cx.seal {
                out.push_str(&format!("  {SEAL};\n"));
            }
            out.push_str(&properties(&variant.fields));
            for field in &variant.always_null {
                out.push_str(&doc(&field.doc, "  "));
                out.push_str(&format!("  {}: null;\n", dprop(field)));
            }
            out.push_str("}>;\n\n");
        }
        let mut lines = sum.doc.clone();
        lines.push("One of its states, told apart by `kind`.".to_string());
        out.push_str(&doc(&lines, ""));
        out.push_str(&format!("export type {} =\n", sum.name));
        out.push_str(&union(
            sum.variants.iter().map(|v| (v.name.clone(), String::new())),
        ));
        out.push('\n');

        let tag_ty = field_type(&sum.tag.ty);
        out.push_str(&format!("const {} = {{\n", tag_table(sum)));
        for v in &sum.variants {
            out.push_str(&format!(
                "  {}: {},\n",
                property(&v.value.ident),
                string_literal(&v.value.wire)
            ));
        }
        out.push_str(&format!(
            "}} as const satisfies Readonly<Record<{kind}, {tag_ty}>>;\n\n"
        ));

        out.push_str(&doc(
            &[
                format!(
                    "Maps a {0} between its state and its flat row {1}: `fromRow` returns the state, or",
                    sum.entity, sum.row
                ),
                "InvalidRow naming the column whose value its state rules out; `toRow` cannot fail."
                    .to_string(),
            ],
            "",
        ));
        out.push_str(&format!("export const {} = {{\n", sum.name));
        out.push_str(&format!(
            "  fromRow: (row: {}): Result<{}, InvalidRow> => {{\n    switch (row.{}) {{\n",
            sum.row,
            sum.name,
            property(&sum.tag.wire)
        ));
        for v in &sum.variants {
            out.push_str(&format!(
                "      case {}: {{\n",
                string_literal(&v.value.wire)
            ));
            let invalid = |column: &str, reason: String| {
                fail(
                    "          ",
                    &format!(
                        "{{ kind: \"InvalidRow\", entity: {}, column: {}, reason: {} }}",
                        string_literal(&sum.entity),
                        string_literal(column),
                        string_literal(&reason)
                    ),
                )
            };
            for f in &v.fields {
                let declared = sum.row_fields.iter().find(|r| r.wire == f.wire);
                if declared.is_some_and(|d| d.nullable) && !f.nullable {
                    out.push_str(&format!(
                        "        if (row.{} === null) {{\n{}        }}\n",
                        property(&f.wire),
                        invalid(&f.wire, format!("present in the state {}", v.value.wire))
                    ));
                }
            }
            for f in &v.always_null {
                out.push_str(&format!(
                    "        if (row.{} !== null) {{\n{}        }}\n",
                    property(&f.wire),
                    invalid(&f.wire, format!("null in the state {}", v.value.wire))
                ));
            }
            out.push_str("        return {\n          ok: true,\n          value: {\n");
            out.push_str(&format!(
                "            kind: {},\n",
                string_literal(&v.value.ident)
            ));
            if cx.seal {
                out.push_str(&format!("            {SEAL},\n"));
            }
            for f in &v.fields {
                out.push_str(&format!(
                    "            {}: row.{},\n",
                    dprop(f),
                    property(&f.wire)
                ));
            }
            for f in &v.always_null {
                out.push_str(&format!("            {}: null,\n", dprop(f)));
            }
            out.push_str("          },\n        };\n      }\n");
        }
        // A tag column marked `@null` holds null in no state.
        if sum
            .row_fields
            .iter()
            .any(|f| f.wire == sum.tag.wire && f.nullable)
        {
            out.push_str("      case null:\n");
            out.push_str(&fail(
                "        ",
                &format!(
                    "{{ kind: \"InvalidRow\", entity: {}, column: {}, reason: \"null, which is no state\" }}",
                    string_literal(&sum.entity),
                    string_literal(&sum.tag.wire)
                ),
            ));
        }
        out.push_str(&format!(
            "      default:\n        return assertNever(row.{});\n    }}\n  }},\n",
            property(&sum.tag.wire)
        ));
        out.push_str(&format!(
            "  toRow: (state: {}): {} => ({{\n",
            sum.name, sum.row
        ));
        for f in &sum.row_fields {
            if f.wire == sum.tag.wire {
                out.push_str(&format!(
                    "    {}: {}[state.kind],\n",
                    property(&f.wire),
                    tag_table(sum)
                ));
            } else {
                out.push_str(&format!(
                    "    {}: state.{},\n",
                    property(&f.wire),
                    sum_prop(sum, &f.wire)
                ));
            }
        }
        out.push_str("  }),\n} as const;\n\n");
    }
}

fn print_rows(cx: &Cx<'_>, out: &mut String) {
    if cx.domain.sums.is_empty() {
        return;
    }
    out.push_str("// ── Rows: the flat shape a store or the wire holds ──\n\n");
    for sum in &cx.domain.sums {
        out.push_str(&doc1(&format!(
            "Every column of {} as the data layer declares it, the tag column {} among them.",
            sum.entity, sum.tag.wire
        )));
        out.push_str(&format!("export type {} = Readonly<{{\n", sum.row));
        for field in &sum.row_fields {
            out.push_str(&format!(
                "  {}: {};\n",
                property(&field.wire),
                column_type(&field.ty, field.nullable)
            ));
        }
        out.push_str("}>;\n\n");
        out.push_str(&row_parser(cx, &sum.row, &sum.entity, &sum.row_fields));
    }
}

// ── Use cases ────────────────────────────────────────────────────────────────

/// `<UseCase>Error`, as the IR names it.
fn error_type(domain: &DomainModel, usecase: &str) -> String {
    domain
        .error_of(usecase)
        .expect("a use case with functions has an error type")
        .name
        .clone()
}

fn invalid_argument(indent: &str, message: &str) -> String {
    fail(
        indent,
        &format!(
            "{{ kind: \"InvalidArgument\", message: {} }}",
            string_literal(message)
        ),
    )
}

fn print_functions(cx: &Cx<'_>, out: &mut String) {
    use super::ir::{Adapt, ArgCheck, Assigned, OutKind};
    let domain = cx.domain;
    if !domain.events.is_empty() {
        out.push_str("// ── Events ──\n\n");
    }
    for event in &domain.events {
        out.push_str(&doc(&event.doc, ""));
        out.push_str(&format!(
            "export type {0} = Readonly<{{\n  kind: \"{0}\";\n",
            event.name
        ));
        for field in &event.fields {
            out.push_str(&format!(
                "  /** The {} row the event was raised on. */\n  {}: {};\n",
                field.entity,
                property(&field.name),
                field.id_ty
            ));
        }
        out.push_str("}>;\n\n");
    }
    if domain.fns.is_empty() {
        return;
    }
    if !domain.marks.is_empty() {
        out.push_str("// ── Run marks of use cases split into transactions ──\n\n");
    }
    for m in &domain.marks {
        out.push_str(&doc1(&format!(
            "The mark step {} of {} leaves on a {} row: the next step takes it. Store it with the row.",
            m.step, m.usecase, m.entity
        )));
        out.push_str(&format!(
            "export type {} = Readonly<{{\n  usecase: {};\n  step: {};\n  id: {};\n}}>;\n\n",
            m.name,
            string_literal(&m.usecase),
            m.step,
            m.id_ty
        ));
    }
    out.push_str("// ── Use cases ──\n\n");
    out.push_str(&doc1(
        "The rows are in states the use case does not run from. Only a dispatcher returns it.",
    ));
    out.push_str(
        "export type NotEnabled = Readonly<{ kind: \"NotEnabled\"; usecase: string }>;\n\n",
    );
    out.push_str(&doc1(
        "A condition of the use case did not hold: the use case does not run.",
    ));
    out.push_str("export type Rejected = Readonly<{ kind: \"Rejected\"; condition: string; label: string; site: string | null }>;\n\n");
    out.push_str(&doc(
        &[
            "The arguments break what the verified model keeps: a child row that does not name the".to_string(),
            "parent passed with it, a row that is not the event's, a column the target state keeps null".to_string(),
            "that holds a value. The model never reaches such rows; a caller that passes them has a bug.".to_string(),
        ],
        "",
    ));
    out.push_str(
        "export type InvalidArgument = Readonly<{ kind: \"InvalidArgument\"; message: string }>;\n\n",
    );
    out.push_str(&doc(
        &[
            "What a row a result writes must still be when the write commits, so the store can write it".to_string(),
            "conditionally and take a write that matches no row as NotEnabled (see the obligations).".to_string(),
        ],
        "",
    ));
    out.push_str("export type ExpectedSource =\n  | Readonly<{ kind: \"absent\" }>\n  | Readonly<{ kind: \"present\" }>\n  | Readonly<{ kind: \"in\"; column: string; value: string }>;\n\n");

    for e in &domain.errors {
        let mut cases = vec!["NotEnabled"];
        if e.rejects {
            cases.push("Rejected");
        }
        if e.invalid_argument {
            cases.push("InvalidArgument");
        }
        out.push_str(&doc1(&format!(
            "What {} can fail with: NotEnabled from a dispatcher{}{}.",
            e.usecase,
            if e.rejects {
                ", Rejected by a condition"
            } else {
                ""
            },
            if e.invalid_argument {
                ", InvalidArgument for rows the model never reaches"
            } else {
                ""
            }
        )));
        out.push_str(&format!(
            "export type {} = {};\n\n",
            e.name,
            cases.join(" | ")
        ));
    }

    for f in &domain.fns {
        // The result type.
        out.push_str(&doc1(&format!("What {} returns.", fn_name(&f.name))));
        out.push_str(&format!("export type {} = Readonly<{{\n", f.result));
        for o in &f.outputs {
            match &o.kind {
                OutKind::Row { ty, .. } => {
                    out.push_str(&format!("  {}: {ty};\n", property(&o.name)));
                }
                OutKind::Deleted {
                    id: Some((ty, _)), ..
                } => {
                    out.push_str(&format!("  {}: {ty};\n", property(&o.name)));
                }
                OutKind::Deleted { id: None, .. } => {}
            }
        }
        for o in &f.outputs {
            out.push_str(&format!(
                "  {}: ExpectedSource;\n",
                property(&format!("{}_expected", o.name))
            ));
        }
        for m in &f.marks_out {
            out.push_str(&format!("  {}: {};\n", property(&m.name), m.ty));
        }
        for event in &f.raises {
            out.push_str(&format!(
                "  {}: {};\n",
                property(&super::actions::snake(&event.ty)),
                event.ty
            ));
        }
        out.push_str("}>;\n\n");

        // The function.
        let mut lines = f.doc.clone();
        for c in &f.conditions {
            lines.push(format!(
                "Requires the condition {} ({}){}; Rejected when it does not hold.",
                c.id,
                one_line(&c.label),
                c.site
                    .as_ref()
                    .map(|s| format!(" at {s}"))
                    .unwrap_or_default()
            ));
        }
        out.push_str(&doc(&lines, ""));
        let mut params: Vec<(String, String)> = f
            .rows
            .iter()
            .map(|r| (local(&r.name), r.ty.clone()))
            .collect();
        for m in &f.marks_in {
            params.push((local(&m.name), m.ty.clone()));
        }
        if let Some(ev) = &f.event_in {
            params.push((local(&ev.name), ev.ty.clone()));
        }
        for input in &f.inputs {
            params.push((local(&input.name), column_type(&input.ty, input.nullable)));
        }
        // The body comes first, so a parameter it does not read can be marked unused (`_`).
        let body_at = out.len();
        let row = |r: usize| local(&f.row(r).name);
        let read = |r: usize, field: &Field| format!("{}.{}", row(r), dprop(field));
        let input = |i: usize| local(&f.inputs[i].name);
        for check in &f.checks {
            match check {
                ArgCheck::Link {
                    child,
                    child_field,
                    parent,
                    parent_field,
                    message,
                } => {
                    out.push_str(&format!(
                        "  if ({} !== {}) {{\n{}  }}\n",
                        read(*child, child_field),
                        read(*parent, parent_field),
                        invalid_argument("    ", message)
                    ));
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
                        .expect("an event row check has an event");
                    out.push_str(&format!(
                        "  if ({} !== {}.{}) {{\n{}  }}\n",
                        read(*r, field),
                        local(&ev.name),
                        property(event_field),
                        invalid_argument("    ", message)
                    ));
                }
                ArgCheck::MarkRow {
                    row: r,
                    field,
                    mark,
                    message,
                } => {
                    out.push_str(&format!(
                        "  if ({} !== {}.id) {{\n{}  }}\n",
                        read(*r, field),
                        local(&f.marks_in[*mark].name),
                        invalid_argument("    ", message)
                    ));
                }
                ArgCheck::MustBeNull {
                    row: r,
                    field,
                    message,
                } => {
                    out.push_str(&format!(
                        "  if ({} !== null) {{\n{}  }}\n",
                        read(*r, field),
                        invalid_argument("    ", message)
                    ));
                }
            }
        }
        for c in &f.conditions {
            out.push_str(&format!(
                "  if (!({})) {{\n{}  }}\n",
                expr(&c.expr, &row, &input),
                fail(
                    "    ",
                    &format!(
                        "{{ kind: \"Rejected\", condition: {}, label: {}, site: {} }}",
                        string_literal(&c.id),
                        string_literal(&c.label),
                        c.site
                            .as_ref()
                            .map_or("null".to_string(), |s| string_literal(s))
                    )
                )
            ));
        }
        // What the verified model keeps of a written column, checked before the result is
        // built from it: a nullable source it keeps present, a second lifecycle column in a
        // state some arm moves it from.
        for o in &f.outputs {
            let OutKind::Row { fields, .. } = &o.kind else {
                continue;
            };
            for (_, assigned) in fields {
                match assigned {
                    Assigned::From {
                        row: r,
                        field,
                        adapt: Adapt::Unwrap(message),
                    } => out.push_str(&format!(
                        "  if ({} === null) {{\n{}  }}\n",
                        read(*r, field),
                        invalid_argument("    ", message)
                    )),
                    Assigned::Switch {
                        row: r,
                        field,
                        arms,
                        enum_ty,
                    } if arms.len()
                        < domain
                            .enums
                            .iter()
                            .find(|e| e.name == *enum_ty)
                            .map_or(0, |e| e.values.len()) =>
                    {
                        let value = read(*r, field);
                        let from: Vec<String> = arms
                            .iter()
                            .map(|(from, _)| format!("{value} === {}", string_literal(&from.wire)))
                            .collect();
                        out.push_str(&format!(
                            "  if (!({})) {{\n{}  }}\n",
                            from.join(" || "),
                            invalid_argument(
                                "    ",
                                &format!(
                                    "rdra-ish: {} is in no state {} moves it from",
                                    field.wire, f.usecase
                                )
                            )
                        ));
                    }
                    _ => {}
                }
            }
        }
        out.push_str("  return {\n    ok: true,\n    value: {\n");
        for o in &f.outputs {
            match &o.kind {
                OutKind::Row {
                    ty,
                    tag,
                    fields,
                    always_null,
                } => {
                    out.push_str(&format!("      {}: {{\n", property(&o.name)));
                    if let Some((_, value)) = tag {
                        out.push_str(&format!(
                            "        kind: {},\n",
                            string_literal(&value.ident)
                        ));
                        if cx.seal {
                            out.push_str(&format!("        {SEAL},\n"));
                        }
                    }
                    for (target, assigned) in fields {
                        let v = match assigned {
                            Assigned::From {
                                row: r,
                                field,
                                adapt,
                            } => match adapt {
                                Adapt::Same | Adapt::Wrap | Adapt::Unwrap(_) => read(*r, field),
                                Adapt::AbsentNull => "null".to_string(),
                            },
                            Assigned::Lit { lit, .. } => lit_ts(lit),
                            Assigned::Input { input: i, .. } => input(*i),
                            Assigned::ParentId { row: r, field, .. } => read(*r, field),
                            Assigned::Switch {
                                row: r,
                                field,
                                arms,
                                ..
                            } => {
                                // The check above leaves only the arms' sources: the last
                                // arm needs no test.
                                let value = read(*r, field);
                                let mut arms = arms.iter().rev();
                                let (_, last) = arms.next().expect("a switch has an arm");
                                let mut v = string_literal(&last.wire);
                                for (from, to) in arms {
                                    v = format!(
                                        "{value} === {} ? {} : {v}",
                                        string_literal(&from.wire),
                                        string_literal(&to.wire)
                                    );
                                }
                                format!("({v})")
                            }
                            Assigned::EventId { field, .. } => format!(
                                "{}.{}",
                                local(&f.event_in.as_ref().expect("an event id has an event").name),
                                property(field)
                            ),
                            Assigned::MarkId { mark, .. } => {
                                format!("{}.id", local(&f.marks_in[*mark].name))
                            }
                            Assigned::Null => "null".to_string(),
                            Assigned::Derived { expr: e, .. } => expr(e, &row, &input),
                        };
                        out.push_str(&format!("        {}: {v},\n", dprop(target)));
                    }
                    for wire in always_null {
                        out.push_str(&format!("        {}: null,\n", cx.prop_of(ty, wire)));
                    }
                    out.push_str("      },\n");
                }
                OutKind::Deleted {
                    row: r,
                    id: Some((_, field)),
                } => {
                    out.push_str(&format!(
                        "      {}: {},\n",
                        property(&o.name),
                        read(*r, field)
                    ));
                }
                OutKind::Deleted { id: None, .. } => {}
            }
        }
        for o in &f.outputs {
            let expected = match f.expected(o) {
                super::ir::Expected::Absent => "{ kind: \"absent\" }".to_string(),
                super::ir::Expected::Present => "{ kind: \"present\" }".to_string(),
                super::ir::Expected::In { column, value } => format!(
                    "{{ kind: \"in\", column: {}, value: {} }}",
                    string_literal(&column),
                    string_literal(&value)
                ),
            };
            out.push_str(&format!(
                "      {}: {expected},\n",
                property(&format!("{}_expected", o.name))
            ));
        }
        let source = |s: &super::ir::MarkSource| match s {
            super::ir::MarkSource::Row { row: r, field } => read(*r, field),
            super::ir::MarkSource::Input(i) => input(*i),
            super::ir::MarkSource::Mark(k) => format!("{}.id", local(&f.marks_in[*k].name)),
        };
        for m in &f.marks_out {
            out.push_str(&format!(
                "      {}: {{ usecase: {}, step: {}, id: {} }},\n",
                property(&m.name),
                string_literal(&f.usecase),
                m.step,
                source(&m.source)
            ));
        }
        for event in &f.raises {
            let mut parts = vec![format!("kind: \"{}\"", event.ty)];
            for (field, s) in &event.ids {
                parts.push(format!("{}: {}", property(&field.name), source(s)));
            }
            out.push_str(&format!(
                "      {}: {{ {} }},\n",
                property(&super::actions::snake(&event.ty)),
                parts.join(", ")
            ));
        }
        out.push_str("    },\n  };\n}\n\n");
        let body = out.split_off(body_at);
        let params: Vec<String> = params
            .iter()
            .map(|(name, ty)| {
                let used = if mentions(&body, name) { "" } else { "_" };
                format!("{used}{name}: {ty}")
            })
            .collect();
        out.push_str(&format!(
            "export function {}({}): Result<{}, {}> {{\n",
            fn_name(&f.name),
            params.join(", "),
            f.result,
            error_type(domain, &f.usecase)
        ));
        out.push_str(&body);
    }
}

/// Whether `code` reads the identifier `name`: an occurrence not inside a longer identifier
/// and not a property name after a `.`.
fn mentions(code: &str, name: &str) -> bool {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
    code.match_indices(name).any(|(at, _)| {
        let before = code[..at].chars().next_back();
        let after = code[at + name.len()..].chars().next();
        !before.is_some_and(|c| ident(c) || c == '.') && !after.is_some_and(ident)
    })
}

fn lit_ts(lit: &super::ir::Lit) -> String {
    match lit {
        super::ir::Lit::Enum { value, .. } => string_literal(&value.wire),
        super::ir::Lit::Bool(b) => b.to_string(),
        super::ir::Lit::Int(n) => n.to_string(),
    }
}

/// `e` as a TypeScript expression. `row` gives a row's name and type.
fn expr(
    e: &super::ir::Expr,
    row: &dyn Fn(usize) -> String,
    input: &dyn Fn(usize) -> String,
) -> String {
    use super::ir::{CmpOp, Expr};
    let read = |r: usize, field: &Field| format!("{}.{}", row(r), dprop(field));
    let cmp = |op: &CmpOp| match op {
        CmpOp::Eq => "===",
        CmpOp::Ne => "!==",
        other => other.symbol(),
    };
    match e {
        Expr::True => "true".to_string(),
        Expr::False => "false".to_string(),
        Expr::Not(inner) => format!("!({})", expr(inner, row, input)),
        Expr::And(items) if items.is_empty() => "true".to_string(),
        Expr::Or(items) if items.is_empty() => "false".to_string(),
        Expr::And(items) => items
            .iter()
            .map(|i| format!("({})", expr(i, row, input)))
            .collect::<Vec<_>>()
            .join(" && "),
        Expr::Or(items) => items
            .iter()
            .map(|i| format!("({})", expr(i, row, input)))
            .collect::<Vec<_>>()
            .join(" || "),
        Expr::IsNull { row: r, field } => format!("{} === null", read(*r, field)),
        Expr::EnumEq {
            row: r,
            field,
            value,
            ..
        } => format!("{} === {}", read(*r, field), string_literal(&value.wire)),
        Expr::BoolEq {
            row: r,
            field,
            value,
        } => format!("{} === {value}", read(*r, field)),
        Expr::IntCmp {
            row: r,
            field,
            op,
            value,
        } => format!("{} {} {value}", read(*r, field), cmp(op)),
        Expr::InputIsNull { input: i } => format!("{} === null", input(*i)),
        Expr::InputEnumEq {
            input: i, value, ..
        } => format!("{} === {}", input(*i), string_literal(&value.wire)),
        Expr::InputBoolEq {
            input: i, value, ..
        } => format!("{} === {value}", input(*i)),
        Expr::KeyEq {
            child,
            child_field,
            parent,
            parent_field,
        } => format!(
            "{} === {}",
            read(*child, child_field),
            read(*parent, parent_field)
        ),
        Expr::Exists {
            input: i,
            elem,
            body,
        } => format!(
            "{}.some(({}) => {})",
            input(*i),
            row(*elem),
            expr(body, row, input)
        ),
        Expr::Forall {
            input: i,
            elem,
            body,
        } => format!(
            "{}.every(({}) => {})",
            input(*i),
            row(*elem),
            expr(body, row, input)
        ),
        Expr::Count {
            input: i,
            elem,
            body,
            op,
            n,
        } => format!(
            "{}.filter(({}) => {}).length {} {n}",
            input(*i),
            row(*elem),
            expr(body, row, input),
            cmp(op)
        ),
    }
}

// ── Rule checks ──────────────────────────────────────────────────────────────

fn print_validators(cx: &Cx<'_>, out: &mut String) {
    let domain = cx.domain;
    if domain.validators.is_empty() {
        return;
    }
    out.push_str("// ── Rule checks ──\n\n");
    for v in &domain.validators {
        out.push_str(&doc1(&format!(
            "The rules a {} row breaks, by the names `check` reports: empty when it keeps them all.",
            v.ty
        )));
        let name = format!("{}RuleViolations", super::camel(&v.ty));
        if v.rules.is_empty() {
            out.push_str(&format!(
                "export function {name}(_row: {}): readonly string[] {{\n  return [];\n}}\n\n",
                v.ty
            ));
            continue;
        }
        out.push_str(&format!(
            "export function {name}(row: {}): readonly string[] {{\n  const out: string[] = [];\n",
            v.ty
        ));
        let row = |_: usize| "row".to_string();
        for rule in &v.rules {
            out.push_str(&format!(
                "  if ({}) {{\n    out.push({}); // {}\n  }}\n",
                expr(&fails(&rule.expr), &row, &|_| String::new()),
                string_literal(&rule.name),
                one_line(&rule.text)
            ));
        }
        out.push_str("  return out;\n}\n\n");
    }
    for sum in &domain.sums {
        out.push_str(&doc1(&format!(
            "The rules a {} row breaks, by its state: empty when it keeps them all.",
            sum.name
        )));
        out.push_str(&format!(
            "export function {}RuleViolations(row: {}): readonly string[] {{\n  const kind = row.kind;\n  switch (kind) {{\n",
            super::camel(&sum.name),
            sum.name,
        ));
        for variant in &sum.variants {
            out.push_str(&format!(
                "    case {}:\n      return {}RuleViolations(row);\n",
                string_literal(&variant.value.ident),
                super::camel(&variant.name)
            ));
        }
        out.push_str("    default:\n      return assertNever(kind);\n  }\n}\n\n");
    }
}

/// The condition under which `e` fails, folded so that a negated comparison reads plainly.
fn fails(e: &super::ir::Expr) -> super::ir::Expr {
    super::actions::simplify(super::ir::Expr::Not(Box::new(e.clone())))
}

// ── Dispatch ─────────────────────────────────────────────────────────────────

fn print_dispatchers(cx: &Cx<'_>, out: &mut String) {
    let domain = cx.domain;
    if domain.dispatchers.is_empty() {
        return;
    }
    out.push_str("// ── Dispatch: run a use case from whatever states its rows are in ──\n\n");
    for d in &domain.dispatchers {
        out.push_str(&doc1(&format!(
            "What {} returns when the use case runs.",
            super::camel(&d.name)
        )));
        out.push_str(&format!(
            "export type {} = {};\n\n",
            d.outcome,
            d.cases
                .iter()
                .map(|c| c.result.clone())
                .collect::<Vec<_>>()
                .join(" | ")
        ));
        out.push_str(&doc1(&format!(
            "Runs {} from whatever states its rows are in: the transition function of those states, or NotEnabled.",
            d.usecase
        )));
        let mut params: Vec<String> = d
            .rows
            .iter()
            .map(|(n, ty)| format!("{}: {ty}", local(n)))
            .collect();
        for (n, ty) in &d.marks {
            params.push(format!("{}: {ty}", local(n)));
        }
        for i in &d.inputs {
            params.push(format!(
                "{}: {}",
                local(&i.name),
                column_type(&i.ty, i.nullable)
            ));
        }
        for (n, ty) in &d.choices {
            params.push(format!("{}: {ty}", local(n)));
        }
        out.push_str(&format!(
            "export function {}({}): Result<{}, {}> {{\n",
            super::camel(&d.name),
            params.join(", "),
            d.outcome,
            error_type(domain, &d.usecase)
        ));
        for c in &d.cases {
            let tests: Vec<String> = c
                .states
                .iter()
                .zip(&d.rows)
                .filter_map(|(s, (n, _))| {
                    let (_, _, value) = s.as_ref()?;
                    Some(format!(
                        "{}.kind === {}",
                        local(n),
                        string_literal(&value.ident)
                    ))
                })
                .chain(c.choice.iter().map(|(i, v)| {
                    format!(
                        "{} === {}",
                        local(&d.choices[*i].0),
                        string_literal(&v.wire)
                    )
                }))
                .collect();
            let mut args: Vec<String> = d.rows.iter().map(|(n, _)| local(n)).collect();
            args.extend(d.marks.iter().map(|(n, _)| local(n)));
            args.extend(c.inputs.iter().map(|i| local(&d.inputs[*i].name)));
            let call = format!("{}({})", fn_name(&c.function), args.join(", "));
            if tests.is_empty() {
                out.push_str(&format!("  return {call};\n"));
            } else {
                out.push_str(&format!(
                    "  if ({}) {{\n    return {call};\n  }}\n",
                    tests.join(" && ")
                ));
            }
        }
        out.push_str(&fail(
            "  ",
            &format!(
                "{{ kind: \"NotEnabled\", usecase: {} }}",
                string_literal(&d.usecase)
            ),
        ));
        out.push_str("}\n\n");
    }
}
