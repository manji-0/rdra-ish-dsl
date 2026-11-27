//! `types.rs`, `contract.rs`, `core.rs` and `port.rs`: the Rust code of the ordinary output,
//! for the subset `--verus` covers, in a form Verus reads, with its contracts.
//!
//! What differs from the ordinary output, and why (docs/design/verus.md):
//! - The functions are the methods of the trait `contract.rs` declares, so a core that weakens
//!   what it promises does not implement it, and the error names the method.
//! - Fields are public, not sealed: Verus lets a public spec read only public fields, and the
//!   trait and the port are public. The port's proof, not a seal, keeps a state honest: it
//!   holds for every value a core returns, however it was made.
//! - A quantifier over related rows is a `while` loop with its invariant, not an iterator
//!   chain with a closure; an enum test is `matches!`; a result is wrapped by `match`, not
//!   `map(Constructor)`.
//! - `Display`, `Error`, `From` and the row conversions stay plain Rust, outside `verus!`.

use std::cell::RefCell;

use rdra_ish_core::lifecycle::{Action, At, CmpOp, GuardWhy, ParamRole, Pred, Term};

use super::model::{act_name, call_variant, Atom, Atoms, Env, Q};
use super::{Ctx, Ent, MTy};
use crate::codegen::ir::{
    Assigned, DispatchDecl, Expected, FieldType, FnDecl, InputType, Lit, OutKind, Scalar,
};
use crate::codegen::rust::{fn_ident, ident};
use crate::codegen::string_literal;

pub(super) struct Printed {
    pub types: String,
    pub contract: String,
    pub core: String,
    pub port: String,
}

/// An argument of a function or a dispatcher that holds the row of action param `k`.
#[derive(Clone)]
enum Arg {
    /// A state value (`LoanOpen`): its tag is fixed.
    Variant {
        name: String,
        ty: String,
        tag: String,
    },
    /// A state of any of its states (`Loan`), in a dispatcher.
    Sum { name: String },
    /// A record (an entity with no states), which is its own row type.
    Record { name: String },
    /// A created row: the input holding its id.
    New { input: String },
}

/// What a function (or a dispatcher) reads: per action param its argument, and per entity a
/// quantifier ranges over, the list input.
struct Sig<'a> {
    action: &'a Action,
    args: Vec<Option<Arg>>,
    lists: Vec<(rdra_ish_core::lifecycle::EntId, String)>,
}

impl Ctx<'_> {
    fn action(&self, name: &str) -> &Action {
        self.ts
            .actions()
            .iter()
            .find(|a| a.name == name)
            .expect("a function prints an action")
    }

    fn ent_of_row(&self, entity: &str) -> &Ent {
        self.ent_by_id(entity).expect("a row of a covered entity")
    }

    /// The signature of `f`: its rows by action param, and its lists by entity.
    fn sig_of(&self, f: &FnDecl) -> Result<Sig<'_>, String> {
        let action = self.action(&f.action);
        let mut args: Vec<Option<Arg>> = vec![None; action.params.len()];
        for r in &f.rows {
            let Some(k) = r.param else { continue };
            let e = self.ent_of_row(&r.entity);
            args[k] = Some(match &e.sum {
                Some(s) => {
                    let (_, tag) =
                        s.variants.iter().find(|(v, _)| *v == r.ty).ok_or_else(|| {
                            format!("{}: {} is no state of {}", f.name, r.ty, e.id)
                        })?;
                    Arg::Variant {
                        name: ident(&r.name),
                        ty: r.ty.clone(),
                        tag: tag.clone(),
                    }
                }
                None => Arg::Record {
                    name: ident(&r.name),
                },
            });
        }
        for o in &f.outputs {
            if let OutKind::Row { fields, .. } = &o.kind {
                for (field, a) in fields {
                    if field.wire == "id" {
                        if let Assigned::Input { input, .. } = a {
                            args[o.param] = Some(Arg::New {
                                input: ident(&f.inputs[*input].name),
                            });
                        }
                    }
                }
            }
        }
        let mut lists = Vec::new();
        for i in &f.inputs {
            if let InputType::Rows(ty) = &i.ty {
                let e = self
                    .ents
                    .iter()
                    .find(|e| e.row == *ty)
                    .ok_or_else(|| format!("{}: a list of {ty} is not covered", f.name))?;
                lists.push((e.eid, ident(&i.name)));
            }
        }
        Ok(Sig {
            action,
            args,
            lists,
        })
    }

    /// The same signature over a dispatcher's rows (states of any state, or records).
    fn sig_of_dispatch(&self, first: &FnDecl) -> Result<Sig<'_>, String> {
        let mut sig = self.sig_of(first)?;
        for a in sig.args.iter_mut() {
            if let Some(Arg::Variant { name, .. }) = a {
                let name = name.clone();
                *a = Some(Arg::Sum { name });
            }
        }
        Ok(sig)
    }

    /// The spec reads of a signature: per param, its ghost row and its id.
    fn spec_row(&self, sig: &Sig, k: u8) -> (String, String) {
        let e = self.ent(sig.action.params[usize::from(k)].entity);
        match &sig.args[usize::from(k)] {
            Some(Arg::Variant { name, ty, .. }) => {
                (format!("view_{ty}({name})"), format!("{name}.id.0 as int"))
            }
            Some(Arg::Sum { name }) => (
                format!("{}_view({name})", e.snake),
                format!("{}_id({name})", e.snake),
            ),
            Some(Arg::Record { name }) => (
                format!("{}_row_view({name})", e.snake),
                format!("{name}.id.0 as int"),
            ),
            Some(Arg::New { input }) => (String::new(), format!("{input}.0 as int")),
            None => (String::new(), String::new()),
        }
    }

    /// The guards of the action on a signature's arguments, as spec.
    fn spec_guard(
        &self,
        sig: &Sig,
        atoms: &Atoms,
        seen: &RefCell<Vec<(Atom, Vec<String>)>>,
    ) -> Result<String, String> {
        let param = |k: u8| self.spec_row(sig, k);
        let lists = sig.lists.clone();
        let quant = move |a: &Atom, values: &[String]| -> String {
            seen.borrow_mut().push((a.clone(), values.to_vec()));
            let list = lists
                .iter()
                .find(|(e, _)| *e == a.ent)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| "MISSING_LIST".to_string());
            let args = std::iter::once(list.to_string())
                .chain(values.iter().cloned())
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}_rows({args})", a.name)
        };
        let env = Env {
            param: &param,
            bound: Vec::new(),
            subst: Vec::new(),
            quant: &quant,
        };
        let mut parts = Vec::new();
        for g in &sig.action.guards {
            parts.push(self.pred(&g.pred, &env, atoms)?);
        }
        let text = if parts.is_empty() {
            "true".to_string()
        } else {
            parts.join("\n        && ")
        };
        if text.contains("MISSING_LIST") {
            return Err(format!(
                "{}: a quantifier over rows the function does not take",
                sig.action.name
            ));
        }
        Ok(text)
    }

    /// What the result must hold, on a signature's arguments: each written row is the row
    /// after the action, each deleted row is named, and each expected source is the model's.
    fn spec_effects(&self, sig: &Sig, f: &FnDecl, res: &str) -> String {
        let a = sig.action;
        let id = |j: u8| self.spec_row(sig, j).1;
        let mut parts = Vec::new();
        for o in &f.outputs {
            let k = o.param as u8;
            match &o.kind {
                OutKind::Row { ty, .. } => {
                    let pre = match a.params[o.param].role {
                        ParamRole::New => None,
                        _ => Some(self.spec_row(sig, k).0),
                    };
                    let e = self.ent(a.params[o.param].entity);
                    let view = if e.sum.is_some() {
                        format!("view_{ty}({res}.{})", ident(&o.name))
                    } else {
                        format!("{}_row_view({res}.{})", e.snake, ident(&o.name))
                    };
                    parts.push(format!("{res}.{}.id.0 as int == {}", ident(&o.name), id(k)));
                    parts.push(format!(
                        "{view} == {}",
                        self.row_after(a, k, pre.as_deref(), &id)
                    ));
                }
                OutKind::Deleted { .. } => {
                    parts.push(format!("{res}.{}.0 as int == {}", ident(&o.name), id(k)));
                }
            }
            parts.push(format!(
                "{res}.{} == {}",
                ident(&o.expected_name()),
                expected(&f.expected(o))
            ));
        }
        if parts.is_empty() {
            "true".to_string()
        } else {
            parts.join("\n            && ")
        }
    }
}

