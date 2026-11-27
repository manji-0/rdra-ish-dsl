//! The Go backend of `check --code` (docs/design/code-check.md): the probe is a `_test.go`
//! file in the service's package (decision 2) that assigns the service to each use case's
//! contract interface, `build` compiles it with `go test -run ^$`, and `run` runs the
//! conformance test the probe holds with `go test`.
//!
//! Go names one missing method per interface it checks, so the probe also names each method
//! of a contract on a line of its own (`var _ = (*svc.Service).PreparePay`): the compiler then
//! names every method the service lacks, and [`GoBackend::classify`] tells a use case with
//! none of its methods (`NotImplemented`) from one with some (`Incomplete`).

use std::cell::RefCell;
use std::path::{Component, Path, PathBuf};

use rdra_ish_emit::codegen::ir::{ContractDecl, PortMethod};
use rdra_ish_emit::codegen::probe::Probe;
use rdra_ish_emit::codegen::{Lang, ProbeBinding, GO_CONFORMANCE_TEST as CONFORMANCE_TEST};

use super::map::{CompilerMessage, Finding};
use super::{Backend, Target};

#[derive(Default)]
pub(super) struct GoBackend {
    /// Every method a compiler message says the service lacks.
    missing: RefCell<Vec<String>>,
    /// Each contract's methods, in Go's spelling, as the probe states them.
    methods: RefCell<Vec<(String, Vec<String>)>>,
}

/// `service = "<import path>.<Type>"`: the import path, the package name (its last element),
/// and the type.
struct Service {
    import: String,
    package: String,
    ty: String,
}

fn service(target: &Target<'_>) -> Option<Service> {
    let spec = target.target.service.as_deref()?;
    let slash = spec.rfind('/').map_or(0, |i| i + 1);
    let dot = slash + spec[slash..].rfind('.')?;
    let import = spec[..dot].to_string();
    let package = import.rsplit('/').next()?.to_string();
    Some(Service {
        package,
        import,
        ty: spec[dot + 1..].to_string(),
    })
}

