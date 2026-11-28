//! SMT-LIB 2 terms and commands as data, and the one printer that writes them.
//!
//! An `Expr` carries its sort, and each constructor checks the sorts of what it takes; a
//! command checks its own when it is printed. A mismatch is a bug in the encoder, so it panics
//! in a debug build. A release build prints the term as asked, z3 rejects it, and the proof
//! fails with a `SolverError`; it never reads as a verdict. Parentheses balance by
//! construction: only the printer writes them, and a symbol may not hold one.
//!
//! A `Script` is commands already printed, one per line: the prover sends the same
//! declarations, steps, and candidates many times, and a printed script is joined by copying.

use std::borrow::Cow;
use std::fmt::{self, Display, Write};
#[cfg(not(kani))]
use std::sync::Arc;

use crate::lifecycle::types::EntId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Sort {
    Bool,
    Int,
    /// The uninterpreted sort of an entity's rows.
    Ent(EntId),
}

impl Display for Sort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Sort::Bool => f.write_str("Bool"),
            Sort::Int => f.write_str("Int"),
            Sort::Ent(e) => write!(f, "E{}", e.index()),
        }
    }
}

/// A term and its sort. Cloning shares the tree.
#[derive(Clone)]
pub(super) struct Expr {
    sort: Sort,
    node: Shared,
}

/// A term's node, shared by its clones. Under Kani a node is leaked instead, since CBMC
/// unwinds the drop of an `Arc<Node>` through every kind of node to the harness's bound.
#[cfg(not(kani))]
type Shared = Arc<Node>;
#[cfg(kani)]
type Shared = &'static Node;

