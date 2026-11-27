//! Compiler messages back to model locations (docs/design/code-check.md, "Diagnostics"). A
//! message on a probe line belongs to that line's use case (the probe's line map); a message
//! elsewhere belongs to the use case of the generated method it names (the contract index),
//! which is unique per action. What a message says (a contract not implemented, a method
//! missing, a signature) is the language's to tell ([`super::Backend::classify`]); the routing
//! and the diagnostics are shared.

use std::path::Path;

use rdra_ish_emit::codegen::ir::ContractDecl;
use rdra_ish_emit::codegen::probe::{ContractIndex, Probe};

use super::{Backend, CodeDiagnostic};

/// One compiler error, as a backend parses it from the compiler's output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompilerMessage {
    /// As the compiler prints it: relative to the directory it ran in, or absolute.
    pub file: String,
    /// 1-based.
    pub line: usize,
    /// 1-based.
    pub column: usize,
    /// `E0277`, `TS2739`; none for a compiler with no codes (Go).
    pub code: Option<String>,
    pub text: String,
}

impl CompilerMessage {
    /// `file:line:col  CODE: text`, the file as `file` shows it: the `code:` line, with the
    /// text's further lines (tsc's elaboration of which member differs, Go's `have` and
    /// `want`) under it.
    fn shown(&self, file: &dyn Fn(&str) -> String) -> String {
        let code = self
            .code
            .as_ref()
            .map(|c| format!("{c}: "))
            .unwrap_or_default();
        format!(
            "{}:{}:{}  {code}{}",
            file(&self.file),
            self.line,
            self.column,
            self.text
        )
    }
}

/// What a compiler message says about a use case's contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // a language backend constructs them; Rust's comes with P2
pub(crate) enum Finding {
    /// The service does not implement the contract (Rust E0277 on a probe line).
    NotImplemented,
    /// The service implements the contract without some of its methods (E0046).
    Incomplete,
    /// A method's argument or return types are not the contract's (E0053).
    Signature,
    /// Another error, on a probe line or naming a generated method.
    Other,
}

/// What the mapping reads: the probe and its line map, the contracts and their methods in the
/// language's spelling, and the service the probe names.
pub(crate) struct Mapping<'a> {
    /// The probe's path, relative to rdra.toml or to the build directory: compared by suffix.
    pub probe_file: &'a Path,
    pub probe: &'a Probe,
    pub index: &'a ContractIndex,
    pub contracts: &'a [ContractDecl],
    /// The service type `[code.<lang>] service` names.
    pub service: &'a str,
    /// A compiler's path (relative to the directory the build ran in) as the check shows
    /// paths: relative to the working directory.
    pub shown: &'a dyn Fn(&str) -> String,
    /// The generated transition functions and dispatchers (Go `RunPay`, `PayFromOpen`), in
    /// the language's spelling, each with its use case's contract: a call to one in the
    /// service is that use case's code.
    pub functions: &'a ContractIndex,
    /// The `[code.<lang>]` table, for the `fix:` of a service the probe cannot find.
    pub table: &'a str,
    /// The source line a message is on, read for a message about a call's arguments whose
    /// text names no function (tsc's TS2554, rustc's E0061).
    pub source_line: &'a dyn Fn(&CompilerMessage) -> Option<String>,
}

/// Words of a compiler message that cannot resolve a name: Go `undefined:`, rustc E0432 /
/// E0433 / E0412, tsc TS2305 / TS2307 / TS2724.
const UNRESOLVED: [&str; 8] = [
    "undefined",
    "cannot find",
    "Cannot find",
    "unresolved",
    "failed to resolve",
    "has no exported member",
    "could not import",
    "not found",
];

/// Whether `msg`, on the probe, says the service `service` names cannot be found: the
/// message names a part of the service's path or its type, and cannot resolve a name.
fn names_missing_service(msg: &CompilerMessage, service: &str) -> bool {
    let parts = service
        .split(['/', '.', '#', ':'])
        .filter(|p| p.len() > 2 && !matches!(*p, "ts" | "mts" | "js" | "com" | "org"));
    UNRESOLVED.iter().any(|w| msg.text.contains(w))
        && parts.into_iter().any(|p| names(&msg.text, p))
}