fn expected(e: &Expected) -> String {
    match e {
        Expected::Absent => "ExpectedSource::Absent".to_string(),
        Expected::Present => "ExpectedSource::Present".to_string(),
        Expected::In { column, value } => format!(
            "(ExpectedSource::In {{ column: {}, value: {} }})",
            string_literal(column),
            string_literal(value)
        ),
    }
}

fn field_ty(ty: &FieldType) -> String {
    match ty {
        FieldType::Id(t) | FieldType::Enum(t) => t.clone(),
        FieldType::Scalar(Scalar::Bool) => "bool".to_string(),
        FieldType::Scalar(s) => s.name().to_string(),
    }
}

/// The exec signature of a function: `name: Type` per row, then per input.
fn exec_params(f: &FnDecl) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = f
        .rows
        .iter()
        .map(|r| (ident(&r.name), r.ty.clone()))
        .collect();
    for i in &f.inputs {
        let ty = match &i.ty {
            InputType::Value(t) => field_ty(t),
            InputType::Rows(t) => format!("&[{t}]"),
        };
        out.push((ident(&i.name), ty));
    }
    out
}

fn spec_param_ty(ty: &str) -> String {
    match ty.strip_prefix("&[").and_then(|t| t.strip_suffix(']')) {
        Some(inner) => format!("Seq<{inner}>"),
        None => ty.to_string(),
    }
}

fn spec_arg(name: &str, ty: &str) -> String {
    if ty.starts_with("&[") {
        format!("{name}@")
    } else {
        name.to_string()
    }
}

pub(super) fn print(ctx: &Ctx, atoms: &Atoms) -> Result<Printed, Vec<String>> {
    let mut why = Vec::new();
    let types = types(ctx, &mut why);
    let mut contract = String::from(
        "use vstd::prelude::*;\nuse crate::types::*;\nuse crate::prelude::*;\nuse crate::model::*;\n\nverus! {\n\n",
    );
    contract.push_str(&views(ctx));
    let mut trait_items = Vec::new();
    let mut core_items = Vec::new();
    for f in &ctx.domain.fns {
        match function(ctx, f, atoms) {
            Ok((spec, decl, body)) => {
                contract.push_str(&spec);
                trait_items.push(decl);
                core_items.push(body);
            }
            Err(w) => why.push(w),
        }
    }
    for d in &ctx.domain.dispatchers {
        match dispatcher(ctx, d, atoms) {
            Ok((spec, decl, body)) => {
                contract.push_str(&spec);
                trait_items.push(decl);
                core_items.push(body);
            }
            Err(w) => why.push(w),
        }
    }
    let core_trait = core_trait_name(ctx);
    contract.push_str(&format!(
        "/// The functions of the core, each with what it must do. `core.rs` implements it; the\n/// port calls only these.\npub trait {core_trait} {{\n{}}}\n\n}} // verus!\n",
        trait_items.join("\n")
    ));
    let core = format!(
        "use vstd::prelude::*;\nuse crate::types::*;\nuse crate::prelude::*;\nuse crate::model::*;\nuse crate::contract::*;\n\nverus! {{\n\n/// The core: the use cases' functions. Edit them freely; Verus checks each against its\n/// contract in contract.rs.\npub struct Core;\n\nimpl {core_trait} for Core {{\n{}}}\n\n}} // verus!\n",
        core_items.join("\n")
    );
    let port = match port(ctx, atoms, &core_trait) {
        Ok(p) => p,
        Err(mut w) => {
            why.append(&mut w);
            String::new()
        }
    };
    if !why.is_empty() {
        return Err(why);
    }
    Ok(Printed {
        types,
        contract,
        core,
        port,
    })
}

pub(super) fn core_trait_name(_ctx: &Ctx) -> String {
    "DomainCore".to_string()
}

// ── types.rs ─────────────────────────────────────────────────────────────────

