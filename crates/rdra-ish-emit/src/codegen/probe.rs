//! The language-neutral parts of the probe (docs/design/code-check.md, "Recommendation"): a
//! generated file that states, one line per use case, that the service implements that use
//! case's contract, and the line map that takes a compiler error on a probe line back to its
//! use case. A language prints the lines; [`ProbeWriter`] numbers them. [`ContractIndex`]
//! takes an error in the user's code back to a use case by the generated method it names.

use super::ir::{ContractDecl, PortDecl, PortMethod};

/// The line of a generated file the fingerprint takes: the second, after the first line.
pub const FINGERPRINT_LINE: usize = 2;

/// One use case line of a probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeLine {
    /// 1-based, in the file as written: the fingerprint line counted.
    pub line: usize,
    /// The use case's id.
    pub usecase: String,
    /// The contract the line names.
    pub contract: String,
}

/// A probe's text and its line map.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Probe {
    /// The text as generated: without the fingerprint line, which writing the file inserts
    /// as line [`FINGERPRINT_LINE`].
    pub text: String,
    pub lines: Vec<ProbeLine>,
}

impl Probe {
    /// The use case line at `line` of the written file.
    pub fn at(&self, line: usize) -> Option<&ProbeLine> {
        self.lines.iter().find(|l| l.line == line)
    }
}

/// Writes a probe a line at a time, numbering the lines as the written file has them.
#[derive(Debug)]
pub struct ProbeWriter {
    probe: Probe,
    /// The line of the written file the next line takes.
    next: usize,
}

impl ProbeWriter {
    /// A probe whose first line is `first`: a comment, which the fingerprint line follows.
    pub fn new(first: &str) -> Self {
        let mut w = ProbeWriter {
            probe: Probe::default(),
            next: 1,
        };
        w.line(first);
        w
    }

    /// A line that names no use case.
    pub fn line(&mut self, text: &str) {
        for l in text.split('\n') {
            self.probe.text.push_str(l);
            self.probe.text.push('\n');
            self.next += 1;
            if self.next == FINGERPRINT_LINE {
                self.next += 1;
            }
        }
    }

    /// The line that states the service implements `contract`: one line, so an error on it
    /// is the use case's.
    pub fn usecase(&mut self, text: &str, contract: &ContractDecl) {
        assert!(!text.contains('\n'), "a use case line is one line");
        self.probe.lines.push(ProbeLine {
            line: self.next,
            usecase: contract.usecase.clone(),
            contract: contract.name.clone(),
        });
        self.line(text);
    }

    pub fn finish(self) -> Probe {
        self.probe
    }
}

/// The generated method names of a port, in one language's spelling, with the contract and
/// the action of each: an error in the user's code that names one goes back to its use case.
#[derive(Clone, Debug, Default)]
pub struct ContractIndex {
    /// `(method name, contract index, action)`.
    entries: Vec<(String, usize, String)>,
}

impl ContractIndex {
    /// The index of `port`, each method spelled by `spell` (Rust `prepare_pay`, `commit_pay`;
    /// Go `PreparePay`, `CommitPay`).
    pub fn new(port: &PortDecl, spell: impl Fn(&PortMethod) -> Vec<String>) -> Self {
        let mut entries = Vec::new();
        for (c, contract) in port.contracts.iter().enumerate() {
            for &m in &contract.methods {
                let method = &port.methods[m];
                for name in spell(method) {
                    entries.push((name, c, method.action.clone()));
                }
            }
        }
        ContractIndex { entries }
    }

    /// The first generated method `text` names as a whole word: its name, the index of its
    /// contract, and its action.
    pub fn find(&self, text: &str) -> Option<(&str, usize, &str)> {
        let word = |c: char| c.is_alphanumeric() || c == '_';
        let mut best: Option<(usize, &(String, usize, String))> = None;
        for entry in &self.entries {
            let at = text.match_indices(entry.0.as_str()).find(|(at, _)| {
                !text[..*at].chars().next_back().is_some_and(word)
                    && !text[at + entry.0.len()..].chars().next().is_some_and(word)
            });
            if let Some((at, _)) = at {
                if best.is_none_or(|(b, _)| at < b) {
                    best = Some((at, entry));
                }
            }
        }
        best.map(|(_, (name, c, action))| (name.as_str(), *c, action.as_str()))
    }

    /// The method names of contract `c`, in the order of its actions.
    pub fn methods_of(&self, c: usize) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|e| e.1 == c)
            .map(|e| e.0.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::ir::FnInput;

    fn method(action: &str, usecase: &str) -> PortMethod {
        PortMethod {
            action: action.to_string(),
            name: action.to_string(),
            usecase: usecase.to_string(),
            rows: Vec::new(),
            marks: Vec::new(),
            event: None,
            inputs: Vec::<FnInput>::new(),
            choices: Vec::new(),
            write: String::new(),
        }
    }

    fn contract(name: &str, usecase: &str, methods: Vec<usize>) -> ContractDecl {
        ContractDecl {
            name: name.to_string(),
            usecase: usecase.to_string(),
            label: String::new(),
            site: None,
            error: format!("{usecase}Error"),
            methods,
        }
    }

    #[test]
    fn probe_lines_count_the_fingerprint_line() {
        let ship = contract("ShipUseCase", "Ship", vec![0]);
        let cancel = contract("CancelUseCase", "Cancel", vec![1]);
        let mut w = ProbeWriter::new("// generated");
        w.line("type Service = svc::Service;\n");
        w.usecase("check::<dyn ShipUseCase>();", &ship);
        w.usecase("check::<dyn CancelUseCase>();", &cancel);
        let probe = w.finish();
        // Line 1 the comment, 2 the fingerprint, 3 the type, 4 empty, 5 Ship, 6 Cancel.
        assert_eq!(probe.at(5).map(|l| l.usecase.as_str()), Some("Ship"));
        assert_eq!(
            probe.at(6).map(|l| l.contract.as_str()),
            Some("CancelUseCase")
        );
        assert_eq!(probe.at(2), None);
        assert_eq!(probe.text.lines().count(), 5);
    }

    #[test]
    fn a_method_name_goes_back_to_its_contract_as_a_whole_word() {
        let port = PortDecl {
            methods: vec![method("Cancel", "Cancel"), method("CancelAll", "CancelAll")],
            contracts: vec![
                contract("CancelUseCase", "Cancel", vec![0]),
                contract("CancelAllUseCase", "CancelAll", vec![1]),
            ],
            ..PortDecl::default()
        };
        let index = ContractIndex::new(&port, |m| {
            vec![
                format!("prepare_{}", m.action.to_lowercase()),
                format!("commit_{}", m.action.to_lowercase()),
            ]
        });
        let msg = "E0053: method `commit_cancelall` has an incompatible type for trait";
        assert_eq!(index.find(msg), Some(("commit_cancelall", 1, "CancelAll")));
        let msg = "method `prepare_cancel` is not a member of trait";
        assert_eq!(index.find(msg), Some(("prepare_cancel", 0, "Cancel")));
        assert_eq!(index.find("prepare_cancel_order"), None);
        assert_eq!(index.methods_of(0), vec!["prepare_cancel", "commit_cancel"]);
    }
}
