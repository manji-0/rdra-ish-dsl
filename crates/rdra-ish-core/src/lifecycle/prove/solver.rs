//! The solver the prover talks to: an SMT-LIB 2 session that takes commands and returns
//! what they print. `Z3` runs `z3 -in` as a child process and keeps it for the whole proof,
//! so scopes (`push` / `pop`) replace process starts.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use super::smt::{Command as Smt, Opt, Script};

/// A solver failure: the process died or answered with an error. Not a verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolverError(pub String);

impl std::fmt::Display for SolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SolverError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sat {
    Sat,
    Unsat,
    /// The solver gave up: it reached its resource limit, or a quantifier it could not decide.
    Unknown,
}

/// An SMT-LIB 2 session.
pub trait Solver {
    /// Runs `commands` and returns the lines they print. A line the solver prints as
    /// `(error ...)` is an `Err`.
    fn exec(&mut self, commands: &str) -> Result<Vec<String>, SolverError>;

    /// `check-sat`, which gives up once it has spent `rlimit` of z3's resource units. The
    /// limit holds for this check only: z3 5.1.0 also holds a `push` to a limit left set, and
    /// a `push` that runs out is an error.
    fn check_sat(&mut self, rlimit: u32) -> Result<Sat, SolverError> {
        let check = Script::from([
            Smt::SetOption(Opt::Rlimit(rlimit)),
            Smt::CheckSat,
            Smt::SetOption(Opt::Rlimit(0)),
        ]);
        let out = self.exec(check.line())?;
        match out.first().map(|l| l.trim()) {
            Some("sat") => Ok(Sat::Sat),
            Some("unsat") => Ok(Sat::Unsat),
            Some("unknown") | Some("timeout") => Ok(Sat::Unknown),
            other => Err(SolverError(format!(
                "unexpected check-sat answer: {}",
                other.unwrap_or("(nothing)")
            ))),
        }
    }
}

/// `z3 -in` as a child process.
pub struct Z3 {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

/// Printed after every batch of commands, so a reader knows where their output ends.
const MARK: &str = "@@rdra-ish-done@@";

impl Z3 {
    pub fn start(binary: &Path) -> Result<Z3, SolverError> {
        let mut child = Command::new(binary)
            .args(["-in", "-smt2"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| SolverError(format!("cannot start {}: {e}", binary.display())))?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        Ok(Z3 {
            child,
            stdin,
            stdout,
        })
    }

    /// The `z3 --version` line, for the report.
    pub fn version(binary: &Path) -> Result<String, SolverError> {
        let out = Command::new(binary)
            .arg("--version")
            .output()
            .map_err(|e| SolverError(format!("cannot run {}: {e}", binary.display())))?;
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

impl Solver for Z3 {
    fn exec(&mut self, commands: &str) -> Result<Vec<String>, SolverError> {
        let io = |e: std::io::Error| SolverError(format!("z3 i/o: {e}"));
        writeln!(self.stdin, "{commands}\n(echo \"{MARK}\")").map_err(io)?;
        self.stdin.flush().map_err(io)?;
        let mut out = Vec::new();
        let mut line = String::new();
        loop {
            line.clear();
            if self.stdout.read_line(&mut line).map_err(io)? == 0 {
                return Err(SolverError("z3 exited before answering".to_string()));
            }
            let l = line.trim_end();
            if l == MARK {
                break;
            }
            if l.starts_with("(error") {
                return Err(SolverError(format!("z3: {l}")));
            }
            out.push(l.to_string());
        }
        Ok(out)
    }
}

impl Drop for Z3 {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "(exit)");
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A parsed S-expression: what `get-value` prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Sexp {
    Atom(String),
    List(Vec<Sexp>),
}

impl Sexp {
    pub(crate) fn parse(text: &str) -> Option<Sexp> {
        let mut tokens = Vec::new();
        let mut cur = String::new();
        let mut quoted = false;
        for c in text.chars() {
            match c {
                '|' => {
                    quoted = !quoted;
                    cur.push(c);
                }
                '(' | ')' if !quoted => {
                    if !cur.is_empty() {
                        tokens.push(std::mem::take(&mut cur));
                    }
                    tokens.push(c.to_string());
                }
                c if c.is_whitespace() && !quoted => {
                    if !cur.is_empty() {
                        tokens.push(std::mem::take(&mut cur));
                    }
                }
                c => cur.push(c),
            }
        }
        if !cur.is_empty() {
            tokens.push(cur);
        }
        let mut at = 0;
        let sexp = Self::read(&tokens, &mut at)?;
        (at == tokens.len()).then_some(sexp)
    }

    fn read(tokens: &[String], at: &mut usize) -> Option<Sexp> {
        let token = tokens.get(*at)?;
        *at += 1;
        match token.as_str() {
            "(" => {
                let mut items = Vec::new();
                loop {
                    if tokens.get(*at)? == ")" {
                        *at += 1;
                        return Some(Sexp::List(items));
                    }
                    items.push(Self::read(tokens, at)?);
                }
            }
            ")" => None,
            atom => Some(Sexp::Atom(atom.to_string())),
        }
    }

    /// An integer, written `5` or `(- 5)`.
    pub(crate) fn int(&self) -> Option<i64> {
        match self {
            Sexp::Atom(a) => a.parse().ok(),
            Sexp::List(items) => match &items[..] {
                [Sexp::Atom(minus), n] if minus == "-" => n.int().map(|n| -n),
                _ => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Sexp;

    #[test]
    fn parses_get_value_output() {
        let s = Sexp::parse("(((v0 p0) 2) ((v1 p0) E0!val!1) (now (- 3)))").unwrap();
        let Sexp::List(pairs) = s else { panic!() };
        assert_eq!(pairs.len(), 3);
        let Sexp::List(last) = &pairs[2] else {
            panic!()
        };
        assert_eq!(last[1].int(), Some(-3));
        assert_eq!(Sexp::parse("(a"), None);
    }
}