fn types(ctx: &Ctx, why: &mut Vec<String>) -> String {
    let d = ctx.domain;
    let mut v = String::from("use vstd::prelude::*;\n\nverus! {\n\n");
    let mut plain = String::new();
    for id in &d.ids {
        if ctx.ent_by_id(&id.entity).is_none() {
            continue;
        }
        if id.repr != Scalar::Int {
            why.push(format!(
                "{}: an id that is not an Int is not covered",
                id.entity
            ));
            continue;
        }
        v.push_str(&format!(
            "/// The id of a {}.\n#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub struct {0}(pub i64);\n\nimpl {0} {{\n    /// The value it holds.\n    pub fn get(self) -> (r: i64)\n        ensures r == self.0\n    {{\n        self.0\n    }}\n}}\n\n",
            id.name
        ).replacen(&format!("The id of a {}.", id.name), &format!("The id of a {}.", id.entity), 1));
        plain.push_str(&format!(
            "impl ::core::convert::From<i64> for {0} {{\n    fn from(value: i64) -> Self {{\n        Self(value)\n    }}\n}}\n\nimpl ::core::fmt::Display for {0} {{\n    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {{\n        write!(f, \"{{}}\", self.0)\n    }}\n}}\n\n",
            id.name
        ));
    }
    let mut used_enums: Vec<String> = Vec::new();
    for e in &ctx.ents {
        let mut fields = Vec::new();
        if let Some(s) = d.sums.iter().find(|s| s.entity == e.id) {
            fields.extend(s.row_fields.iter().cloned());
        }
        if let Some(r) = d.records.iter().find(|r| r.id == e.id) {
            fields.extend(r.fields.iter().cloned());
        }
        for f in fields {
            if let FieldType::Enum(t) = f.ty {
                if !used_enums.contains(&t) {
                    used_enums.push(t);
                }
            }
        }
    }
    for en in d.enums.iter().filter(|en| used_enums.contains(&en.name)) {
        v.push_str(&format!(
            "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum {} {{\n{}}}\n\n",
            en.name,
            en.values
                .iter()
                .map(|x| format!("    {},\n", x.ident))
                .collect::<String>()
        ));
        plain.push_str(&format!(
            "impl {0} {{\n    /// The value as the model spells it.\n    pub fn as_str(self) -> &'static str {{\n        match self {{\n{1}        }}\n    }}\n\n    /// The value the model spells `s`.\n    pub fn parse(s: &str) -> ::core::option::Option<Self> {{\n        match s {{\n{2}            _ => ::core::option::Option::None,\n        }}\n    }}\n}}\n\n",
            en.name,
            en.values
                .iter()
                .map(|x| format!("            Self::{} => {},\n", x.ident, string_literal(&x.wire)))
                .collect::<String>(),
            en.values
                .iter()
                .map(|x| format!("            {} => ::core::option::Option::Some(Self::{}),\n", string_literal(&x.wire), x.ident))
                .collect::<String>()
        ));
    }
    for e in &ctx.ents {
        if let Some(s) = d.sums.iter().find(|s| s.entity == e.id) {
            for var in &s.variants {
                v.push_str(&format!(
                    "/// {} in the state `{}`. Its fields are public: the port's proof, not a seal, keeps what\n/// a state holds honest (docs/design/verus.md).\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct {} {{\n{}}}\n\n",
                    e.id,
                    var.value.wire,
                    var.name,
                    var.fields
                        .iter()
                        .map(|f| format!("    pub {}: {},\n", ident(&f.name), field_ty(&f.ty)))
                        .collect::<String>()
                ));
                plain.push_str(&format!(
                    "impl {} {{\n{}}}\n\n",
                    var.name,
                    var.fields
                        .iter()
                        .map(|f| format!(
                            "    /// The column `{}`.\n    pub fn {}(&self) -> &{} {{\n        &self.{}\n    }}\n",
                            f.wire,
                            ident(&f.name),
                            field_ty(&f.ty),
                            ident(&f.name)
                        ))
                        .collect::<String>()
                ));
            }
            v.push_str(&format!(
                "/// A {}, in one of its states.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub enum {} {{\n{}}}\n\n",
                e.id,
                s.name,
                s.variants
                    .iter()
                    .map(|x| format!("    {}({}),\n", x.value.ident, x.name))
                    .collect::<String>()
            ));
            v.push_str(&format!(
                "/// Every column of {} as a store holds it.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct {} {{\n{}}}\n\n",
                e.id,
                s.row,
                s.row_fields
                    .iter()
                    .map(|f| format!("    pub {}: {},\n", ident(&f.name), field_ty(&f.ty)))
                    .collect::<String>()
            ));
            // from_row / into_row: plain Rust.
            let tag = ident(&s.tag.name);
            let mut from = String::new();
            let mut into = String::new();
            for var in &s.variants {
                let inits: Vec<String> = var
                    .fields
                    .iter()
                    .map(|f| format!("{0}: row.{0}", ident(&f.name)))
                    .collect();
                from.push_str(&format!(
                    "            {}::{} => Self::{}({} {{ {} }}),\n",
                    field_ty(&s.tag.ty),
                    var.value.ident,
                    var.value.ident,
                    var.name,
                    inits.join(", ")
                ));
                let outs: Vec<String> = var
                    .fields
                    .iter()
                    .map(|f| format!("{0}: x.{0}", ident(&f.name)))
                    .collect();
                into.push_str(&format!(
                    "            Self::{}(x) => {} {{ {tag}: {}::{}, {} }},\n",
                    var.value.ident,
                    s.row,
                    field_ty(&s.tag.ty),
                    var.value.ident,
                    outs.join(", ")
                ));
            }
            for var in &s.variants {
                plain.push_str(&format!(
                    "impl ::core::convert::From<{}> for {} {{\n    fn from(state: {}) -> Self {{\n        Self::{}(state)\n    }}\n}}\n\n",
                    var.name, s.name, var.name, var.value.ident
                ));
            }
            plain.push_str(&format!(
                "impl {0} {{\n    /// The state of a row.\n    pub fn from_row(row: {1}) -> Self {{\n        match row.{tag} {{\n{from}        }}\n    }}\n\n    /// The flat row of this state.\n    pub fn into_row(self) -> {1} {{\n        match self {{\n{into}        }}\n    }}\n}}\n\n",
                s.name, s.row
            ));
        } else if let Some(r) = d.records.iter().find(|r| r.id == e.id) {
            v.push_str(&format!(
                "/// A {}, which has no states: its own row type.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct {} {{\n{}}}\n\n",
                e.id,
                r.name,
                r.fields
                    .iter()
                    .map(|f| format!("    pub {}: {},\n", ident(&f.name), field_ty(&f.ty)))
                    .collect::<String>()
            ));
        }
    }
    v.push_str(
        "/// The rows are in states the use case does not run from, or a commit found a row changed.\n\
         #[verifier::allow(autoderive_clone_without_spec)]\n\
         #[derive(Debug, Clone, PartialEq, Eq)]\npub struct NotEnabled {\n    pub usecase: &'static str,\n}\n\n\
         /// A condition applied to the use case does not hold.\n\
         #[verifier::allow(autoderive_clone_without_spec)]\n\
         #[derive(Debug, Clone, PartialEq, Eq)]\npub struct Rejected {\n    pub condition: &'static str,\n    pub label: &'static str,\n    pub site: Option<&'static str>,\n}\n\n\
         /// The arguments break what the verified model keeps.\n\
         #[verifier::allow(autoderive_clone_without_spec)]\n\
         #[derive(Debug, Clone, PartialEq, Eq)]\npub struct InvalidArgument {\n    pub usecase: &'static str,\n    pub reason: &'static str,\n}\n\n\
         /// What a written row must still be when the write commits.\n\
         #[verifier::allow(autoderive_clone_without_spec)]\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum ExpectedSource {\n    Absent,\n    Present,\n    In { column: &'static str, value: &'static str },\n}\n\n",
    );
    for err in &d.errors {
        let mut cases = vec!["NotEnabled"];
        if err.rejects {
            cases.push("Rejected");
        }
        if err.invalid_argument {
            cases.push("InvalidArgument");
        }
        v.push_str(&format!(
            "/// Why {} did not run.\n#[verifier::allow(autoderive_clone_without_spec)]\n#[derive(Debug, Clone, PartialEq, Eq)]\npub enum {} {{\n{}}}\n\n",
            err.usecase,
            err.name,
            cases
                .iter()
                .map(|c| format!("    {c}({c}),\n"))
                .collect::<String>()
        ));
        plain.push_str(&format!(
            "impl ::core::fmt::Display for {} {{\n    fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {{\n        write!(f, \"{{self:?}}\")\n    }}\n}}\n\nimpl ::std::error::Error for {} {{}}\n\n",
            err.name, err.name
        ));
    }
    for f in &d.fns {
        let mut fields = Vec::new();
        for o in &f.outputs {
            let ty = match &o.kind {
                OutKind::Row { ty, .. } => ty.clone(),
                OutKind::Deleted {
                    id: Some((t, _)), ..
                } => t.clone(),
                OutKind::Deleted { id: None, .. } => continue,
            };
            fields.push(format!("    pub {}: {ty},\n", ident(&o.name)));
        }
        for o in &f.outputs {
            fields.push(format!(
                "    pub {}: ExpectedSource,\n",
                ident(&o.expected_name())
            ));
        }
        v.push_str(&format!(
            "/// What `{}` returns.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct {} {{\n{}}}\n\n",
            fn_ident(&f.name),
            f.result,
            fields.concat()
        ));
    }
    for dsp in &d.dispatchers {
        v.push_str(&format!(
            "/// What `{}` returns when the use case runs.\n#[derive(Debug, Clone, PartialEq, Eq)]\npub enum {} {{\n{}}}\n\n",
            fn_ident(&dsp.name),
            dsp.outcome,
            dsp.cases
                .iter()
                .map(|c| format!("    {}({}),\n", c.case, c.result))
                .collect::<String>()
        ));
    }
    v.push_str("} // verus!\n\n// Plain Rust, outside `verus!`: Verus compiles it unverified.\n\n");
    v.push_str(&plain);
    v
}

// ── contract.rs: views ───────────────────────────────────────────────────────

