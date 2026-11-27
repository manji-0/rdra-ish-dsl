//! Random models of the reference interpreter's subset, as source text, with the bounds to
//! compare them at. `scripts/reduction-diff.py` draws keys, `.along`, split use cases and
//! timers, which the subset leaves out, so this generator is the subset's own.
//!
//! Each model has one or two entities of up to three columns (Enum, Bool, Int, nullable
//! String; `@null`, `@default` and an Int `@check` at random), a creating use case or none
//! (pre-existing),
//! updating use cases that `sets` a literal, `null` or `present` (with `.when` guards, one
//! or two lines a column), move an Enum by `transitions` arms (with choices and `.when`), or
//! write nothing, a deleting use case, `applies_to` guards, and one to three rules of each
//! entity of the four written forms. Bounds: one to three instances, Int range 0..=1 to
//! 0..=3.

use std::fmt::Write as _;
use std::num::NonZeroU8;

use super::Bounds;

/// A xorshift generator: the same seed gives the same model on every platform.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// True with probability `p` percent.
    fn chance(&mut self, p: usize) -> bool {
        self.below(100) < p
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
}

#[derive(Clone)]
enum Ty {
    Enum(usize),
    Bool,
    Int,
    Str,
}

#[derive(Clone)]
struct Col {
    name: String,
    ty: Ty,
    null: bool,
}

struct Gen {
    rng: Rng,
    int_max: usize,
    src: String,
    conds: usize,
}

impl Gen {
    /// A comparison of a column of `cols`, its column named bare or as `E.col`.
    fn atom(&mut self, cols: &[Col], qual: &str) -> String {
        let c = self.rng.pick(cols).clone();
        let name = format!("{qual}{}", c.name);
        if c.null && self.rng.chance(20) {
            let v = if self.rng.chance(50) {
                "null"
            } else {
                "present"
            };
            let op = if self.rng.chance(70) { "==" } else { "!=" };
            return format!("{name} {op} {v}");
        }
        match c.ty {
            Ty::Enum(k) => {
                let v = self.rng.below(k);
                match self.rng.below(3) {
                    0 => format!("{name} == v{v}"),
                    1 => format!("{name} != v{v}"),
                    _ => {
                        let w = (v + 1) % k;
                        let op = if self.rng.chance(50) { "in" } else { "not in" };
                        format!("{name} {op} (v{v}, v{w})")
                    }
                }
            }
            Ty::Bool => {
                let op = if self.rng.chance(70) { "==" } else { "!=" };
                format!("{name} {op} {}", self.rng.chance(50))
            }
            Ty::Int => {
                let others: Vec<&Col> = cols
                    .iter()
                    .filter(|o| matches!(o.ty, Ty::Int) && o.name != c.name)
                    .collect();
                let op = *self.rng.pick(&["==", "!=", "<", "<=", ">", ">="]);
                if !others.is_empty() && self.rng.chance(25) {
                    let o = self.rng.pick(&others).name.clone();
                    return format!("{name} {op} {qual}{o}");
                }
                format!("{name} {op} {}", self.rng.below(self.int_max + 1))
            }
            Ty::Str => format!("{name} == present"),
        }
    }

    /// A new condition on `ent`, by name.
    fn condition(&mut self, ent: &str, cols: &[Col]) -> String {
        self.conds += 1;
        let name = format!("C{}", self.conds);
        let n = 1 + self.rng.below(2);
        let atoms: Vec<String> = (0..n)
            .map(|_| self.atom(cols, &format!("{ent}.")))
            .collect();
        let _ = writeln!(self.src, "condition {name} \"c\"");
        let _ = writeln!(self.src, "holds({name}, {})", atoms.join(", "));
        name
    }

    /// A value `sets` may write to `c`.
    fn value(&mut self, c: &Col) -> String {
        if c.null && self.rng.chance(25) {
            return "null".into();
        }
        match c.ty {
            Ty::Enum(k) => format!("v{}", self.rng.below(k)),
            Ty::Bool => self.rng.chance(50).to_string(),
            Ty::Int if c.null && self.rng.chance(30) => "present".into(),
            Ty::Int => self.rng.below(self.int_max + 1).to_string(),
            Ty::Str => "present".into(),
        }
    }

    /// `sets` lines for one column of `ent` by `uc`: one, or two guarded ones.
    fn sets(&mut self, uc: &str, ent: &str, cols: &[Col], writable: &[Col]) {
        if writable.is_empty() {
            return;
        }
        let c = self.rng.pick(writable).clone();
        let lines = if self.rng.chance(30) { 2 } else { 1 };
        for _ in 0..lines {
            let v = self.value(&c);
            let when = if lines == 2 || self.rng.chance(30) {
                format!(".when({})", self.condition(ent, cols))
            } else {
                String::new()
            };
            let _ = writeln!(self.src, "sets({uc}, {ent}.{} == {v}){when}", c.name);
        }
    }

    fn rule(&mut self, ent: &str, cols: &[Col]) {
        let atoms = |g: &mut Gen, n: usize| -> String {
            (0..n)
                .map(|_| g.atom(cols, ""))
                .collect::<Vec<_>>()
                .join(", ")
        };
        // `.when(Cond)` on a `forbidden` or an `invariant` adds the condition's body.
        let when = |g: &mut Gen| -> String {
            match g.rng.chance(20) {
                true => format!(".when({})", g.condition(ent, cols)),
                false => String::new(),
            }
        };
        let line = match self.rng.below(4) {
            0 => {
                let n = 1 + self.rng.below(2);
                let a = atoms(self, n);
                format!("forbidden({ent}, {a}){}", when(self))
            }
            1 => format!("required({ent}, {})", atoms(self, 1)),
            2 => format!("exclusive({ent}, {})", atoms(self, 2)),
            _ => {
                let guard = when(self);
                let (g, t) = (atoms(self, 1), atoms(self, 1));
                format!("invariant({ent}){guard}.when({g}).then({t})")
            }
        };
        let _ = writeln!(self.src, "{line}");
    }
}