/// Whether `text` holds `word` as a whole word (a contract's name in a message about a bound).
fn names(text: &str, word: &str) -> bool {
    let w = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(word).any(|(at, _)| {
        !text[..at].chars().next_back().is_some_and(w)
            && !text[at + word.len()..].chars().next().is_some_and(w)
    })
}

/// Whether two paths name the same file, one maybe relative to a directory above the other.
fn same_file(a: &Path, b: &Path) -> bool {
    a.ends_with(b) || b.ends_with(a)
}

/// How many of the messages that belong to no use case `CodeUnmapped` quotes.
const UNMAPPED_LISTED: usize = 20;

/// The diagnostics of `messages`: one per use case and finding, and one `CodeUnmapped`
/// listing the messages that belong to no use case.
pub(crate) fn map_messages(
    messages: &[CompilerMessage],
    m: &Mapping<'_>,
    backend: &dyn Backend,
) -> Vec<CodeDiagnostic> {
    let mut out: Vec<CodeDiagnostic> = Vec::new();
    let mut unmapped: Vec<&CompilerMessage> = Vec::new();
    let on_probe_file = |msg: &CompilerMessage| same_file(Path::new(&msg.file), m.probe_file);
    // A service the probe cannot find fails every line of the probe: one finding, not one per
    // use case.
    let (lost, messages): (Vec<&CompilerMessage>, Vec<&CompilerMessage>) =
        messages.iter().partition(|msg| {
            on_probe_file(msg)
                && backend.classify(msg) == Finding::Other
                && names_missing_service(msg, m.service)
        });
    if let Some(first) = lost.first() {
        out.push(CodeDiagnostic {
            name: "CodeServiceNotFound".to_string(),
            message: format!(
                "the probe cannot find the service {} ({} compiler error(s) on the probe)",
                m.service,
                lost.len()
            ),
            model: None,
            code: Some(first.shown(m.shown)),
            fix: Some(format!(
                "if the service's type or path changed, set `service` (and `module`, `new`) in {} of rdra.toml to it, then write the probe again with `rdra-ish codegen --probe` (or `--code`)",
                m.table
            )),
            warning: false,
        });
    }
    let on_probe = |msg: &CompilerMessage| {
        on_probe_file(msg)
            .then(|| m.probe.at(msg.line))
            .flatten()
            .and_then(|l| m.contracts.iter().position(|c| c.name == l.contract))
    };
    let named = |text: &str| {
        m.index
            .find(text)
            .or_else(|| m.functions.find(text))
            .map(|(_, c, _)| c)
    };
    // The messages on probe lines first: one names the use case where the probe states it,
    // and the same finding elsewhere (an impl that needs the contract) adds nothing.
    let mut ordered: Vec<&CompilerMessage> = messages;
    ordered.sort_by_key(|msg| on_probe(msg).is_none());
    let routed: Vec<(&CompilerMessage, Option<usize>)> = ordered
        .into_iter()
        .map(|msg| {
            let contract = on_probe(msg)
                .or_else(|| named(&msg.text))
                .or_else(|| m.contracts.iter().position(|c| names(&msg.text, &c.name)))
                .or_else(|| {
                    // A wrong number of arguments names no function: the call on its line
                    // does.
                    msg.text
                        .contains("argument")
                        .then(|| (m.source_line)(msg))
                        .flatten()
                        .and_then(|line| named(&line))
                });
            (msg, contract)
        })
        .collect();
    // Each contract's members that some message says are missing, across all its messages.
    let mut missing: Vec<Vec<String>> = vec![Vec::new(); m.contracts.len()];
    for (msg, contract) in &routed {
        if let (Some(c), Some((names, _))) = (contract, backend.missing_members(msg)) {
            for name in names {
                if !missing[*c].contains(&name) {
                    missing[*c].push(name);
                }
            }
        }
    }
    for (msg, contract) in routed {
        let Some(c) = contract else {
            unmapped.push(msg);
            continue;
        };
        let finding = whole_contract(backend.classify(msg), msg, c, m, backend, &missing[c]);
        let d = diagnostic(&m.contracts[c], c, msg, finding, m, backend);
        if !out.iter().any(|x| x.name == d.name) {
            out.push(d);
        }
    }
    if !unmapped.is_empty() {
        // Each one's first line; the whole of each is in the compiler's output above.
        let listed: Vec<String> = unmapped
            .iter()
            .take(UNMAPPED_LISTED)
            .map(|msg| {
                let one = CompilerMessage {
                    text: msg.text.lines().next().unwrap_or_default().to_string(),
                    ..(*msg).clone()
                };
                one.shown(m.shown)
            })
            .collect();
        let mut code = listed.join("\n");
        if unmapped.len() > UNMAPPED_LISTED {
            code.push_str(&format!(
                "\n... and {} more in the compiler's output above",
                unmapped.len() - UNMAPPED_LISTED
            ));
        }
        out.push(CodeDiagnostic {
            name: "CodeUnmapped".to_string(),
            message: format!(
                "{} compiler error(s) belong to no use case: the compiler's output is above",
                unmapped.len()
            ),
            model: None,
            code: Some(code),
            fix: None,
            warning: false,
        });
    }
    out
}