fn views(ctx: &Ctx) -> String {
    let mut out = String::from("// ── What a state value stands for in the spec ──\n\n");
    for e in &ctx.ents {
        let Some(s) = &e.sum else { continue };
        let mut arms_view = String::new();
        let mut arms_id = String::new();
        for (vname, tag_ident) in &s.variants {
            let fields: Vec<String> = e
                .fields
                .iter()
                .map(|f| {
                    if f.tag {
                        format!("{}: {}", f.name, tag_value(f, tag_ident))
                    } else {
                        match f.ty {
                            MTy::Key { .. } => format!("{0}: x.{0}.0 as int", f.name),
                            _ => format!("{0}: x.{0}", f.name),
                        }
                    }
                })
                .collect();
            out.push_str(&format!(
                "pub open spec fn view_{vname}(x: {vname}) -> {} {{\n    {} {{ {} }}\n}}\n\n",
                e.m,
                e.m,
                fields.join(", ")
            ));
            arms_view.push_str(&format!(
                "        {}::{tag_ident}(x) => view_{vname}(x),\n",
                s.name
            ));
            arms_id.push_str(&format!(
                "        {}::{tag_ident}(x) => x.id.0 as int,\n",
                s.name
            ));
        }
        out.push_str(&format!(
            "pub open spec fn {0}_view(x: {1}) -> {2} {{\n    match x {{\n{arms_view}    }}\n}}\n\npub open spec fn {0}_id(x: {1}) -> int {{\n    match x {{\n{arms_id}    }}\n}}\n\n",
            e.snake, s.name, e.m
        ));
    }
    out
}

fn tag_value(f: &super::MField, ident_: &str) -> String {
    match &f.ty {
        MTy::Enum { ty, .. } => format!("{ty}::{ident_}"),
        _ => unreachable!("a tag is an enum"),
    }
}

// ── contract.rs and core.rs: one function ────────────────────────────────────

/// The spec of `f`, its trait declaration, and its body in the core.
fn function(ctx: &Ctx, f: &FnDecl, atoms: &Atoms) -> Result<(String, String, String), String> {
    let sig = ctx.sig_of(f)?;
    let name = fn_ident(&f.name);
    let params = exec_params(f);
    let error = ctx.domain.error_name(&f.usecase).to_string();
    let seen = RefCell::new(Vec::new());
    let guard = ctx.spec_guard(&sig, atoms, &seen)?;
    let effects = ctx.spec_effects(&sig, f, "res");
    let spec_params: Vec<String> = params
        .iter()
        .map(|(n, t)| format!("{n}: {}", spec_param_ty(t)))
        .collect();
    let spec_args: Vec<String> = params.iter().map(|(n, t)| spec_arg(n, t)).collect();
    let spec = format!(
        "/// What `{name}` must do: run exactly when {} runs on its rows, and write what it writes.\npub open spec fn {name}_spec({}, r: Result<{}, {error}>) -> bool {{\n    match r {{\n        Ok(res) => {guard_g}\n            && {effects},\n        Err(_) => !({guard_g}),\n    }}\n}}\n\n",
        sig.action.name,
        spec_params.join(", "),
        f.result,
        guard_g = format!("({guard})"),
    );
    let exec_sig: Vec<String> = params.iter().map(|(n, t)| format!("{n}: {t}")).collect();
    let decl = format!(
        "    fn {name}({}) -> (r: Result<{}, {error}>)\n        ensures {name}_spec({}, r);\n",
        exec_sig.join(", "),
        f.result,
        spec_args.join(", ")
    );
    let body = exec_body(ctx, f, &sig, &seen.borrow(), atoms)?;
    let body = format!(
        "    fn {name}({}) -> (r: Result<{}, {error}>) {{\n{body}    }}\n",
        exec_sig.join(", "),
        f.result
    );
    Ok((spec, decl, body))
}

/// An exec value: an expression and what it is.
#[derive(Clone)]
enum XTy {
    Enum,
    Bool,
    I64,
}

/// An exec term: a term, read on the row a quantifier binds (or none), as exec code.
type XTerm<'a> = dyn Fn(&Term, Option<(&str, usize)>) -> Result<(String, XTy), String> + 'a;

/// What a port call runs: the signature, the core method, its arguments `(name, type)`, and
/// per outcome case `(pattern, function)`.
type PortCall<'a> = (
    Sig<'a>,
    String,
    Vec<(String, String)>,
    Vec<(String, String)>,
);