/// The model of `seed`, and the bounds to compare it at.
pub(super) fn model(seed: u64) -> (String, Bounds) {
    let mut rng = Rng::new(seed);
    let int_max = 1 + rng.below(3);
    let ents = 1 + rng.below(2);
    let mut g = Gen {
        rng,
        int_max,
        src: String::new(),
        conds: 0,
    };
    let mut per_entity = Vec::new();
    for e in 0..ents {
        let name = ["A", "B"][e].to_string();
        let ncols = 1 + g.rng.below(3);
        let mut cols = Vec::new();
        let _ = writeln!(g.src, "entity {name} \"{name}\" {{\n  id: Int @pk");
        for i in 0..ncols {
            let ty = match g.rng.below(4) {
                0 | 1 => Ty::Enum(2 + g.rng.below(2)),
                2 => Ty::Int,
                _ if g.rng.chance(50) => Ty::Bool,
                _ => Ty::Str,
            };
            let null = matches!(ty, Ty::Str) || g.rng.chance(30);
            let col = Col {
                name: format!("c{i}"),
                ty: ty.clone(),
                null,
            };
            let mut decl = match &ty {
                Ty::Enum(k) => {
                    let vs: Vec<String> = (0..*k).map(|v| format!("v{v}")).collect();
                    format!("Enum({})", vs.join(", "))
                }
                Ty::Bool => "Bool".into(),
                Ty::Int => "Int".into(),
                Ty::Str => "String".into(),
            };
            if null {
                decl.push_str(" @null");
            }
            if matches!(ty, Ty::Int) && g.rng.chance(15) {
                let _ = write!(decl, " @check(c{i} <= {})", g.rng.below(int_max + 1));
            }
            if !matches!(ty, Ty::Str) && g.rng.chance(25) {
                let v = match &ty {
                    Ty::Enum(k) => format!("v{}", g.rng.below(*k)),
                    Ty::Bool => "true".into(),
                    _ => g.rng.below(int_max + 1).to_string(),
                };
                let _ = write!(decl, " @default({v})");
            }
            let _ = writeln!(g.src, "  {}: {decl}", col.name);
            cols.push(col);
        }
        let _ = writeln!(g.src, "}}");
        per_entity.push((name, cols));
    }
    for (name, cols) in &per_entity {
        // An Enum column `transitions` moves is a state model, which no `sets` writes.
        let lifecycle = (cols.iter())
            .find(|c| matches!(c.ty, Ty::Enum(_)) && !c.null)
            .filter(|_| g.rng.chance(60))
            .map(|c| match c.ty {
                Ty::Enum(k) => (c.name.clone(), k),
                _ => unreachable!(),
            });
        let writable: Vec<Col> = (cols.iter())
            .filter(|c| lifecycle.as_ref().is_none_or(|(l, _)| *l != c.name))
            .cloned()
            .collect();
        if g.rng.chance(65) {
            let uc = format!("Make{name}");
            let _ = writeln!(g.src, "usecase {uc} \"make\"\ncreates({uc}, {name})");
            if g.rng.chance(40) {
                g.sets(&uc, name, cols, &writable);
            }
            if g.rng.chance(20) {
                let c = g.condition(name, cols);
                let _ = writeln!(g.src, "applies_to({c}, {uc})");
            }
        }
        for u in 0..1 + g.rng.below(3) {
            let uc = format!("Up{name}{u}");
            let _ = writeln!(g.src, "usecase {uc} \"up\"\nupdates({uc}, {name})");
            match (g.rng.below(5), &lifecycle) {
                (0 | 1, _) => g.sets(&uc, name, cols, &writable),
                (2 | 3, Some((col, k))) => {
                    let (col, k) = (col.clone(), *k);
                    for _ in 0..1 + g.rng.below(2) {
                        let (a, b) = (g.rng.below(k), g.rng.below(k));
                        let when = if g.rng.chance(30) {
                            format!(".when({})", g.condition(name, cols))
                        } else {
                            String::new()
                        };
                        let _ =
                            writeln!(g.src, "transitions({uc}, {name}.{col}, v{a} -> v{b}){when}");
                    }
                }
                _ => {}
            }
            if g.rng.chance(25) {
                let c = g.condition(name, cols);
                let _ = writeln!(g.src, "applies_to({c}, {uc})");
            }
        }
        if g.rng.chance(30) {
            let uc = format!("Del{name}");
            let _ = writeln!(g.src, "usecase {uc} \"del\"\ndeletes({uc}, {name})");
        }
        for _ in 0..1 + g.rng.below(3) {
            g.rule(name, cols);
        }
    }
    let n = |g: &mut Gen| {
        let most = if ents == 1 { 3 } else { 2 };
        NonZeroU8::new(1 + g.rng.below(most) as u8).expect("n > 0")
    };
    let mut bounds = Bounds::uniform(n(&mut g), int_max as u8);
    if ents == 2 && g.rng.chance(30) {
        bounds.per_entity.insert("B".into(), n(&mut g));
    }
    (g.src, bounds)
}