/// `path` absolute and without `.` or `..`, by its text: the files need not exist.
fn normalize(path: &Path) -> PathBuf {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// `to` relative to the directory `from`, with `/` between the parts: `.` when they are the
/// same.
fn relative(from: &Path, to: &Path) -> String {
    match crate::paths::relative(&normalize(from), &normalize(to)) {
        rel if rel.is_empty() => ".".to_string(),
        rel => rel,
    }
}

/// The import path of the domain package: `module` when given, else the service's import
/// path moved from the probe's directory to the domain's.
fn domain_import(target: &Target<'_>, svc: &Service, probe: &Path) -> String {
    if let Some(module) = &target.target.module {
        return module.clone();
    }
    let probe_dir = target.path(probe).parent().map(Path::to_path_buf);
    let domain_dir = target
        .path(target.target.code_file())
        .parent()
        .map(Path::to_path_buf);
    let (Some(probe_dir), Some(domain_dir)) = (probe_dir, domain_dir) else {
        return svc.import.clone();
    };
    let mut parts: Vec<&str> = svc.import.split('/').collect();
    let rel = relative(&probe_dir, &domain_dir);
    for part in rel.split('/') {
        match part {
            "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

/// The directory of the probe, relative to the build's root, as `go` takes a package:
/// `./svc`.
fn package_dir(target: &Target<'_>) -> Option<String> {
    let probe = target.path(target.target.probe.as_ref()?);
    let dir = probe.parent()?;
    let rel = relative(&target.root(), dir);
    Some(if rel == "." {
        ".".to_string()
    } else if rel.starts_with("..") {
        rel
    } else {
        format!("./{rel}")
    })
}

/// The method `text` says the service lacks: `missing method X` (an interface the service
/// does not satisfy), or `has no field or method X` (a method expression of the probe).
fn lacked(text: &str) -> Option<&str> {
    for marker in ["missing method ", "has no field or method "] {
        if let Some(at) = text.find(marker) {
            let rest = &text[at + marker.len()..];
            let end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            return Some(&rest[..end]);
        }
    }
    None
}

impl Backend for GoBackend {
    /// The probe `probe_for` prints, bound to the service, the domain's import path, and the
    /// traces' path from the probe's directory (where `go test` runs a test).
    fn probe(&self, target: &Target<'_>) -> Option<(PathBuf, Probe)> {
        let path = target.target.probe.clone()?;
        let svc = service(target)?;
        let probe_dir = target.path(&path).parent()?.to_path_buf();
        let module = domain_import(target, &svc, &path);
        // The service's package by its `package` clause when the domain shares it.
        let svc_package = match &target.target.package {
            Some(package) if module == svc.import => package.clone(),
            _ => svc.package.clone(),
        };
        let conformance = target.target.traces.as_ref().map(|traces| {
            let new = target
                .target
                .new
                .clone()
                .unwrap_or_else(|| format!("new({svc_package}.{})", svc.ty));
            (new, relative(&probe_dir, &target.path(traces)))
        });
        let binding = ProbeBinding {
            service: target.target.service.clone()?,
            module,
            conformance,
            package: target.target.package.clone(),
        };
        let domain = target.generated.domain.as_ref()?;
        // Each contract's methods, for `classify` to tell a use case with none from one with
        // some.
        if let Some(port) = &domain.port {
            *self.methods.borrow_mut() = port
                .contracts
                .iter()
                .map(|c| {
                    let mut names: Vec<String> = c
                        .methods
                        .iter()
                        .flat_map(|&m| self.method_names(&port.methods[m]))
                        .collect();
                    names.extend(c.entries.iter().flat_map(|e| self.entry_names(&e.name)));
                    (c.name.clone(), names)
                })
                .collect();
        }
        Some((path, super::probe_of(target, Lang::Go, domain, &binding)?))
    }

    fn build_command(&self, target: &Target<'_>) -> Option<Vec<String>> {
        let dir = package_dir(target)?;
        Some(
            ["go", "test", "-count=1", "-run", "^$", "-gcflags=-e", &dir]
                .map(String::from)
                .to_vec(),
        )
    }

    fn run_command(&self, target: &Target<'_>) -> Option<Vec<String>> {
        target.target.traces.as_ref()?;
        let dir = package_dir(target)?;
        let run = format!("^{CONFORMANCE_TEST}$");
        // The test logs what it ran and the calls that reached the unimplemented port, which
        // `go test` shows only so.
        let command: Vec<String> = ["go", "test", "-count=1", "-v", "-run", &run, &dir]
            .map(String::from)
            .to_vec();
        Some(command)
    }

    /// `file:line:col: text` lines (`go vet` prefixes `vet: `, or `vet.exe: ` on Windows),
    /// with the tab-indented lines
    /// under one (`have` / `want`) joined to its text.
    fn parse_messages(&self, stdout: &str, stderr: &str) -> Vec<CompilerMessage> {
        let mut out: Vec<CompilerMessage> = Vec::new();
        for line in stdout.lines().chain(stderr.lines()) {
            if line.starts_with('\t') {
                if let Some(last) = out.last_mut() {
                    last.text.push('\n');
                    last.text.push_str(line.trim());
                }
                continue;
            }
            let line = line
                .strip_prefix("vet: ")
                .or_else(|| line.strip_prefix("vet.exe: "))
                .unwrap_or(line);
            let mut parts = line.splitn(4, ':');
            let (Some(file), Some(l), Some(c), Some(text)) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let (Ok(l), Ok(c)) = (l.parse::<usize>(), c.parse::<usize>()) else {
                continue;
            };
            if !file.ends_with(".go") {
                continue;
            }
            out.push(CompilerMessage {
                file: file.strip_prefix("./").unwrap_or(file).to_string(),
                line: l,
                column: c,
                code: None,
                text: text.trim().to_string(),
            });
        }
        let mut missing = self.missing.borrow_mut();
        for m in &out {
            if let Some(name) = lacked(&m.text) {
                if !missing.iter().any(|x| x == name) {
                    missing.push(name.to_string());
                }
            }
        }
        out
    }

    /// `wrong type for method` is a signature; a method the service lacks is `Incomplete`, or
    /// `NotImplemented` when the service lacks every method of that use case.
    fn classify(&self, message: &CompilerMessage) -> Finding {
        if message.text.contains("wrong type for method") {
            return Finding::Signature;
        }
        let Some(name) = lacked(&message.text) else {
            return Finding::Other;
        };
        let missing = self.missing.borrow();
        let methods = self.methods.borrow();
        let Some((_, names)) = methods.iter().find(|(_, n)| n.iter().any(|x| x == name)) else {
            return Finding::Other;
        };
        if names.iter().all(|n| missing.contains(n)) {
            Finding::NotImplemented
        } else {
            Finding::Incomplete
        }
    }

    fn method_names(&self, method: &PortMethod) -> Vec<String> {
        rdra_ish_emit::codegen::method_names(Lang::Go, method).to_vec()
    }

    fn entry_names(&self, name: &str) -> Vec<String> {
        vec![rdra_ish_emit::codegen::entry_name(Lang::Go, name)]
    }

    /// `go test` fails the conformance test with `rdra-ish conformance: <report>`: the
    /// report and the rows that differ; else a panic of the service; else the last line.
    fn run_failure(&self, stdout: &str, stderr: &str) -> Option<String> {
        let all = format!("{stdout}\n{stderr}");
        super::harness_report(&all)
            .or_else(|| super::panic_report(&all))
            .or_else(|| super::last_line(&all))
    }

    fn implement_hint(&self, contract: &ContractDecl, service: &str, methods: &[&str]) -> String {
        format!(
            "give {service} the methods of {} ({})",
            contract.name,
            methods.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_vet_on_windows_prefixes_vet_exe() {
        let stderr = "vet.exe: svc/rdra_contract_test.go:25:28: cannot use (*svc.Service)(nil) as domain.ShipUseCase value\n";
        let found = GoBackend::default().parse_messages("", stderr);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].file, "svc/rdra_contract_test.go");
        assert_eq!((found[0].line, found[0].column), (25, 28));
    }

    #[test]
    fn compiler_lines_are_read_with_their_have_and_want() {
        let stderr = "# example.com/shop/svc_test [example.com/shop/svc.test]\n\
            svc/rdra_contract_test.go:16:29: cannot use (*svc.Service)(nil) (value of type *svc.Service) as domain.PayUseCase value in variable declaration: *svc.Service does not implement domain.PayUseCase (wrong type for method PreparePay)\n\
            \t\thave PreparePay(domain.OrderId, string) (domain.PayOutcome, error)\n\
            \t\twant PreparePay(domain.OrderId, domain.DateTime) (domain.PayOutcome, error)\n\
            vet: ./svc/rdra_contract_test.go:19:9: (*svc.Service).CommitShip undefined (type *svc.Service has no field or method CommitShip)\n\
            FAIL\texample.com/shop/svc [build failed]\n";
        let go = GoBackend::default();
        let found = go.parse_messages("", stderr);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].file, "svc/rdra_contract_test.go");
        assert_eq!((found[0].line, found[0].column), (16, 29));
        assert!(found[0].text.ends_with(
            "want PreparePay(domain.OrderId, domain.DateTime) (domain.PayOutcome, error)"
        ));
        assert_eq!(found[1].file, "svc/rdra_contract_test.go");
        assert_eq!(go.classify(&found[0]), Finding::Signature);
    }

    #[test]
    fn a_use_case_lacking_every_method_is_not_implemented() {
        let go = GoBackend::default();
        // What `probe` records of a contract with one action.
        *go.methods.borrow_mut() = vec![(
            "ShipUseCase".to_string(),
            vec!["PrepareShip".to_string(), "CommitShip".to_string()],
        )];
        let msg = |text: &str| CompilerMessage {
            file: "p_test.go".to_string(),
            line: 3,
            column: 1,
            code: None,
            text: text.to_string(),
        };
        let lacks_commit =
            msg("*svc.Service does not implement domain.ShipUseCase (missing method CommitShip)");
        let lacks_prepare = msg("(*svc.Service).PrepareShip undefined (type *svc.Service has no field or method PrepareShip)");
        go.parse_messages("", "p_test.go:3:1: *svc.Service does not implement domain.ShipUseCase (missing method CommitShip)\n");
        assert_eq!(go.classify(&lacks_commit), Finding::Incomplete);
        go.parse_messages("", "p_test.go:5:1: (*svc.Service).PrepareShip undefined (type *svc.Service has no field or method PrepareShip)\n");
        assert_eq!(go.classify(&lacks_commit), Finding::NotImplemented);
        assert_eq!(go.classify(&lacks_prepare), Finding::NotImplemented);
    }

    #[test]
    fn a_failed_run_quotes_the_harness_report() {
        let stdout = "--- FAIL: TestRdraConformance (0.01s)\n    rdra_contract_test.go:52: rdra-ish conformance: traces line 40: after \"Pay 1\", the store differs from the model:\n        Order 1: status paid, want pending\nFAIL\nFAIL\texample.com/shop/svc\t0.2s\n";
        assert_eq!(
            GoBackend::default().run_failure(stdout, "").as_deref(),
            Some("traces line 40: after \"Pay 1\", the store differs from the model:")
        );
        assert_eq!(
            GoBackend::default().run_failure("FAIL\n", "").as_deref(),
            Some("FAIL")
        );
    }

    #[test]
    fn relative_paths_climb_and_descend() {
        assert_eq!(
            relative(Path::new("/a/b/svc"), Path::new("/a/b/domain")),
            "../domain"
        );
        assert_eq!(relative(Path::new("/a/b"), Path::new("/a/b")), ".");
        assert_eq!(relative(Path::new("/a/b"), Path::new("/a/b/c/d")), "c/d");
    }
}