/// The body of `f`: a loop per quantifier, a check per guard, then the result.
fn exec_body(
    ctx: &Ctx,
    f: &FnDecl,
    sig: &Sig,
    seen: &[(Atom, Vec<String>)],
    _atoms: &Atoms,
) -> Result<String, String> {
    let a = sig.action;
    let mut out = String::new();
    // Exec values of the guards' terms outside quantifiers.
    let xterm = |t: &Term, inner: Option<(&str, usize)>| -> Result<(String, XTy), String> {
        let row = |at: &At| -> Result<(String, Option<Arg>), String> {
            match at {
                At::Param(k) => Ok((String::new(), sig.args[usize::from(*k)].clone())),
                At::Bound(j) => match inner {
                    Some((r, d)) if usize::from(*j) == d => Ok((r.to_string(), None)),
                    _ => Err("a bound row out of scope".to_string()),
                },
                At::Next(_) => Err("a condition on the successor".to_string()),
            }
        };
        match t {
            Term::Slot { var, at } => {
                let (r, arg) = row(at)?;
                let (_, fld) = ctx.field(*var);
                let base = match (&arg, r.is_empty()) {
                    (_, false) => r,
                    (Some(Arg::Variant { name, .. }), _) | (Some(Arg::Record { name }), _) => {
                        name.clone()
                    }
                    _ => return Err("a column of a row the function does not take".to_string()),
                };
                if fld.tag {
                    if let Some(Arg::Variant { tag, .. }) = &arg {
                        return Ok((tag_value(fld, tag), XTy::Enum));
                    }
                }
                Ok(match fld.ty {
                    MTy::Enum { .. } => (format!("{base}.{}", ident(&fld.name)), XTy::Enum),
                    MTy::Bool => (format!("{base}.{}", ident(&fld.name)), XTy::Bool),
                    MTy::Key { .. } => (format!("{base}.{}.0", ident(&fld.name)), XTy::I64),
                })
            }
            Term::Index(at) => {
                let (r, arg) = row(at)?;
                Ok(match (arg, r.is_empty()) {
                    (_, false) => (format!("{r}.id.0"), XTy::I64),
                    (Some(Arg::New { input }), _) => (format!("{input}.0"), XTy::I64),
                    (Some(Arg::Variant { name, .. }), _)
                    | (Some(Arg::Record { name }), _)
                    | (Some(Arg::Sum { name }), _) => (format!("{name}.id.0"), XTy::I64),
                    _ => return Err("the id of a row the function does not take".to_string()),
                })
            }
            _ => Err("Int values and `now` are not covered".to_string()),
        }
    };
    let atom_locals: RefCell<usize> = RefCell::new(0);
    fn xpred(
        ctx: &Ctx,
        p: &Pred,
        xterm: &XTerm,
        inner: Option<(&str, usize)>,
        subst: &[(Term, String)],
        quant: &dyn Fn() -> String,
    ) -> Result<String, String> {
        let term =
            |t: &Term| -> Result<(String, XTy, Option<rdra_ish_core::lifecycle::VarId>), String> {
                if let Some((_, s)) = subst.iter().find(|(x, _)| x == t) {
                    let (_, ty) = xterm(t, inner).unwrap_or((String::new(), XTy::I64));
                    let var = match t {
                        Term::Slot { var, .. } => Some(*var),
                        _ => None,
                    };
                    return Ok((s.clone(), ty, var));
                }
                if let Term::Lit(_) = t {
                    return Ok((String::new(), XTy::Enum, None));
                }
                let (s, ty) = xterm(t, inner)?;
                let var = match t {
                    Term::Slot { var, .. } => Some(*var),
                    _ => None,
                };
                Ok((s, ty, var))
            };
        let cmp = |a: &Term, op: CmpOp, b: &Term| -> Result<String, String> {
            let neg = match op {
                CmpOp::Eq => false,
                CmpOp::Ne => true,
                _ => return Err("an ordering comparison is not covered".to_string()),
            };
            let text = match (a, b) {
                (x, Term::Lit(v)) | (Term::Lit(v), x) => {
                    let (s, ty, var) = term(x)?;
                    let var = var.ok_or("a value compared with a literal")?;
                    match ty {
                        XTy::Enum => format!("matches!({s}, {})", ctx.lit(var, *v)),
                        XTy::Bool => format!("{s} == {}", ctx.lit(var, *v)),
                        XTy::I64 => return Err("a key compared with a literal".to_string()),
                    }
                }
                (x, y) => {
                    let (s, tx, _) = term(x)?;
                    let (u, _, _) = term(y)?;
                    match tx {
                        XTy::I64 | XTy::Bool => format!("{s} == {u}"),
                        XTy::Enum => {
                            return Err("two enum columns compared in a function".to_string())
                        }
                    }
                }
            };
            Ok(if neg {
                format!("!({text})")
            } else {
                format!("({text})")
            })
        };
        Ok(match p {
            Pred::True => "true".into(),
            Pred::False => "false".into(),
            Pred::Cmp(a, op, b) => cmp(a, *op, b)?,
            Pred::In(t, vs) => {
                let parts: Result<Vec<String>, String> =
                    vs.iter().map(|v| cmp(t, CmpOp::Eq, v)).collect();
                format!("({})", parts?.join(" || "))
            }
            Pred::Not(x) => format!("!{}", xpred(ctx, x, xterm, inner, subst, quant)?),
            Pred::And(ps) | Pred::Or(ps) => {
                let sep = if matches!(p, Pred::And(_)) {
                    " && "
                } else {
                    " || "
                };
                if ps.is_empty() {
                    if matches!(p, Pred::And(_)) {
                        "true".into()
                    } else {
                        "false".into()
                    }
                } else {
                    let parts: Result<Vec<String>, String> = ps
                        .iter()
                        .map(|x| xpred(ctx, x, xterm, inner, subst, quant))
                        .collect();
                    format!("({})", parts?.join(sep))
                }
            }
            Pred::Count { op, n, .. } => {
                let op = match op {
                    CmpOp::Eq => "==",
                    CmpOp::Ne => "!=",
                    CmpOp::Lt => "<",
                    CmpOp::Le => "<=",
                    CmpOp::Gt => ">",
                    CmpOp::Ge => ">=",
                };
                format!("({} {op} {n})", quant())
            }
            Pred::Forall { .. } | Pred::Exists { .. } => quant(),
        })
    }
    // One loop per quantifier, in the order the spec met them.
    for (i, (atom, spec_values)) in seen.iter().enumerate() {
        let list = sig
            .lists
            .iter()
            .find(|(e, _)| *e == atom.ent)
            .map(|(_, n)| n.clone())
            .ok_or_else(|| format!("{}: a quantifier over rows it does not take", f.name))?;
        let e = ctx.ent(atom.ent);
        let mut subst = Vec::new();
        let mut spec_frees = String::new();
        for ((t, _), sv) in atom.frees.iter().zip(spec_values) {
            let (x, _) = xterm(t, None)?;
            subst.push((t.clone(), x));
            spec_frees.push_str(&format!(", {sv}"));
        }
        let row = format!("{list}[i]");
        let body = xpred(
            ctx,
            &atom.pred,
            &xterm,
            Some((&row, atom.depth)),
            &subst,
            &|| "UNSUPPORTED".to_string(),
        )?;
        if body.contains("UNSUPPORTED") {
            return Err(format!("{}: a quantifier inside a quantifier", f.name));
        }
        let body_spec = format!(
            "{}_body({}_row_view({list}@[j]), {list}@[j].id.0 as int{spec_frees})",
            atom.name, e.snake
        );
        let local = format!("q{i}");
        let at_i = body_spec.replace("@[j]", "@[i as int]");
        // The row's test, bound once, and said to be the spec's body on that row.
        let mention = format!(
            "            let b = {body};\n            proof {{\n                assert(b == {at_i});\n            }}\n"
        );
        let text = match atom.kind {
            Q::Forall => format!(
                "        let mut {local} = true;\n        let mut i: usize = 0;\n        while i < {list}.len()\n            invariant\n                i <= {list}.len(),\n                {local} == (forall|j: int| 0 <= j < i ==> #[trigger] {body_spec}),\n            decreases {list}.len() - i,\n        {{\n{mention}            if !b {{\n                {local} = false;\n            }}\n            i += 1;\n        }}\n"
            ),
            Q::Exists => format!(
                "        let mut {local} = false;\n        let mut i: usize = 0;\n        while i < {list}.len()\n            invariant\n                i <= {list}.len(),\n                {local} == (exists|j: int| 0 <= j < i && #[trigger] {body_spec}),\n            decreases {list}.len() - i,\n        {{\n{mention}            if b {{\n                {local} = true;\n            }}\n            i += 1;\n        }}\n"
            ),
            Q::Count => format!(
                "        let mut {local}: usize = 0;\n        let mut i: usize = 0;\n        while i < {list}.len()\n            invariant\n                i <= {list}.len(),\n                {local} == {name}_rows({list}@.subrange(0, i as int){spec_frees}),\n                {local} <= i,\n            decreases {list}.len() - i,\n        {{\n            proof {{\n                assert({list}@.subrange(0, i as int + 1).drop_last() =~= {list}@.subrange(0, i as int));\n            }}\n{mention}            if b {{\n                {local} += 1;\n            }}\n            i += 1;\n        }}\n        proof {{\n            assert({list}@.subrange(0, {list}@.len() as int) =~= {list}@);\n        }}\n",
                name = atom.name
            ),
        };
        out.push_str(&text);
    }
    // One check per guard, the quantifiers read from their loops.
    let usecase = string_literal(&f.usecase);
    let error = ctx.domain.error_name(&f.usecase);
    for g in &a.guards {
        let next_local = || {
            let mut n = atom_locals.borrow_mut();
            let s = format!("q{n}");
            *n += 1;
            s
        };
        let text = xpred(ctx, &g.pred, &xterm, None, &[], &next_local)?;
        let fail = match g.why {
            GuardWhy::TransitionFrom => continue,
            GuardWhy::Owner => {
                // The function's types already keep it (no error to report): the contract
                // still asks for it, and Verus proves it from the types.
                if !ctx
                    .domain
                    .error_of(&f.usecase)
                    .is_some_and(|e| e.invalid_argument)
                {
                    continue;
                }
                format!(
                    "{error}::InvalidArgument(InvalidArgument {{ usecase: {usecase}, reason: \"a row does not name the row it belongs to\" }})"
                )
            }
            GuardWhy::Condition { cond, .. } => {
                let c = &ctx.model.conditions[cond];
                let site = f
                    .conditions
                    .iter()
                    .find(|x| x.id == c.id)
                    .and_then(|x| x.site.clone());
                if !ctx.domain.error_of(&f.usecase).is_some_and(|e| e.rejects) {
                    continue;
                }
                format!(
                    "{error}::Rejected(Rejected {{ condition: {}, label: {}, site: {} }})",
                    string_literal(&c.id),
                    string_literal(&crate::codegen::one_line(&c.label)),
                    match site {
                        Some(s) => format!("Some({})", string_literal(&s)),
                        None => "None".to_string(),
                    }
                )
            }
            _ => return Err(format!("{}: a guard that is not covered", f.name)),
        };
        out.push_str(&format!(
            "        if !{text} {{\n            return Err({fail});\n        }}\n"
        ));
    }
    // The result.
    let row_name = |r: usize| ident(&f.row(r).name);
    let mut fields = Vec::new();
    for o in &f.outputs {
        match &o.kind {
            OutKind::Row { ty, fields: fs, .. } => {
                let mut inits = Vec::new();
                for (field, assigned) in fs {
                    let value = match assigned {
                        Assigned::From {
                            row,
                            field: src,
                            adapt,
                        } if *adapt == crate::codegen::ir::Adapt::Same => {
                            format!("{}.{}", row_name(*row), ident(&src.name))
                        }
                        Assigned::Lit { lit, wrap: false } => match lit {
                            Lit::Enum { ty, value } => format!("{ty}::{}", value.ident),
                            Lit::Bool(b) => b.to_string(),
                            Lit::Int(_) => {
                                return Err(format!("{}: an Int value is not covered", f.name))
                            }
                        },
                        Assigned::Input { input, wrap: false } => ident(&f.inputs[*input].name),
                        Assigned::ParentId {
                            row,
                            field: src,
                            wrap: false,
                        } => {
                            format!("{}.{}", row_name(*row), ident(&src.name))
                        }
                        _ => {
                            return Err(format!(
                                "{}: the value of {} is not covered",
                                f.name, field.wire
                            ))
                        }
                    };
                    inits.push(format!("{}: {value}", ident(&field.name)));
                }
                fields.push(format!(
                    "            {}: {ty} {{ {} }},\n",
                    ident(&o.name),
                    inits.join(", ")
                ));
            }
            OutKind::Deleted {
                row,
                id: Some((_, field)),
            } => {
                fields.push(format!(
                    "            {}: {}.{},\n",
                    ident(&o.name),
                    row_name(*row),
                    ident(&field.name)
                ));
            }
            OutKind::Deleted { id: None, .. } => {
                return Err(format!("{}: a deleted row with no id", f.name))
            }
        }
    }
    for o in &f.outputs {
        let ex = match f.expected(o) {
            Expected::Absent => "ExpectedSource::Absent".to_string(),
            Expected::Present => "ExpectedSource::Present".to_string(),
            Expected::In { column, value } => format!(
                "ExpectedSource::In {{ column: {}, value: {} }}",
                string_literal(&column),
                string_literal(&value)
            ),
        };
        fields.push(format!(
            "            {}: {ex},\n",
            ident(&o.expected_name())
        ));
    }
    out.push_str(&format!(
        "        Ok({} {{\n{}        }})\n",
        f.result,
        fields.concat()
    ));
    Ok(out)
}