enum Node {
    /// A numeral, which is never negative in SMT-LIB.
    Num(u64),
    /// A symbol applied to arguments; with none it prints as the bare symbol.
    App(Cow<'static, str>, Vec<Expr>),
    /// `forall` or `exists` over symbols.
    Quant(&'static str, Vec<Expr>, Expr),
    /// `let` of symbols to terms.
    Let(Vec<(Expr, Expr)>, Expr),
}

impl Expr {
    /// A constant, a bound variable, or a nullary function, by name.
    pub(super) fn sym(name: impl Into<Cow<'static, str>>, sort: Sort) -> Expr {
        Expr::app(name, sort, Vec::new())
    }

    pub(super) fn bool(b: bool) -> Expr {
        Expr::sym(if b { "true" } else { "false" }, Sort::Bool)
    }

    /// An integer: a numeral, or `(- n)` below zero.
    pub(super) fn int(n: i64) -> Expr {
        let num = Expr::new(Sort::Int, Node::Num(n.unsigned_abs()));
        match n < 0 {
            true => Expr::app("-", Sort::Int, vec![num]),
            false => num,
        }
    }

    fn new(sort: Sort, node: Node) -> Expr {
        Expr {
            sort,
            #[cfg(not(kani))]
            node: Arc::new(node),
            #[cfg(kani)]
            node: Box::leak(Box::new(node)),
        }
    }

    fn app(head: impl Into<Cow<'static, str>>, sort: Sort, args: Vec<Expr>) -> Expr {
        let head = head.into();
        debug_assert!(
            !head.is_empty() && !head.contains(|c: char| c.is_whitespace() || "()|;\"".contains(c)),
            "{head:?} is not a simple symbol"
        );
        Expr::new(sort, Node::App(head, args))
    }

    fn is_sym(&self) -> bool {
        matches!(&*self.node, Node::App(_, args) if args.is_empty())
    }
}

fn want(sort: Sort, e: &Expr) {
    debug_assert_eq!(e.sort, sort, "{e} has the wrong sort");
}

fn same(es: &[Expr]) {
    if let Some(first) = es.first() {
        for e in es {
            want(first.sort, e);
        }
    }
}

fn bools(head: &'static str, args: Vec<Expr>) -> Expr {
    debug_assert!(!args.is_empty(), "({head}) has no arguments");
    for a in &args {
        want(Sort::Bool, a);
    }
    Expr::app(head, Sort::Bool, args)
}

/// `(and ..)` as given: one part prints as `(and p)`. The caller drops the parens if it wants.
pub(super) fn and(parts: Vec<Expr>) -> Expr {
    bools("and", parts)
}

/// `(or ..)` as given, like `and`.
pub(super) fn or(parts: Vec<Expr>) -> Expr {
    bools("or", parts)
}

pub(super) fn not(p: Expr) -> Expr {
    bools("not", vec![p])
}

pub(super) fn implies(p: Expr, q: Expr) -> Expr {
    bools("=>", vec![p, q])
}

pub(super) fn eq(a: Expr, b: Expr) -> Expr {
    want(a.sort, &b);
    Expr::app("=", Sort::Bool, vec![a, b])
}

pub(super) fn distinct(es: Vec<Expr>) -> Expr {
    debug_assert!(es.len() > 1, "a distinct of fewer than two terms");
    same(&es);
    Expr::app("distinct", Sort::Bool, es)
}

pub(super) fn ite(c: Expr, then: Expr, other: Expr) -> Expr {
    want(Sort::Bool, &c);
    want(then.sort, &other);
    let sort = then.sort;
    Expr::app("ite", sort, vec![c, then, other])
}

/// An order between integers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Rel {
    Lt,
    Le,
    Gt,
    Ge,
}

pub(super) fn rel(r: Rel, a: Expr, b: Expr) -> Expr {
    want(Sort::Int, &a);
    want(Sort::Int, &b);
    let head = match r {
        Rel::Lt => "<",
        Rel::Le => "<=",
        Rel::Gt => ">",
        Rel::Ge => ">=",
    };
    Expr::app(head, Sort::Bool, vec![a, b])
}

pub(super) fn add(a: Expr, b: Expr) -> Expr {
    want(Sort::Int, &a);
    want(Sort::Int, &b);
    Expr::app("+", Sort::Int, vec![a, b])
}

pub(super) fn sub(a: Expr, b: Expr) -> Expr {
    want(Sort::Int, &a);
    want(Sort::Int, &b);
    Expr::app("-", Sort::Int, vec![a, b])
}

fn quant(q: &'static str, vars: Vec<Expr>, body: Expr) -> Expr {
    debug_assert!(!vars.is_empty(), "a {q} binds nothing");
    debug_assert!(vars.iter().all(Expr::is_sym), "a {q} binds a non-symbol");
    want(Sort::Bool, &body);
    Expr::new(Sort::Bool, Node::Quant(q, vars, body))
}

/// `body` for every value of the symbols `vars`.
pub(super) fn forall(vars: Vec<Expr>, body: Expr) -> Expr {
    quant("forall", vars, body)
}

pub(super) fn exists(vars: Vec<Expr>, body: Expr) -> Expr {
    quant("exists", vars, body)
}

/// `body` with each symbol bound to its term.
pub(super) fn let_in(binds: Vec<(Expr, Expr)>, body: Expr) -> Expr {
    debug_assert!(!binds.is_empty(), "a let binds nothing");
    for (v, t) in &binds {
        debug_assert!(v.is_sym(), "a let binds a non-symbol");
        want(v.sort, t);
    }
    let sort = body.sort;
    Expr::new(sort, Node::Let(binds, body))
}

/// The meaning of the terms `encode.rs::Enc::one_of` writes, for its Kani harness: `false`, an
/// atom, or an `or` of atoms, where an atom is `=` or `<=` of numerals and one Int symbol. Any
/// other term panics, so the harness also proves `one_of` writes nothing else. The evaluator
/// does not recurse past the `or`, so CBMC does not unwind it through every kind of node.
#[cfg(kani)]
impl Expr {
    /// Whether a Bool term holds with the symbol `sym` at `at`.
    pub(super) fn holds_at(&self, sym: &str, at: i64) -> bool {
        match self.head() {
            ("false", []) => false,
            ("or", args) => args.iter().any(|a| a.atom_at(sym, at)),
            _ => self.atom_at(sym, at),
        }
    }

    fn atom_at(&self, sym: &str, at: i64) -> bool {
        match self.head() {
            ("=", [a, b]) => a.int_at(sym, at) == b.int_at(sym, at),
            ("<=", [a, b]) => a.int_at(sym, at) <= b.int_at(sym, at),
            _ => panic!("a term outside the fragment"),
        }
    }