/// `Incomplete` is `NotImplemented` when the members the message says are missing are every
/// member of contract `c`, or when `missing`, what all the contract's messages say is
/// missing, holds every member: a compiler that has no code for "not implemented" (tsc), an
/// empty impl block (E0046 listing them all), or members missing across several messages, as
/// Go's backend counts them.
fn whole_contract(
    finding: Finding,
    msg: &CompilerMessage,
    c: usize,
    m: &Mapping<'_>,
    backend: &dyn Backend,
    missing: &[String],
) -> Finding {
    if finding != Finding::Incomplete {
        return finding;
    }
    let Some((names, more)) = backend.missing_members(msg) else {
        return finding;
    };
    let members = m.index.methods_of(c);
    let this_message =
        names.iter().all(|n| members.contains(&n.as_str())) && names.len() + more == members.len();
    let across = !members.is_empty() && members.iter().all(|x| missing.iter().any(|n| n == x));
    if this_message || across {
        Finding::NotImplemented
    } else {
        finding
    }
}

fn diagnostic(
    contract: &ContractDecl,
    c: usize,
    msg: &CompilerMessage,
    finding: Finding,
    m: &Mapping<'_>,
    backend: &dyn Backend,
) -> CodeDiagnostic {
    let uc = &contract.usecase;
    let hint = || backend.implement_hint(contract, m.service, &m.index.methods_of(c));
    let (kind, message, fix) = match finding {
        Finding::NotImplemented => (
            "CodeMissingUseCase",
            format!("use case {uc} has no implementation in {}", m.service),
            Some(hint()),
        ),
        Finding::Incomplete => (
            "CodeIncomplete",
            format!("use case {uc} is implemented in part in {}", m.service),
            Some(hint()),
        ),
        Finding::Signature => (
            "CodeSignature",
            format!("use case {uc} takes other inputs than the model's"),
            Some(format!(
                "take and return the types {} declares in the generated code",
                contract.name
            )),
        ),
        Finding::Other => (
            "CodeError",
            format!("the implementation of use case {uc} does not compile"),
            None,
        ),
    };
    let model = match &contract.site {
        Some(site) => format!("{site}  usecase {uc} \"{}\"", contract.label),
        None => format!("usecase {uc} \"{}\"", contract.label),
    };
    CodeDiagnostic {
        name: format!("{kind}_{uc}"),
        message,
        model: Some(model),
        code: Some(msg.shown(m.shown)),
        fix,
        warning: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rdra_ish_emit::codegen::ir::{PortDecl, PortMethod};
    use rdra_ish_emit::codegen::probe::ProbeWriter;

    fn as_is(file: &str) -> String {
        file.to_string()
    }

    fn no_line(_: &CompilerMessage) -> Option<String> {
        None
    }

    /// A backend of a made-up language: codes `X1` (not implemented) and `X2` (signature).
    struct Toy;
    impl Backend for Toy {
        fn classify(&self, m: &CompilerMessage) -> Finding {
            match m.code.as_deref() {
                Some("X1") => Finding::NotImplemented,
                Some("X2") => Finding::Signature,
                _ => Finding::Other,
            }
        }
    }

    fn method(action: &str) -> PortMethod {
        PortMethod {
            action: action.to_string(),
            name: action.to_string(),
            usecase: action.to_string(),
            rows: Vec::new(),
            marks: Vec::new(),
            event: None,
            inputs: Vec::new(),
            choices: Vec::new(),
            write: String::new(),
        }
    }

    fn contract(usecase: &str, m: usize) -> ContractDecl {
        ContractDecl {
            name: format!("{usecase}UseCase"),
            usecase: usecase.to_string(),
            label: format!("{usecase} label"),
            site: Some(format!("m.rdra:{}:1", m + 1)),
            error: format!("{usecase}Error"),
            methods: vec![m],
            entries: Vec::new(),
            caller: None,
        }
    }

    fn msg(file: &str, line: usize, code: &str, text: &str) -> CompilerMessage {
        CompilerMessage {
            file: file.to_string(),
            line,
            column: 7,
            code: Some(code.to_string()),
            text: text.to_string(),
        }
    }

    /// A backend whose code `X3` lists missing members, `M: a, b (+N)`.
    struct Lister;
    impl Backend for Lister {
        fn classify(&self, _m: &CompilerMessage) -> Finding {
            Finding::Incomplete
        }
        fn missing_members(&self, m: &CompilerMessage) -> Option<(Vec<String>, usize)> {
            let list = m.text.strip_prefix("M: ")?;
            let (names, more) = match list.split_once(" (+") {
                Some((names, n)) => (names, n.trim_end_matches(')').parse().ok()?),
                None => (list, 0),
            };
            Some((names.split(", ").map(str::to_string).collect(), more))
        }
    }

    #[test]
    fn a_message_missing_every_member_is_the_contract_not_implemented() {
        let port = PortDecl {
            methods: vec![method("Ship")],
            contracts: vec![contract("Ship", 0)],
            ..PortDecl::default()
        };
        let probe = Probe::default();
        let index = ContractIndex::new(&port, |m| {
            vec![format!("prep_{}", m.action), format!("commit_{}", m.action)]
        });
        let m = Mapping {
            probe_file: Path::new("tests/probe.x"),
            probe: &probe,
            index: &index,
            contracts: &port.contracts,
            service: "Svc",
            shown: &as_is,
            functions: &ContractIndex::default(),
            table: "[code.x]",
            source_line: &no_line,
        };
        let name = |text: &str| {
            map_messages(&[msg("src/svc.x", 3, "X3", text)], &m, &Lister)[0]
                .name
                .clone()
        };
        assert_eq!(name("M: prep_Ship, commit_Ship"), "CodeMissingUseCase_Ship");
        assert_eq!(name("M: prep_Ship (+1)"), "CodeMissingUseCase_Ship");
        assert_eq!(name("M: commit_Ship"), "CodeIncomplete_Ship");
        assert_eq!(name("M: prep_Ship, other"), "CodeIncomplete_Ship");
    }

    /// Ship's contract with the method names `spell` gives, mapped for `map_messages`.
    fn ship(spell: fn(&PortMethod) -> Vec<String>) -> (PortDecl, ContractIndex) {
        let port = PortDecl {
            methods: vec![method("Ship")],
            contracts: vec![contract("Ship", 0)],
            ..PortDecl::default()
        };
        let index = ContractIndex::new(&port, spell);
        (port, index)
    }

    fn names_of(found: &[CodeDiagnostic]) -> Vec<&str> {
        found.iter().map(|d| d.name.as_str()).collect()
    }

    #[test]
    fn members_missing_across_messages_are_the_contract_not_implemented() {
        let (port, index) =
            ship(|m| vec![format!("prep_{}", m.action), format!("commit_{}", m.action)]);
        let probe = Probe::default();
        let m = Mapping {
            probe_file: Path::new("tests/probe.x"),
            probe: &probe,
            index: &index,
            contracts: &port.contracts,
            service: "Svc",
            shown: &as_is,
            functions: &ContractIndex::default(),
            table: "[code.x]",
            source_line: &no_line,
        };
        let both = [
            msg("src/svc.x", 3, "X3", "M: prep_Ship"),
            msg("src/svc.x", 9, "X3", "M: commit_Ship"),
        ];
        assert_eq!(
            names_of(&map_messages(&both, &m, &Lister)),
            ["CodeMissingUseCase_Ship"]
        );
        let one = [msg("src/svc.x", 9, "X3", "M: commit_Ship")];
        assert_eq!(
            names_of(&map_messages(&one, &m, &Lister)),
            ["CodeIncomplete_Ship"]
        );
    }

    #[test]
    fn rust_unions_the_members_its_e0046_messages_miss() {
        let (port, index) = ship(|m| {
            let snake = m.action.to_lowercase();
            vec![format!("prepare_{snake}"), format!("commit_{snake}")]
        });
        let probe = Probe::default();
        let m = Mapping {
            probe_file: Path::new("tests/rdra_contract.rs"),
            probe: &probe,
            index: &index,
            contracts: &port.contracts,
            service: "Svc",
            shown: &as_is,
            functions: &ContractIndex::default(),
            table: "[code.x]",
            source_line: &no_line,
        };
        let e0046 = |line, member: &str| {
            msg(
                "src/lib.rs",
                line,
                "E0046",
                &format!("not all trait items implemented, missing: `{member}`"),
            )
        };
        let rust = super::super::rust::Rust;
        let both = [e0046(3, "prepare_ship"), e0046(9, "commit_ship")];
        assert_eq!(
            names_of(&map_messages(&both, &m, &rust)),
            ["CodeMissingUseCase_Ship"]
        );
        assert_eq!(
            names_of(&map_messages(&both[1..], &m, &rust)),
            ["CodeIncomplete_Ship"]
        );
    }

    #[test]
    fn typescript_unions_the_members_its_messages_miss() {
        let (port, index) = ship(|m| {
            vec![
                format!("prepare{}", m.action),
                format!("commit{}", m.action),
            ]
        });
        let probe = Probe::default();
        let m = Mapping {
            probe_file: Path::new("rdra_contract.ts"),
            probe: &probe,
            index: &index,
            contracts: &port.contracts,
            service: "Svc",
            shown: &as_is,
            functions: &ContractIndex::default(),
            table: "[code.x]",
            source_line: &no_line,
        };
        let ts2741 = |line, member: &str| {
            msg(
                "src/svc.ts",
                line,
                "TS2741",
                &format!("Property '{member}' is missing in type 'Svc' but required in type 'ShipUseCase'."),
            )
        };
        let ts = super::super::ts::TypeScript;
        let both = [ts2741(3, "prepareShip"), ts2741(9, "commitShip")];
        assert_eq!(
            names_of(&map_messages(&both, &m, &ts)),
            ["CodeMissingUseCase_Ship"]
        );
        assert_eq!(
            names_of(&map_messages(&both[1..], &m, &ts)),
            ["CodeIncomplete_Ship"]
        );
    }

    /// Go messages of a dogfooding run (go/logs/c3_build.txt, c4_rename_full.txt), mapped
    /// with ScheduleFollowUpVisit's functions.
    #[test]
    fn calls_of_generated_functions_and_a_lost_service_name_their_cause() {
        let port = PortDecl {
            methods: vec![method("ScheduleFollowUpVisit"), method("Ship")],
            contracts: vec![contract("ScheduleFollowUpVisit", 0), contract("Ship", 1)],
            ..PortDecl::default()
        };
        let mut w = ProbeWriter::new("// probe");
        w.usecase(
            "var _ domain.ScheduleFollowUpVisitUseCase = (*clinic.Service)(nil)",
            &port.contracts[0],
        );
        w.usecase(
            "var _ domain.ShipUseCase = (*clinic.Service)(nil)",
            &port.contracts[1],
        );
        let probe = w.finish();
        let index = ContractIndex::new(&port, |m| vec![format!("Prepare{}", m.action)]);
        // The dispatcher and a transition function of each, as Go spells them.
        let functions = ContractIndex::new(&port, |m| {
            vec![
                format!("Run{}", m.action),
                format!("{}FromActive", m.action),
            ]
        });
        let shown = |f: &str| format!("svc/{f}");
        // The line of a call whose message names no function.
        let line = |msg: &CompilerMessage| {
            (msg.line == 12).then(|| "    out, err := domain.RunShip(row, at)".to_string())
        };
        let m = Mapping {
            probe_file: Path::new("clinic/rdra_contract_test.go"),
            probe: &probe,
            index: &index,
            contracts: &port.contracts,
            service: "example.com/clinic/clinic.Service",
            shown: &shown,
            functions: &functions,
            table: "[code.go]",
            source_line: &line,
        };
        let go = |file: &str, line: usize, text: &str| CompilerMessage {
            file: file.to_string(),
            line,
            column: 3,
            code: None,
            text: text.to_string(),
        };
        let found = map_messages(
            &[
                go("clinic/rdra_contract_test.go", 3, "undefined: clinic.Service"),
                go("clinic/rdra_contract_test.go", 4, "undefined: clinic.Service"),
                go("clinic/usecases_glue.go", 599, "not enough arguments in call to domain.RunScheduleFollowUpVisit\nhave (domain.ProviderSchedule)\nwant (domain.ProviderSchedule, string)"),
                go("clinic/usecases_glue.go", 12, "too many arguments"),
                go("clinic/http.go", 7, "undefined: x"),
                go("clinic/http.go", 8, "declared and not used: y"),
            ],
            &m,
            &super::super::go::GoBackend::default(),
        );
        assert_eq!(
            names_of(&found),
            [
                "CodeServiceNotFound",
                "CodeError_ScheduleFollowUpVisit",
                "CodeError_Ship",
                "CodeUnmapped"
            ]
        );
        assert_eq!(
            found[0].to_string(),
            "error[CodeServiceNotFound]: the probe cannot find the service example.com/clinic/clinic.Service (2 compiler error(s) on the probe)\n  \
             code:  svc/clinic/rdra_contract_test.go:3:3  undefined: clinic.Service\n  \
             fix:   if the service's type or path changed, set `service` (and `module`, `new`) in [code.go] of rdra.toml to it, then write the probe again with `rdra-ish codegen --probe` (or `--code`)"
        );
        assert!(
            found[1]
                .to_string()
                .contains("\n         want (domain.ProviderSchedule, string)"),
            "{}",
            found[1]
        );
        assert_eq!(
            found[3].code.as_deref(),
            Some("svc/clinic/http.go:7:3  undefined: x\nsvc/clinic/http.go:8:3  declared and not used: y")
        );
    }

    #[test]
    fn messages_go_back_to_use_cases_by_probe_line_and_by_method() {
        let port = PortDecl {
            methods: vec![method("Ship"), method("Cancel")],
            contracts: vec![contract("Ship", 0), contract("Cancel", 1)],
            ..PortDecl::default()
        };
        let mut w = ProbeWriter::new("// probe");
        w.usecase("is::<ShipUseCase>();", &port.contracts[0]);
        w.usecase("is::<CancelUseCase>();", &port.contracts[1]);
        let probe = w.finish();
        let index = ContractIndex::new(&port, |m| vec![format!("prep_{}", m.action)]);
        let m = Mapping {
            probe_file: Path::new("svc/tests/probe.x"),
            probe: &probe,
            index: &index,
            contracts: &port.contracts,
            service: "Svc",
            shown: &as_is,
            functions: &ContractIndex::default(),
            table: "[code.x]",
            source_line: &no_line,
        };
        let found = map_messages(
            &[
                // Line 3 is Ship's (line 2 is the fingerprint).
                msg("tests/probe.x", 3, "X1", "Svc lacks ShipUseCase\nmore"),
                msg("tests/probe.x", 3, "X1", "the same again"),
                msg(
                    "src/svc.x",
                    40,
                    "X2",
                    "method `prep_Cancel` has another type",
                ),
                msg("src/svc.x", 41, "X9", "something else"),
                msg("src/other.x", 2, "X9", "and more"),
            ],
            &m,
            &Toy,
        );
        let names: Vec<&str> = found.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "CodeMissingUseCase_Ship",
                "CodeSignature_Cancel",
                "CodeUnmapped"
            ]
        );
        assert_eq!(
            found[0].to_string(),
            "error[CodeMissingUseCase_Ship]: use case Ship has no implementation in Svc\n  \
             model: m.rdra:1:1  usecase Ship \"Ship label\"\n  \
             code:  tests/probe.x:3:7  X1: Svc lacks ShipUseCase\n         more\n  \
             fix:   implement ShipUseCase for Svc (prep_Ship)"
        );
        assert!(found[2].message.starts_with("2 compiler error(s)"));
    }
}