// ── contract.rs and core.rs: one dispatcher ──────────────────────────────────

fn dispatcher(
    ctx: &Ctx,
    d: &DispatchDecl,
    atoms: &Atoms,
) -> Result<(String, String, String), String> {
    let fns: Vec<&FnDecl> = d
        .cases
        .iter()
        .map(|c| {
            ctx.domain
                .fns
                .iter()
                .find(|f| f.name == c.function)
                .ok_or_else(|| format!("{}: no function {}", d.name, c.function))
        })
        .collect::<Result<_, _>>()?;
    let first = fns[0];
    let sig = ctx.sig_of_dispatch(first)?;
    let name = fn_ident(&d.name);
    let error = ctx.domain.error_name(&d.usecase).to_string();
    let mut params: Vec<(String, String)> =
        d.rows.iter().map(|(n, t)| (ident(n), t.clone())).collect();
    for i in &d.inputs {
        let ty = match &i.ty {
            InputType::Value(t) => field_ty(t),
            InputType::Rows(t) => format!("&[{t}]"),
        };
        params.push((ident(&i.name), ty));
    }
    let seen = RefCell::new(Vec::new());
    let guard = ctx.spec_guard(&sig, atoms, &seen)?;
    let mut cases = String::new();
    for (c, f) in d.cases.iter().zip(&fns) {
        let fsig = Sig {
            action: sig.action,
            args: sig.args.clone(),
            lists: sig.lists.clone(),
        };
        // The case is the one of the rows' states.
        let mut states = Vec::new();
        for ((n, _), st) in d.rows.iter().zip(&c.states) {
            if let Some((_, _, value)) = st {
                let k = sig
                    .args
                    .iter()
                    .position(|a| matches!(a, Some(Arg::Sum { name }) if *name == ident(n)));
                if let Some(k) = k {
                    let e = ctx.ent(sig.action.params[k].entity);
                    if let Some(tag) = e.fields.iter().find(|f| f.tag) {
                        states.push(format!(
                            "{}_view({}).{} == {}",
                            e.snake,
                            ident(n),
                            tag.name,
                            tag_value(tag, &value.ident)
                        ));
                    }
                }
            }
        }
        states.push(ctx.spec_effects(&fsig, f, "res"));
        cases.push_str(&format!(
            "            {}::{}(res) => {},\n",
            d.outcome,
            c.case,
            states.join("\n            && ")
        ));
    }
    let spec_params: Vec<String> = params
        .iter()
        .map(|(n, t)| format!("{n}: {}", spec_param_ty(t)))
        .collect();
    let spec_args: Vec<String> = params.iter().map(|(n, t)| spec_arg(n, t)).collect();
    let spec = format!(
        "/// What `{name}` must do: run exactly when {} runs on its rows, whatever states they are in, and write what it writes.\npub open spec fn {name}_spec({}, r: Result<{}, {error}>) -> bool {{\n    match r {{\n        Ok(o) => ({guard})\n            && match o {{\n{cases}            }},\n        Err(_) => !({guard}),\n    }}\n}}\n\n",
        sig.action.name,
        spec_params.join(", "),
        d.outcome,
    );
    let exec_sig: Vec<String> = params.iter().map(|(n, t)| format!("{n}: {t}")).collect();
    let decl = format!(
        "    fn {name}({}) -> (r: Result<{}, {error}>)\n        ensures {name}_spec({}, r);\n",
        exec_sig.join(", "),
        d.outcome,
        spec_args.join(", ")
    );
    // The body: match the states, call the case's function.
    let scrut: Vec<String> = d.rows.iter().map(|(n, _)| ident(n)).collect();
    let mut arms = String::new();
    for c in &d.cases {
        let pats: Vec<String> = d
            .rows
            .iter()
            .zip(&c.states)
            .map(|((n, ty), st)| match st {
                Some((_, _, value)) => format!("{ty}::{}({})", value.ident, ident(n)),
                None => ident(n),
            })
            .collect();
        let mut args: Vec<String> = d.rows.iter().map(|(n, _)| ident(n)).collect();
        for i in &c.inputs {
            args.push(ident(&d.inputs[*i].name));
        }
        arms.push_str(&format!(
            "            ({},) => match Self::{}({}) {{\n                Ok(res) => Ok({}::{}(res)),\n                Err(e) => Err(e),\n            }},\n",
            pats.join(", "),
            fn_ident(&c.function),
            args.join(", "),
            d.outcome,
            c.case
        ));
    }
    let all_cover = d.cases.len() == 1 && d.cases[0].states.iter().all(|s| s.is_none());
    if !all_cover {
        arms.push_str(&format!(
            "            _ => Err({error}::NotEnabled(NotEnabled {{ usecase: {} }})),\n",
            string_literal(&d.usecase)
        ));
    }
    let body = format!(
        "    fn {name}({}) -> (r: Result<{}, {error}>) {{\n        match ({},) {{\n{arms}        }}\n    }}\n",
        exec_sig.join(", "),
        d.outcome,
        scrut.join(", ")
    );
    Ok((spec, decl, body))
}

// ── port.rs ──────────────────────────────────────────────────────────────────

