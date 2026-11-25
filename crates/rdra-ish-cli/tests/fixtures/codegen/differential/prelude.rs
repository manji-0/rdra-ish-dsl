// Driver of tests/codegen_differential.rs: one JSON case per stdin line, one JSON result per
// stdout line. The test appends the decoders, the encoders, and `drv_run`.
#[path = "domain.rs"]
mod domain;
#[allow(unused_imports)]
use domain::*;

#[derive(Clone, Debug)]
enum DrvJ { Null, Bool(bool), Num(i64), Str(String), Arr(Vec<DrvJ>), Obj(Vec<(String, DrvJ)>) }

impl DrvJ {
    fn get(&self, k: &str) -> &DrvJ {
        match self {
            DrvJ::Obj(m) => m.iter().find(|(n, _)| n == k).map(|(_, v)| v).unwrap_or(&DrvJ::Null),
            _ => panic!("driver: not an object"),
        }
    }
    fn int(&self) -> i64 { match self { DrvJ::Num(n) => *n, other => panic!("driver: {other:?} is not a number") } }
    fn str(&self) -> String { match self { DrvJ::Str(s) => s.clone(), other => panic!("driver: {other:?} is not a string") } }
    fn bool(&self) -> bool { match self { DrvJ::Bool(b) => *b, other => panic!("driver: {other:?} is not a bool") } }
    fn arr(&self) -> &[DrvJ] { match self { DrvJ::Arr(a) => a, other => panic!("driver: {other:?} is not a list") } }
}

fn drv_opt<T>(j: &DrvJ, f: impl Fn(&DrvJ) -> T) -> Option<T> {
    match j { DrvJ::Null => None, j => Some(f(j)) }
}

fn drv_opt_enc<T>(v: &Option<T>, f: impl Fn(&T) -> DrvJ) -> DrvJ {
    match v { Some(x) => f(x), None => DrvJ::Null }
}

struct DrvP<'a> { s: &'a [u8], i: usize }

impl DrvP<'_> {
    fn ws(&mut self) { while self.i < self.s.len() && (self.s[self.i] as char).is_ascii_whitespace() { self.i += 1; } }
    fn eat(&mut self, c: u8) { self.ws(); assert_eq!(self.s[self.i], c, "driver: bad JSON"); self.i += 1; }
    fn value(&mut self) -> DrvJ {
        self.ws();
        match self.s[self.i] {
            b'n' => { self.i += 4; DrvJ::Null }
            b't' => { self.i += 4; DrvJ::Bool(true) }
            b'f' => { self.i += 5; DrvJ::Bool(false) }
            b'"' => DrvJ::Str(self.string()),
            b'[' => {
                self.i += 1;
                let mut out = Vec::new();
                self.ws();
                if self.s[self.i] == b']' { self.i += 1; return DrvJ::Arr(out); }
                loop {
                    out.push(self.value());
                    self.ws();
                    let c = self.s[self.i];
                    self.i += 1;
                    if c == b']' { return DrvJ::Arr(out); }
                }
            }
            b'{' => {
                self.i += 1;
                let mut out = Vec::new();
                self.ws();
                if self.s[self.i] == b'}' { self.i += 1; return DrvJ::Obj(out); }
                loop {
                    self.ws();
                    let k = self.string();
                    self.eat(b':');
                    out.push((k, self.value()));
                    self.ws();
                    let c = self.s[self.i];
                    self.i += 1;
                    if c == b'}' { return DrvJ::Obj(out); }
                }
            }
            _ => {
                let start = self.i;
                while self.i < self.s.len() && (self.s[self.i] == b'-' || self.s[self.i].is_ascii_digit()) { self.i += 1; }
                DrvJ::Num(std::str::from_utf8(&self.s[start..self.i]).unwrap().parse().expect("driver: a number"))
            }
        }
    }
    fn string(&mut self) -> String {
        self.eat(b'"');
        let mut out = Vec::new();
        loop {
            let c = self.s[self.i];
            self.i += 1;
            match c {
                b'"' => return String::from_utf8(out).unwrap(),
                b'\\' => {
                    let e = self.s[self.i];
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let hex = std::str::from_utf8(&self.s[self.i..self.i + 4]).unwrap();
                            self.i += 4;
                            let ch = char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap();
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        other => out.push(other),
                    }
                }
                c => out.push(c),
            }
        }
    }
}

fn drv_write(j: &DrvJ, out: &mut String) {
    match j {
        DrvJ::Null => out.push_str("null"),
        DrvJ::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        DrvJ::Num(n) => out.push_str(&n.to_string()),
        DrvJ::Str(s) => {
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                    c => out.push(c),
                }
            }
            out.push('"');
        }
        DrvJ::Arr(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() { if i > 0 { out.push(','); } drv_write(x, out); }
            out.push(']');
        }
        DrvJ::Obj(m) => {
            out.push('{');
            for (i, (k, v)) in m.iter().enumerate() {
                if i > 0 { out.push(','); }
                drv_write(&DrvJ::Str(k.clone()), out);
                out.push(':');
                drv_write(v, out);
            }
            out.push('}');
        }
    }
}

fn drv_obj(pairs: Vec<(&str, DrvJ)>) -> DrvJ {
    DrvJ::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    let input = std::io::read_to_string(std::io::stdin()).expect("driver: stdin");
    let mut out = String::new();
    for line in input.lines() {
        let case = DrvP { s: line.as_bytes(), i: 0 }.value();
        let f = case.get("f").int();
        let args = case.get("a").arr().to_vec();
        let result = std::panic::catch_unwind(|| drv_run(f, &args)).unwrap_or_else(|p| {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            drv_obj(vec![("panic", DrvJ::Str(msg))])
        });
        drv_write(&result, &mut out);
        out.push('\n');
    }
    print!("{out}");
}