    /// A Bool application's symbol and arguments.
    fn head(&self) -> (&str, &[Expr]) {
        assert!(self.sort == Sort::Bool, "a term of the wrong sort");
        match &*self.node {
            Node::App(head, args) => (head.as_ref(), &args[..]),
            _ => panic!("a term outside the fragment"),
        }
    }

    /// The value of an Int term with the symbol `sym` at `at`.
    fn int_at(&self, sym: &str, at: i64) -> i64 {
        assert!(self.sort == Sort::Int, "a term of the wrong sort");
        match &*self.node {
            Node::Num(n) => i64::try_from(*n).expect("a numeral past i64"),
            Node::App(head, args) if args.is_empty() && head.as_ref() == sym => at,
            _ => panic!("a term outside the fragment"),
        }
    }
}

/// A function symbol: its name, argument sorts, and result sort.
#[derive(Clone, Debug)]
pub(super) struct Fun {
    pub name: String,
    pub args: Vec<Sort>,
    pub ret: Sort,
}

impl Fun {
    pub(super) fn apply(&self, args: Vec<Expr>) -> Expr {
        debug_assert_eq!(
            args.len(),
            self.args.len(),
            "{} applied to too few or many",
            self.name
        );
        for (s, a) in self.args.iter().zip(&args) {
            want(*s, a);
        }
        Expr::app(self.name.clone(), self.ret, args)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Opt {
    /// z3's model-based quantifier instantiation.
    Mbqi(bool),
    /// z3's resource limit, in units; 0 is none.
    Rlimit(u32),
}

#[derive(Clone)]
pub(super) enum Command {
    Reset,
    SetOption(Opt),
    /// An entity's row sort.
    DeclareSort(EntId),
    /// A constant: the symbol, with its sort.
    DeclareConst(Expr),
    DeclareFun(Fun),
    /// A function with these parameter symbols and this body.
    DefineFun(Fun, Vec<Expr>, Expr),
    Assert(Expr),
    Push,
    Pop,
    CheckSat,
    GetValue(Vec<Expr>),
}

impl Command {
    /// The sorts and symbols the command needs: a debug build panics on a bad one.
    fn check(&self) {
        match self {
            Command::DeclareConst(c) => debug_assert!(c.is_sym(), "a constant {c} not a symbol"),
            Command::DefineFun(f, params, body) => {
                debug_assert_eq!(params.len(), f.args.len(), "{} has the wrong arity", f.name);
                for (s, p) in f.args.iter().zip(params) {
                    debug_assert!(p.is_sym(), "a parameter {p} not a symbol");
                    want(*s, p);
                }
                want(f.ret, body);
            }
            Command::Assert(p) => want(Sort::Bool, p),
            _ => {}
        }
    }
}

/// Commands printed one per line, each ending in a newline.
#[derive(Clone, Default)]
pub(super) struct Script(String);

impl Script {
    pub(super) fn push(&mut self, c: Command) {
        writeln!(self.0, "{c}").expect("a String takes any write");
    }

    pub(super) fn append(&mut self, other: &Script) {
        self.0 += &other.0;
    }

    /// The text, ending in a newline.
    pub(super) fn text(&self) -> &str {
        &self.0
    }

    /// The text without its last newline: how a batch that closes a scope or checks is sent.
    pub(super) fn line(&self) -> &str {
        self.0.strip_suffix('\n').unwrap_or(&self.0)
    }
}

impl<const N: usize> From<[Command; N]> for Script {
    fn from(cs: [Command; N]) -> Script {
        let mut s = Script::default();
        for c in cs {
            s.push(c);
        }
        s
    }
}

fn list<T>(
    f: &mut fmt::Formatter<'_>,
    items: &[T],
    item: impl Fn(&mut fmt::Formatter<'_>, &T) -> fmt::Result,
) -> fmt::Result {
    f.write_char('(')?;
    for (i, x) in items.iter().enumerate() {
        if i > 0 {
            f.write_char(' ')?;
        }
        item(f, x)?;
    }
    f.write_char(')')
}

/// `(v s)` for a symbol `v` of sort `s`.
fn binder(f: &mut fmt::Formatter<'_>, v: &Expr) -> fmt::Result {
    write!(f, "({v} {})", v.sort)
}

impl Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &*self.node {
            Node::Num(n) => write!(f, "{n}"),
            Node::App(head, args) if args.is_empty() => f.write_str(head),
            Node::App(head, args) => {
                write!(f, "({head}")?;
                for a in args {
                    write!(f, " {a}")?;
                }
                f.write_char(')')
            }
            Node::Quant(q, vars, body) => {
                write!(f, "({q} ")?;
                list(f, vars, binder)?;
                write!(f, " {body})")
            }
            Node::Let(binds, body) => {
                f.write_str("(let ")?;
                list(f, binds, |f, (v, t)| write!(f, "({v} {t})"))?;
                write!(f, " {body})")
            }
        }
    }
}

impl fmt::Debug for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}: {}", self.sort)
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.check();
        match self {
            Command::Reset => f.write_str("(reset)"),
            Command::SetOption(Opt::Mbqi(on)) => write!(f, "(set-option :smt.mbqi {on})"),
            Command::SetOption(Opt::Rlimit(n)) => write!(f, "(set-option :rlimit {n})"),
            Command::DeclareSort(e) => write!(f, "(declare-sort {} 0)", Sort::Ent(*e)),
            Command::DeclareConst(c) => write!(f, "(declare-const {c} {})", c.sort),
            Command::DeclareFun(fun) => {
                write!(f, "(declare-fun {} ", fun.name)?;
                list(f, &fun.args, |f, s| write!(f, "{s}"))?;
                write!(f, " {})", fun.ret)
            }
            Command::DefineFun(fun, params, body) => {
                write!(f, "(define-fun {} ", fun.name)?;
                list(f, params, binder)?;
                write!(f, " {} {body})", fun.ret)
            }
            Command::Assert(p) => write!(f, "(assert {p})"),
            Command::Push => f.write_str("(push)"),
            Command::Pop => f.write_str("(pop)"),
            Command::CheckSat => f.write_str("(check-sat)"),
            Command::GetValue(ts) => {
                f.write_str("(get-value ")?;
                list(f, ts, |f, t| write!(f, "{t}"))?;
                f.write_char(')')
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e0() -> Sort {
        Sort::Ent(EntId::at(0))
    }

    #[test]
    fn prints_terms_and_commands() {
        let x = Expr::sym("x", e0());
        let v = Fun {
            name: "v0".to_string(),
            args: vec![e0()],
            ret: Sort::Int,
        };
        let body = and(vec![rel(Rel::Le, Expr::int(0), v.apply(vec![x.clone()]))]);
        let mut s = Script::from([
            Command::DeclareSort(EntId::at(0)),
            Command::DeclareFun(v.clone()),
            Command::Assert(forall(vec![x.clone()], body)),
        ]);
        s.push(Command::DefineFun(
            Fun {
                name: "now_n".to_string(),
                args: Vec::new(),
                ret: Sort::Int,
            },
            Vec::new(),
            add(Expr::sym("now", Sort::Int), Expr::int(-3)),
        ));
        s.push(Command::GetValue(vec![x.clone(), Expr::int(2)]));
        assert_eq!(
            s.text(),
            "(declare-sort E0 0)\n(declare-fun v0 (E0) Int)\n\
             (assert (forall ((x E0)) (and (<= 0 (v0 x)))))\n\
             (define-fun now_n () Int (+ now (- 3)))\n(get-value (x 2))\n"
        );
        let w = Expr::sym("b0_0", e0());
        let b = Expr::sym("b0", e0());
        let t = exists(vec![w.clone()], let_in(vec![(b.clone(), w)], eq(b, x)));
        assert_eq!(
            t.to_string(),
            "(exists ((b0_0 E0)) (let ((b0 b0_0)) (= b0 x)))"
        );
        assert_eq!(
            Script::from([Command::Pop, Command::Pop]).line(),
            "(pop)\n(pop)"
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "wrong sort")]
    fn a_sort_mismatch_panics() {
        eq(Expr::sym("x", e0()), Expr::int(1));
    }
}