fn port(ctx: &Ctx, atoms: &Atoms, core_trait: &str) -> Result<String, Vec<String>> {
    let mut why = Vec::new();
    let mut out = String::from(
        "use vstd::prelude::*;\nuse crate::types::*;\nuse crate::prelude::*;\nuse crate::model::*;\nuse crate::contract::*;\n\nverus! {\n\n",
    );
    // apply and expected, per function.
    for e in &ctx.ents {
        let tag = e.fields.iter().find(|f| f.tag);
        let in_arm = match (tag, &e.sum) {
            (Some(t), Some(s)) => {
                let wires: Vec<String> = s
                    .variants
                    .iter()
                    .zip(&s.wires)
                    .map(|((_, id), w)| format!("{}::{id} => {}", t.ty.spec(), string_literal(w)))
                    .collect();
                out.push_str(&format!(
                    "pub open spec fn {}_tag_wire(v: {}) -> &'static str {{\n    match v {{ {} }}\n}}\n\n",
                    e.snake,
                    t.ty.spec(),
                    wires.join(", ")
                ));
                format!(
                    "m.contains_key(id) && column == {} && {}_tag_wire(m[id].{}) == value",
                    string_literal(
                        &ctx.domain
                            .sums
                            .iter()
                            .find(|s| s.entity == e.id)
                            .map(|s| s.tag.wire.clone())
                            .unwrap_or_default()
                    ),
                    e.snake,
                    t.name
                )
            }
            _ => "false".to_string(),
        };
        out.push_str(&format!(
            "/// The row `id` is what `ex` names, in the rows `m`.\npub open spec fn {}_expected(m: Map<int, {}>, id: int, ex: ExpectedSource) -> bool {{\n    match ex {{\n        ExpectedSource::Absent => !m.contains_key(id),\n        ExpectedSource::Present => m.contains_key(id),\n        ExpectedSource::In {{ column, value }} => {in_arm},\n    }}\n}}\n\n",
            e.snake, e.m
        ));
    }
    let mut store_fns = String::new();
    let mut apply_fns = String::new();
    for f in &ctx.domain.fns {
        let a = ctx.action(&f.action);
        let fname = fn_ident(&f.name);
        let error = ctx.domain.error_name(&f.usecase);
        let mut applied: Vec<(String, String)> = ctx
            .ents
            .iter()
            .map(|e| (e.snake.clone(), format!("s.{}", e.snake)))
            .collect();
        let mut expected_parts = Vec::new();
        let mut body_checks = Vec::new();
        let mut body_writes = Vec::new();
        for o in &f.outputs {
            let e = ctx.ent(a.params[o.param].entity);
            let field = ident(&o.name);
            let (id, write) = match &o.kind {
                OutKind::Row { .. } => {
                    let view = if e.sum.is_some() {
                        let ty = match &o.kind {
                            OutKind::Row { ty, .. } => ty.clone(),
                            _ => unreachable!(),
                        };
                        format!("view_{ty}(w.{field})")
                    } else {
                        format!("{}_row_view(w.{field})", e.snake)
                    };
                    let id = format!("w.{field}.id.0 as int");
                    let slot = applied.iter_mut().find(|(n, _)| *n == e.snake).unwrap();
                    slot.1 = format!("{}.insert({id}, {view})", slot.1);
                    let row = match &e.sum {
                        Some(sum) => format!("{}::from(w.{field}.clone()).into_row()", sum.name),
                        None => format!("w.{field}.clone()"),
                    };
                    (
                        id,
                        format!(
                            "            self.{0}.retain(|r| r.id.0 != w.{field}.id.0);\n            self.{0}.push({row});\n",
                            e.snake
                        ),
                    )
                }
                OutKind::Deleted { .. } => {
                    let id = format!("w.{field}.0 as int");
                    let slot = applied.iter_mut().find(|(n, _)| *n == e.snake).unwrap();
                    slot.1 = format!("{}.remove({id})", slot.1);
                    (
                        id,
                        format!(
                            "            self.{}.retain(|r| r.id.0 != w.{field}.0);\n",
                            e.snake
                        ),
                    )
                }
            };
            expected_parts.push(format!(
                "{}_expected(s.{}, {id}, w.{})",
                e.snake,
                e.snake,
                ident(&o.expected_name())
            ));
            let exec_id = match &o.kind {
                OutKind::Row { .. } => format!("w.{field}.id.0"),
                _ => format!("w.{field}.0"),
            };
            let tagcol = e.sum.as_ref().map(|s| ident(&s.tag_field));
            let in_check = match &tagcol {
                Some(t) => format!(
                    "self.{}.iter().any(|r| r.id.0 == {exec_id} && r.{t}.as_str() == value)",
                    e.snake
                ),
                None => "false".to_string(),
            };
            body_checks.push(format!(
                "match w.{} {{ ExpectedSource::Absent => !self.{1}.iter().any(|r| r.id.0 == {exec_id}), ExpectedSource::Present => self.{1}.iter().any(|r| r.id.0 == {exec_id}), ExpectedSource::In {{ value, .. }} => {in_check} }}",
                ident(&o.expected_name()),
                e.snake
            ));
            body_writes.push(write);
        }
        let fields: Vec<String> = applied.iter().map(|(n, m)| format!("{n}: {m}")).collect();
        apply_fns.push_str(&format!(
            "/// The state after a commit of what `{fname}` returned.\npub open spec fn apply_{fname}(s: MState, w: {}) -> MState {{\n    MState {{ {} }}\n}}\n\n/// What `{fname}` returned may still be written.\npub open spec fn expected_{fname}(s: MState, w: {}) -> bool {{\n    {}\n}}\n\n",
            f.result,
            fields.join(", "),
            f.result,
            if expected_parts.is_empty() { "true".to_string() } else { expected_parts.join("\n    && ") }
        ));
        store_fns.push_str(&format!(
            "    /// Writes what `{fname}` returned in one transaction, each row only where it still is\n    /// what the result expects; else writes nothing.\n    #[verifier::external_body]\n    pub fn commit_{fname}(&mut self, w: {}) -> (r: Result<(), {error}>)\n        ensures\n            if expected_{fname}(old(self)@, w) {{\n                r is Ok && final(self)@ == apply_{fname}(old(self)@, w)\n            }} else {{\n                r is Err && final(self)@ == old(self)@\n            }},\n    {{\n        if {} {{\n{}            Ok(())\n        }} else {{\n            Err({error}::NotEnabled(NotEnabled {{ usecase: {} }}))\n        }}\n    }}\n\n",
            f.result,
            if body_checks.is_empty() { "true".to_string() } else { body_checks.join("\n            && ") },
            body_writes.concat(),
            string_literal(&f.usecase)
        ));
    }
    out.push_str(&apply_fns);
    // The store.
    let mut fields = String::new();
    let mut loads = String::new();
    for e in &ctx.ents {
        fields.push_str(&format!("    pub(crate) {}: Vec<{}>,\n", e.snake, e.row));
        let (ret, conv, view, idx) = match &e.sum {
            Some(s) => (
                s.name.clone(),
                format!("{}::from_row(r.clone())", s.name),
                format!("{}_view(x)", e.snake),
                format!("{}_id(x)", e.snake),
            ),
            None => (
                e.row.clone(),
                "r.clone()".to_string(),
                format!("{}_row_view(x)", e.snake),
                "x.id.0 as int".to_string(),
            ),
        };
        loads.push_str(&format!(
            "    /// The {0} with id `id`, if there is one.\n    #[verifier::external_body]\n    pub fn load_{1}(&self, id: {2}) -> (r: Option<{ret}>)\n        ensures\n            match r {{\n                Some(x) => {idx} == id.0 as int && self@.{1}.contains_key(id.0 as int) && {view} == self@.{1}[id.0 as int],\n                None => !self@.{1}.contains_key(id.0 as int),\n            }},\n    {{\n        self.{1}.iter().find(|r| r.id.0 == id.0).map(|r| {conv})\n    }}\n\n    /// Every {0} row, read in the call's transaction.\n    #[verifier::external_body]\n    pub fn rows_{1}(&self) -> (r: Vec<{3}>)\n        ensures {1}_snapshot(self@.{1}, r@),\n    {{\n        self.{1}.clone()\n    }}\n\n",
            e.id, e.snake, e.id_ty, e.row
        ));
    }
    out.push_str(&format!(
        "/// The store, as the port sees it. TRUSTED: Verus does not check that a store keeps what\n/// its functions ensure. The bodies are an in-memory store for the differential harness;\n/// a service puts its database behind the same functions and keeps the same promises\n/// (#439's H3 and H4: the store is the real one, and each call is one transaction).\n#[verifier::external_body]\npub struct Store {{\n{fields}}}\n\nimpl Store {{\n    /// The rows the store holds, as the model's state.\n    pub uninterp spec fn view(&self) -> MState;\n\n{loads}{store_fns}}}\n\n"
    ));
    // Port calls.
    for a in ctx.ts.actions() {
        match port_call(ctx, a, atoms, core_trait) {
            Ok(t) => out.push_str(&t),
            Err(w) => why.push(w),
        }
    }
    out.push_str("} // verus!\n\n");
    out.push_str(
        "impl Store {\n    /// An empty store.\n    pub fn new() -> Self {\n        Store {\n",
    );
    for e in &ctx.ents {
        out.push_str(&format!("            {}: Vec::new(),\n", e.snake));
    }
    out.push_str("        }\n    }\n}\n");
    if why.is_empty() {
        Ok(out)
    } else {
        Err(why)
    }
}

