//! `Pred` as TLA+ text, with the explorer's comparison rules (`lifecycle::expr::compare`):
//! numeric when either side is Int-typed, `null` equal only to `null` and never ordered, and
//! an owner compared with a bound entity's slot.

use rdra_ish_core::lifecycle::{At, CmpOp, Domain, Pred, Term, TransitionSystem};

use super::{render_value, TlaNames, NULL_INT};

/// Where a predicate is read.
pub(super) struct Ctx<'a> {
    pub ts: &'a TransitionSystem,
    pub names: &'a TlaNames,
    /// Read the post-state (after-asserts).
    pub primed: bool,
    /// Binders already bound outside the predicate (`b0..`).
    pub depth: u8,
}

pub(super) fn at(a: At) -> String {
    match a {
        At::Param(p) => format!("p{p}"),
        At::Bound(b) => format!("b{b}"),
    }
}

impl Ctx<'_> {
    fn prime(&self) -> &'static str {
        if self.primed {
            "'"
        } else {
            ""
        }
    }

    /// `E_exists[x] = "present"` in the read state.
    pub fn present(&self, e: rdra_ish_core::lifecycle::EntId, x: &str) -> String {
        let exists = self.names.var(self.ts.entity(e).exists);
        format!("{exists}{}[{x}] = \"present\"", self.prime())
    }

    pub fn pred(&mut self, p: &Pred) -> String {
        match p {
            Pred::True => "TRUE".to_string(),
            Pred::False => "FALSE".to_string(),
            Pred::Cmp(a, op, b) => self.cmp(a, *op, b),
            Pred::Not(x) => format!("~({})", self.pred(x)),
            Pred::And(xs) => self.join(xs, "/\\", "TRUE"),
            Pred::Or(xs) => self.join(xs, "\\/", "FALSE"),
            Pred::Forall { entity, body } => self.quantify(*entity, body, true),
            Pred::Exists { entity, body } => self.quantify(*entity, body, false),
        }
    }

    fn join(&mut self, xs: &[Pred], op: &str, empty: &str) -> String {
        match xs {
            [] => empty.to_string(),
            [x] => self.pred(x),
            _ => xs
                .iter()
                .map(|x| format!("({})", self.pred(x)))
                .collect::<Vec<_>>()
                .join(&format!(" {op} ")),
        }
    }

    fn quantify(&mut self, e: rdra_ish_core::lifecycle::EntId, body: &Pred, all: bool) -> String {
        let b = format!("b{}", self.depth);
        self.depth += 1;
        let inner = self.pred(body);
        self.depth -= 1;
        let ids = self.names.ids(e);
        let present = self.present(e, &b);
        if all {
            format!("\\A {b} \\in {ids} : ({present}) => ({inner})")
        } else {
            format!("\\E {b} \\in {ids} : ({present}) /\\ ({inner})")
        }
    }

    fn slot(&self, t: &Term) -> Option<(String, &Domain)> {
        match t {
            Term::Slot { var, at: a } => Some((
                format!("{}{}[{}]", self.names.var(*var), self.prime(), at(*a)),
                &self.ts.var(*var).domain,
            )),
            _ => None,
        }
    }

    fn cmp(&self, a: &Term, op: CmpOp, b: &Term) -> String {
        // An owner against a bound entity's slot.
        for (x, y) in [(a, b), (b, a)] {
            if let (Some((owner, Domain::Instance { .. })), Term::Index(i)) = (self.slot(x), y) {
                return match op {
                    CmpOp::Eq => format!("{owner} = {}", at(*i)),
                    CmpOp::Ne => format!("{owner} # {}", at(*i)),
                    _ => "FALSE".to_string(),
                };
            }
        }
        let numeric_slot = |t: &Term| self.slot(t).filter(|(_, d)| d.is_numeric());
        let domain = numeric_slot(a).or_else(|| numeric_slot(b)).map(|(_, d)| d);
        let is_number = |t: &Term| matches!(t, Term::Int(_) | Term::Now);
        if domain.is_some() || is_number(a) || is_number(b) {
            return self.numeric(a, op, b, domain);
        }
        // Same-domain values: TLA equality on the rendered values is index equality. The
        // build rejects ordered comparisons of non-numeric operands.
        let other = |t: &Term| self.slot(t).map(|(_, d)| d);
        let text = |t: &Term, peer: &Term| match t {
            Term::Lit(v) => match other(peer) {
                Some(d) => render_value(d, *v),
                None => v.0.to_string(),
            },
            Term::Index(i) => at(*i),
            _ => self.slot(t).map(|(s, _)| s).unwrap_or_default(),
        };
        let (l, r) = (text(a, b), text(b, a));
        match op {
            CmpOp::Eq => format!("{l} = {r}"),
            CmpOp::Ne => format!("{l} # {r}"),
            _ => "FALSE".to_string(),
        }
    }

    /// Numeric comparison; a nullable operand is guarded out of every ordered comparison.
    fn numeric(&self, a: &Term, op: CmpOp, b: &Term, domain: Option<&Domain>) -> String {
        let operand = |t: &Term| -> (String, bool) {
            match t {
                Term::Int(n) => (n.to_string(), false),
                Term::Now => (format!("now{}", self.prime()), false),
                Term::Index(i) => (format!("({} - 1)", at(*i)), false),
                Term::Lit(v) => match domain {
                    Some(d) => match d.number(*v) {
                        Some(n) => (n.to_string(), false),
                        None => (NULL_INT.to_string(), true),
                    },
                    None => (v.0.to_string(), false),
                },
                Term::Slot { .. } => {
                    let (s, d) = self.slot(t).expect("slot");
                    (s, *d == Domain::NullableInt)
                }
            }
        };
        let ((l, ln), (r, rn)) = (operand(a), operand(b));
        let sym = match op {
            CmpOp::Eq => return format!("{l} = {r}"),
            CmpOp::Ne => return format!("{l} # {r}"),
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
        };
        let mut parts = Vec::new();
        if ln {
            parts.push(format!("{l} # {NULL_INT}"));
        }
        if rn {
            parts.push(format!("{r} # {NULL_INT}"));
        }
        parts.push(format!("{l} {sym} {r}"));
        parts.join(" /\\ ")
    }
}
