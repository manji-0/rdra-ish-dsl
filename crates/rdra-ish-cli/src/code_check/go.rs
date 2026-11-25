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
use rdra_ish_emit::codegen::probe::{Probe, ProbeWriter};

use super::map::{CompilerMessage, Finding};
use super::{Backend, Target};

/// The test the probe holds when the traces are configured: `check --code=run` runs it.
const CONFORMANCE_TEST: &str = "TestRdraConformance";

#[derive(Default)]
pub(super) struct GoBackend {
    /// Every method a compiler message says the service lacks.
    missing: RefCell<Vec<String>>,
    /// Each use case's methods, in Go's spelling, as the shared mapping asked for them.
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

/// `to` relative to the directory `from`, with `/` between the parts.
fn relative(from: &Path, to: &Path) -> String {
    let (from, to) = (normalize(from), normalize(to));
    let a: Vec<_> = from.components().collect();
    let b: Vec<_> = to.components().collect();
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut parts: Vec<String> = vec!["..".to_string(); a.len() - common];
    parts.extend(
        b[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
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
        .path(&target.target.domain)
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
    fn probe(&self, target: &Target<'_>) -> Option<(PathBuf, Probe)> {
        let path = target.target.probe.clone()?;
        let svc = service(target)?;
        let port = target.generated.domain.as_ref()?.port.as_ref()?;
        let domain = domain_import(target, &svc, &path);
        let same = domain == svc.import;
        let dpkg = if same {
            svc.package.clone()
        } else {
            target.request.package.clone()
        };
        let harness = target.target.traces.is_some() && target.generated.traces.is_some();
        let mut w = ProbeWriter::new("// Code generated by rdra-ish codegen. DO NOT EDIT.");
        w.line(&format!(
            "//\n// The probe of `rdra-ish check --code` (docs/design/code-check.md): each use case line\n// states that *{0}.{1} implements the use case's contract, and the lines under it name each\n// of the contract's methods, so the compiler names what the service lacks. `check\n// --code=build` compiles it; `check --code=run` runs {CONFORMANCE_TEST} as well. Generate\n// it again with `rdra-ish codegen <model> --code`; do not edit it.\n\npackage {0}_test\n",
            svc.package, svc.ty
        ));
        let mut imports = String::from("import (\n");
        if harness {
            imports.push_str("\t\"os\"\n\t\"testing\"\n\n");
        }
        if !same {
            imports.push_str(&format!("\t\"{domain}\"\n"));
        }
        imports.push_str(&format!("\t\"{}\"\n)\n", svc.import));
        w.line(&imports);
        let receiver = format!("(*{}.{})", svc.package, svc.ty);
        for c in &port.contracts {
            let site = c
                .site
                .as_ref()
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            w.line(&format!(
                "// usecase {} \"{}\"{site}",
                c.usecase,
                c.label.replace('\n', " ")
            ));
            w.usecase(&format!("var _ {dpkg}.{} = {receiver}(nil)", c.name), c);
            for &m in &c.methods {
                for name in self.method_names(&port.methods[m]) {
                    w.line(&format!("var _ = {receiver}.{name}"));
                }
            }
            w.line("");
        }
        if harness {
            let probe_dir = target.path(&path).parent().map(Path::to_path_buf);
            let traces = target.target.traces.as_ref().map(|t| target.path(t));
            let traces = match (probe_dir, traces) {
                (Some(dir), Some(t)) => relative(&dir, &t),
                _ => "traces.txt".to_string(),
            };
            let new = target
                .target
                .new
                .clone()
                .unwrap_or_else(|| format!("new({}.{})", svc.package, svc.ty));
            w.line(&format!(
                "// {CONFORMANCE_TEST} drives a fresh service per trace along the model's traces\n// ({traces}): `rdra-ish check --code=run` runs it.\nfunc {CONFORMANCE_TEST}(t *testing.T) {{\n\ttraces, err := os.ReadFile({traces:?})\n\tif err != nil {{\n\t\tt.Fatal(err)\n\t}}\n\treport, err := {dpkg}.Conform(func() {dpkg}.ConformancePort {{ return {new} }}, string(traces))\n\tif err != nil {{\n\t\tt.Fatal(err)\n\t}}\n\tt.Logf(\"ok %d traces, %d calls, %d refusals, %d interleavings\", report.Traces, report.Calls, report.Refusals, report.Interleavings)\n}}"
            ));
        }
        let mut probe = w.finish();
        // Writing the file ends it with one newline.
        while probe.text.ends_with('\n') {
            probe.text.pop();
        }
        Some((path, probe))
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
        Some(
            ["go", "test", "-count=1", "-run", &run, &dir]
                .map(String::from)
                .to_vec(),
        )
    }

    /// `file:line:col: text` lines (`go vet` prefixes `vet: `), with the tab-indented lines
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
            let line = line.strip_prefix("vet: ").unwrap_or(line);
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
        let names = vec![
            format!("Prepare{}", method.name),
            format!("Commit{}", method.name),
        ];
        let mut methods = self.methods.borrow_mut();
        match methods.iter_mut().find(|(uc, _)| *uc == method.usecase) {
            Some((_, known)) => {
                for n in &names {
                    if !known.contains(n) {
                        known.push(n.clone());
                    }
                }
            }
            None => methods.push((method.usecase.clone(), names.clone())),
        }
        names
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
        let method = |name: &str| PortMethod {
            action: name.to_string(),
            name: name.to_string(),
            usecase: "Ship".to_string(),
            rows: Vec::new(),
            marks: Vec::new(),
            event: None,
            inputs: Vec::new(),
            choices: Vec::new(),
            write: String::new(),
        };
        go.method_names(&method("Ship"));
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
    fn relative_paths_climb_and_descend() {
        assert_eq!(
            relative(Path::new("/a/b/svc"), Path::new("/a/b/domain")),
            "../domain"
        );
        assert_eq!(relative(Path::new("/a/b"), Path::new("/a/b")), ".");
        assert_eq!(relative(Path::new("/a/b"), Path::new("/a/b/c/d")), "c/d");
    }
}