fn port_call(ctx: &Ctx, a: &Action, atoms: &Atoms, core_trait: &str) -> Result<String, String> {
    let fns: Vec<&FnDecl> = ctx
        .domain
        .fns
        .iter()
        .filter(|f| f.action == a.name)
        .collect();
    let Some(first) = fns.first() else {
        return Err(format!("{}: no function", a.name));
    };
    let error = ctx.domain.error_name(&first.usecase).to_string();
    let dispatch = ctx
        .domain
        .dispatchers
        .iter()
        .find(|d| d.cases.iter().any(|c| c.function == first.name));
    let (sig, call_name, call_args_src, outcome_cases): PortCall = match dispatch {
        Some(d) => {
            let sig = ctx.sig_of_dispatch(first)?;
            let mut args: Vec<(String, String)> =
                d.rows.iter().map(|(n, t)| (ident(n), t.clone())).collect();
            for i in &d.inputs {
                let ty = match &i.ty {
                    InputType::Value(t) => field_ty(t),
                    InputType::Rows(t) => format!("&[{t}]"),
                };
                args.push((ident(&i.name), ty));
            }
            let cases = d
                .cases
                .iter()
                .map(|c| (format!("{}::{}", d.outcome, c.case), fn_ident(&c.function)))
                .collect();
            (sig, fn_ident(&d.name), args, cases)
        }
        None => {
            if fns.len() != 1 {
                return Err(format!("{}: several functions and no dispatcher", a.name));
            }
            let sig = ctx.sig_of(first)?;
            if sig
                .args
                .iter()
                .any(|x| matches!(x, Some(Arg::Variant { .. })))
            {
                return Err(format!("{}: a state row and no dispatcher", a.name));
            }
            (sig, fn_ident(&first.name), exec_params(first), Vec::new())
        }
    };
    let name = act_name(a);
    let mut params = Vec::new();
    for (k, p) in a.params.iter().enumerate() {
        params.push(format!("p{k}: {}", ctx.ent(p.entity).id_ty));
    }
    let ids: Vec<String> = (0..a.params.len())
        .map(|k| format!("p{k}.0 as int"))
        .collect();
    let call = format!("Call::{}({})", call_variant(a), ids.join(", "));
    let mut body = String::from("        let ghost s = store@;\n");
    // Load each row argument by its param's id; a missing row means the action does not run.
    let mut call_args = Vec::new();
    for (n, ty) in &call_args_src {
        // Which param, or which list, or which created id.
        let k = sig.args.iter().position(|x| match x {
            Some(Arg::Variant { name, .. })
            | Some(Arg::Sum { name })
            | Some(Arg::Record { name }) => name == n,
            Some(Arg::New { input }) => input == n,
            None => false,
        });
        if let Some(k) = k {
            match &sig.args[k] {
                Some(Arg::New { .. }) => call_args.push(format!("p{k}")),
                _ => {
                    let e = ctx.ent(a.params[k].entity);
                    body.push_str(&format!(
                        "        let {n} = match store.load_{}(p{k}) {{\n            Some(x) => x,\n            None => return Err({error}::NotEnabled(NotEnabled {{ usecase: {} }})),\n        }};\n",
                        e.snake,
                        string_literal(&first.usecase)
                    ));
                    call_args.push(n.clone());
                }
            }
        } else if let Some((eid, _)) = sig.lists.iter().find(|(_, l)| l == n) {
            let e = ctx.ent(*eid);
            body.push_str(&format!("        let {n} = store.rows_{}();\n", e.snake));
            call_args.push(format!("{n}.as_slice()"));
        } else {
            return Err(format!(
                "{}: the argument {n} ({ty}) is not covered",
                a.name
            ));
        }
    }
    // The links: on every row, the quantifiers over the lists are the model's. The guard's
    // text is not needed, only the quantifiers it meets.
    let seen = RefCell::new(Vec::new());
    let _ = ctx.spec_guard(&sig, atoms, &seen)?;
    let mut links = Vec::new();
    for (atom, values) in seen.borrow().iter() {
        let list = sig
            .lists
            .iter()
            .find(|(e, _)| *e == atom.ent)
            .map(|(_, n)| n.clone())
            .ok_or_else(|| format!("{}: a quantifier over rows it does not take", a.name))?;
        let args = values.iter().map(|v| format!(", {v}")).collect::<String>();
        let list_spec = list.trim_end_matches('@');
        links.push(format!(
            "            {}_link(s, {list_spec}@{args});",
            atom.name
        ));
    }
    if !links.is_empty() {
        body.push_str(&format!(
            "        proof {{\n{}\n        }}\n",
            links.join("\n")
        ));
    }
    let ext: String = ctx
        .ents
        .iter()
        .map(|e| {
            format!(
                "                    assert(store@.{0} =~= {name}_step(s, {1}).{0});\n",
                e.snake,
                ids.join(", ")
            )
        })
        .collect();
    let commit = |fname: &str| {
        format!(
            "let r = store.commit_{fname}(w);\n                proof {{\n                    if r is Ok {{\n{ext}                    }}\n                }}\n                r"
        )
    };
    if outcome_cases.is_empty() {
        body.push_str(&format!(
            "        match C::{call_name}({}) {{\n            Ok(w) => {{\n                {}\n            }}\n            Err(e) => Err(e),\n        }}\n",
            call_args.join(", "),
            commit(&call_name)
        ));
    } else {
        let mut arms = String::new();
        for (pat, fname) in &outcome_cases {
            arms.push_str(&format!(
                "            Ok({pat}(w)) => {{\n                {}\n            }}\n",
                commit(fname)
            ));
        }
        body.push_str(&format!(
            "        match C::{call_name}({}) {{\n{arms}            Err(e) => Err(e),\n        }}\n",
            call_args.join(", ")
        ));
    }
    Ok(format!(
        "/// One call of {}: loads its rows, runs the core, commits. The store's rows move exactly as the\n/// model's `next` says: the step when it runs, nothing when it is refused.\npub fn port_{name}<C: {core_trait}>(store: &mut Store, {}) -> (r: Result<(), {error}>)\n    ensures\n        final(store)@ == next(old(store)@, {call}),\n        r is Ok <==> enabled(old(store)@, {call}),\n{{\n{body}}}\n\n",
        a.name,
        params.join(", ")
    ))
}
