//! The Go conformance port and harness: `StoreSnapshot`, `ConformancePort`, and `Conform`,
//! which drives a port along the traces `conformance::traces` writes.

use super::conformance::TRACES_FORMAT;
use super::go::{doc, field_type, local, struct_decl, Line};
use super::ir::{DomainModel, Field, FieldType, Scalar, Track};
use super::{string_literal, type_ident};

/// The packages the harness imports.
pub(super) const IMPORTS: [&str; 5] = ["errors", "fmt", "sort", "strconv", "strings"];

/// `Issue<Actor>Capability`.
fn issue(domain: &DomainModel, cap: &super::ir::CapabilityDecl) -> String {
    super::go::derived(domain, &format!("go:issue:{}", cap.actor), || {
        format!("Issue{}", cap.name)
    })
}

/// The unexported method that makes a capability a caller of `caller`.
fn caller_marker(caller: &super::ir::CallerDecl) -> String {
    format!("is{}", caller.name)
}

/// The capability of each actor and the caller interface of each use case (#337): a prepare
/// or an entry point takes the caller first, so code that holds no capability of an actor that
/// performs the use case does not compile. A capability is a struct with an unexported field:
/// only `Issue<Actor>Capability` makes a valid one. Go lets code outside the package write
/// the zero value, which `Valid` reports false for, and a nil caller; `CallerValid` refuses
/// both, and a service calls it.
fn print_capabilities(domain: &DomainModel, port: &super::ir::PortDecl, out: &mut String) {
    if domain.capabilities.is_empty() {
        return;
    }
    let valid = super::go::derived(domain, "go:callervalid", || "CallerValid".to_string());
    out.push_str("// ── Capabilities: who may run each use case ──\n\n");
    for cap in &domain.capabilities {
        let issue = issue(domain, cap);
        out.push_str(&doc(
            &cap.name,
            &[
                format!(
                    "is the capability of the actor {} ({}): the use cases it performs take it.",
                    cap.actor,
                    super::one_line(&cap.label)
                ),
                format!("Only {issue} makes a valid one, which the code calls once it has authenticated a {}; rdra-ish does not check who calls it. Go lets code outside this package write the zero value, {}{{}}: it is not valid (Valid reports false), and {valid} refuses it.", cap.actor, cap.name),
            ],
            "",
        ));
        out.push_str(&struct_decl(
            &cap.name,
            &[Line::Cells(vec!["issued".into(), "bool".into()])],
        ));
        out.push_str(&format!(
            "// {issue} returns the capability, for code that has authenticated a {1}.\nfunc {issue}() {0} {{\n\treturn {0}{{issued: true}}\n}}\n\n",
            cap.name, cap.actor
        ));
        out.push_str(&format!(
            "// Valid reports whether {issue} made the capability: the zero value is not valid.\nfunc (c {}) Valid() bool {{\n\treturn c.issued\n}}\n\n",
            cap.name
        ));
    }
    out.push_str(&format!(
        "// {valid} reports whether by is a capability an Issue function made: false for a nil caller\n// and for the zero value of a capability. A service refuses a call whose caller is not valid.\nfunc {valid}(by interface{{ Valid() bool }}) bool {{\n\treturn by != nil && by.Valid()\n}}\n\n"
    ));
    for c in &port.contracts {
        let Some(caller) = &c.caller else {
            continue;
        };
        let caps: Vec<&str> = caller
            .actors
            .iter()
            .map(|&i| domain.capabilities[i].name.as_str())
            .collect();
        out.push_str(&format!(
            "// {} is who may run {}: the capability of an actor that performs it ({}).\n// A capability of another actor does not implement it, so a call with one does not compile.\ntype {0} interface {{\n\t// Valid reports whether the capability was issued.\n\tValid() bool\n\t{}()\n}}\n\n",
            caller.name,
            c.usecase,
            caps.join(", "),
            caller_marker(caller)
        ));
        for cap in &caps {
            out.push_str(&format!(
                "func ({cap}) {}() {{}}\n\n",
                caller_marker(caller)
            ));
        }
    }
}

/// One interface per use case (docs/design/code-check.md, decision 6): the prepare and commit
/// methods of its actions. A service implements each; the probe names each.
fn print_contracts(domain: &DomainModel, port: &super::ir::PortDecl, out: &mut String) {
    if port.contracts.is_empty() {
        return;
    }
    print_capabilities(domain, port, out);
    out.push_str("// ── Contracts: one per use case, which the service implements ──\n\n");
    for c in &port.contracts {
        let at = c
            .site
            .as_ref()
            .map(|s| format!(" ({s})"))
            .unwrap_or_default();
        out.push_str(&doc(
            &c.name,
            &[
                format!("is the use case {}{at}: {}.", c.usecase, super::one_line(&c.label)),
                format!("Each action is two calls: Prepare* loads its rows by id and calls the dispatcher or transition function, writing nothing; the matching Commit* stores the rows, marks, and events the result holds in one transaction, each row only where it still is what the result's <Row>Expected names. Either returns an error, a *{} where the use case does not run, and changes nothing then.", c.error),
            ],
            "",
        ));
        out.push_str(&format!("type {} interface {{\n", c.name));
        let by: Vec<String> = c
            .caller
            .iter()
            .map(|caller| format!("by {}", caller.name))
            .collect();
        for &i in &c.methods {
            let m = &port.methods[i];
            out.push_str(&format!(
                "\t// Prepare{0} loads the rows of {1} (the action {2}) and runs it, writing nothing.\n\tPrepare{0}({3}) ({4}, error)\n",
                m.name,
                m.usecase,
                m.action,
                by.iter().cloned().chain(params(m)).collect::<Vec<_>>().join(", "),
                m.write
            ));
            out.push_str(&format!(
                "\t// Commit{0} stores what Prepare{0} returned in one transaction.\n\tCommit{0}(write {1}) error\n",
                m.name, m.write
            ));
        }
        for e in &c.entries {
            out.push_str(&format!(
                "\t// {} runs the action {} of {} whole, by the ids of its rows: the generator printed no\n\t// function for it (#335), so the service writes it and keeps the obligations.\n\t{0}({}) error\n",
                entry_method(e),
                e.action,
                c.usecase,
                by.iter().cloned().chain(entry_params(e)).collect::<Vec<_>>().join(", ")
            ));
        }
        out.push_str("}\n\n");
    }
}

/// The method of an entry point (#335): `Run<Action>`.
pub(super) fn entry_method(e: &super::ir::EntryDecl) -> String {
    format!("Run{}", e.name)
}

fn entry_params(e: &super::ir::EntryDecl) -> Vec<String> {
    e.rows
        .iter()
        .map(|(n, ty)| format!("{} {ty}", local(n)))
        .collect()
}

/// The Go names of a contract's methods: `Prepare*` and `Commit*` per action with functions,
/// and `Run*` per entry point.
pub(super) fn method_names(port: &super::ir::PortDecl, c: &super::ir::ContractDecl) -> Vec<String> {
    let mut out = Vec::new();
    for &i in &c.methods {
        let m = &port.methods[i];
        out.push(format!("Prepare{}", m.name));
        out.push(format!("Commit{}", m.name));
    }
    out.extend(c.entries.iter().map(entry_method));
    out
}

/// The test the Go probe holds when bound to traces: `check --code=run` runs it.
pub const CONFORMANCE_TEST: &str = "TestRdraConformance";

/// The probe (docs/design/code-check.md): a test file of the service's package, `package
/// <svc>_test`, that assigns the service to each use case's contract on the use case's line and
/// names each of the contract's methods on a line of its own (Go names one missing method per
/// interface; a method expression per method names them all); and `TestRdraConformance` when
/// bound. `binding.service` is `<import path>.<Type>`, `binding.module` the domain's import
/// path, and the traces' path is from the probe's directory, where `go test` runs a test.
pub(super) fn probe(
    domain: &DomainModel,
    binding: &super::ProbeBinding,
) -> Option<super::probe::Probe> {
    let port = domain.port.as_ref()?;
    if port.contracts.is_empty() {
        return None;
    }
    let (svc_import, ty) = binding
        .service
        .rsplit_once('.')
        .filter(|(_, ty)| !ty.contains('/'))?;
    let qualifier = |path: &str| {
        path.rsplit('/')
            .next()
            .unwrap_or(path)
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
    };
    // The domain package by its `package` clause when the binding names it: an import path
    // need not end in the package's name.
    let dom = binding
        .package
        .clone()
        .unwrap_or_else(|| qualifier(&binding.module));
    let same = binding.module == svc_import;
    let svc = if same {
        dom.clone()
    } else {
        qualifier(svc_import)
    };
    let conformance = binding.conformance.as_ref().filter(|_| domain.harness);
    let mut w =
        super::probe::ProbeWriter::new("// Code generated by rdra-ish codegen. DO NOT EDIT.");
    w.line(&format!(
        "//\n// The contract probe of `rdra-ish check --code`: each use case line states that *{svc}.{ty}\n// implements the use case's contract, and the lines under it name each of the contract's\n// methods, so the compiler names every method the service lacks. `check --code=build` compiles\n// it; `check --code=run` runs {CONFORMANCE_TEST}. `rdra-ish codegen --code` writes it again.\n\npackage {svc}_test\n"
    ));
    let mut imports = String::from("import (\n");
    if conformance.is_some() {
        imports.push_str("\t\"os\"\n\t\"testing\"\n\n");
    }
    if !same {
        imports.push_str(&format!("\t{dom} {}\n", string_literal(&binding.module)));
    }
    imports.push_str(&format!("\t{}\n)\n", string_literal(svc_import)));
    w.line(&imports);
    let receiver = format!("(*{svc}.{ty})");
    for c in &port.contracts {
        let site = c
            .site
            .as_ref()
            .map(|s| format!(" ({s})"))
            .unwrap_or_default();
        w.line(&format!(
            "// usecase {} {}{site}",
            c.usecase,
            string_literal(&super::one_line(&c.label))
        ));
        w.usecase(&format!("var _ {dom}.{} = {receiver}(nil)", c.name), c);
        for name in method_names(port, c) {
            w.line(&format!("var _ = {receiver}.{name}"));
        }
        w.line("");
    }
    if let Some((new, traces)) = conformance {
        w.line(&format!(
            "// {CONFORMANCE_TEST} drives a fresh service per trace along the model's traces\n// ({traces}): `rdra-ish check --code=run` runs it.\nfunc {CONFORMANCE_TEST}(t *testing.T) {{\n\ttraces, err := os.ReadFile({})\n\tif err != nil {{\n\t\tt.Fatal(err)\n\t}}\n\treport, err := {dom}.Conform(func() {dom}.ConformancePort {{ return {new} }}, string(traces))\n\tif err != nil {{\n\t\tt.Fatal(\"rdra-ish conformance: \" + err.Error())\n\t}}\n\tt.Logf(\"ok %d traces, %d calls, %d refusals, %d interleavings\", report.Traces, report.Calls, report.Refusals, report.Interleavings)\n}}",
            string_literal(traces)
        ));
    }
    let mut probe = w.finish();
    // The file is written with one newline at its end.
    while probe.text.ends_with('\n') {
        probe.text.pop();
    }
    Some(probe)
}

pub(super) fn print_port(domain: &DomainModel, out: &mut String) {
    let Some(port) = &domain.port else {
        return;
    };
    print_contracts(domain, port, out);
    if !domain.harness {
        return;
    }
    out.push_str(
        "// ── Conformance: the port a finished service implements, and the harness that drives it ──\n\n",
    );
    for r in &port.runs {
        out.push_str(&doc(
            &r.name,
            &[format!(
                "is a run mark of {} on a {} row, as a store keeps it: the last step of the run that wrote the row.",
                r.usecase, r.entity
            )],
            "",
        ));
        out.push_str(&format!(
            "type {} struct {{\n\tId   {}\n\tStep int\n}}\n\n",
            r.name, r.id_ty
        ));
    }
    out.push_str("// StoreSnapshot is everything a store holds, as the conformance harness reads it: the rows of\n// each entity, the pending run marks, and the events raised on rows that a triggered use case reads.\n");
    let mut lines = Vec::new();
    for e in &port.entities {
        lines.push(Line::Cells(vec![
            type_ident(&e.entity),
            format!("[]{}", e.ty),
        ]));
    }
    for r in &port.runs {
        lines.push(Line::Cells(vec![
            format!("{}s", r.name),
            format!("[]{}", r.name),
        ]));
    }
    for ev in &port.events {
        lines.push(Line::Cells(vec![ev.ty.clone(), format!("[]{}", ev.ty)]));
    }
    out.push_str(&struct_decl("StoreSnapshot", &lines));

    out.push_str("// ConformancePort is what a finished service offers the conformance harness: every use case's\n// contract, and a store the harness can seed and read back.\n");
    out.push_str("type ConformancePort interface {\n");
    for c in &port.contracts {
        out.push_str(&format!("\t{}\n", c.name));
    }
    out.push_str(
        "\t// Seed empties the store and stores these rows: the initial state of a trace.\n",
    );
    out.push_str("\tSeed(rows StoreSnapshot)\n");
    out.push_str("\t// Snapshot returns everything the store holds now.\n");
    out.push_str("\tSnapshot() StoreSnapshot\n");
    out.push_str("}\n\n");

    out.push_str("// ConformanceReport is what Conform checked.\ntype ConformanceReport struct {\n\tTraces        int\n\tCalls         int\n\tRefusals      int\n\tInterleavings int\n}\n\n");

    out.push_str(&format!(
        r##"// Conform drives a fresh port per trace along traces (the file `codegen --conformance` writes):
// it seeds each trace's initial state, makes each call, and after each compares what the store
// holds with the state the model reaches. A call the model refuses must return an error and
// change nothing, and so must a commit whose rows another call changed after its prepare. It
// returns the first difference.
func Conform(newPort func() ConformancePort, traces string) (ConformanceReport, error) {{
	var report ConformanceReport
	lines := strings.Split(traces, "\n")
	if len(lines) == 0 || strings.TrimRight(lines[0], "\r ") != {format} {{
		return report, errors.New("not a traces file of this version: generate the code and the traces again")
	}}
	states := map[string][]string{{}}
	defining := ""
	var port ConformancePort
	var seed StoreSnapshot
	var want []string
	for n, line := range lines[1:] {{
		at := func(err error) error {{ return fmt.Errorf("traces line %d: %w", n+2, err) }}
		toks := strings.Fields(line)
		if len(toks) == 0 {{
			continue
		}}
		rest := toks[1:]
		state := func(i int) ([]string, error) {{
			if i < len(rest) {{
				if s, ok := states[rest[i]]; ok {{
					return s, nil
				}}
			}}
			return nil, at(errors.New("names no state defined above"))
		}}
		switch toks[0] {{
		case "#":
		case "S":
			if len(rest) == 0 {{
				return report, at(errors.New("a state without a number"))
			}}
			defining = rest[0]
			states[defining] = []string{{}}
		case "A":
			states[defining] = append(states[defining], strings.Join(rest, " "))
		case "T":
			s, err := state(0)
			if err != nil {{
				return report, err
			}}
			want = s
			port = newPort()
			seed = StoreSnapshot{{}}
			report.Traces++
		case "R":
			if err := confSeedRow(&seed, rest); err != nil {{
				return report, at(err)
			}}
		case "G":
			if port == nil {{
				return report, at(errors.New("seeding outside a trace"))
			}}
			port.Seed(seed)
			seed = StoreSnapshot{{}}
			if err := confCompare(confAbstract(port.Snapshot()), want, "after seeding the initial state"); err != nil {{
				return report, at(err)
			}}
		case "C":
			next, err := state(0)
			if err != nil {{
				return report, err
			}}
			if port == nil || len(rest) < 2 {{
				return report, at(errors.New("a call outside a trace"))
			}}
			call := rest[1:]
			refused, err := confRun(port, call)
			if err != nil {{
				return report, at(err)
			}}
			if refused != nil {{
				return report, at(fmt.Errorf("the model runs `%s` here, and the service refused it: %v", strings.Join(call, " "), refused))
			}}
			if err := confCompare(confAbstract(port.Snapshot()), next, "after `"+strings.Join(call, " ")+"`"); err != nil {{
				return report, at(err)
			}}
			want = next
			report.Calls++
		case "X":
			if port == nil {{
				return report, at(errors.New("a call outside a trace"))
			}}
			refused, err := confRun(port, rest)
			if err != nil {{
				return report, at(err)
			}}
			if refused == nil {{
				return report, at(fmt.Errorf("the model refuses `%s` here, and the service ran it", strings.Join(rest, " ")))
			}}
			if err := confCompare(confAbstract(port.Snapshot()), want, "after the refused `"+strings.Join(rest, " ")+"`"); err != nil {{
				return report, at(err)
			}}
			report.Refusals++
		case "I":
			next, err := state(0)
			if err != nil {{
				return report, err
			}}
			split := -1
			for i, t := range rest {{
				if t == "|" {{
					split = i
					break
				}}
			}}
			if port == nil || split < 1 {{
				return report, at(errors.New("an interleaving outside a trace, or without `|`"))
			}}
			first, second := rest[1:split], rest[split+1:]
			commit, refused, err := confPrepare(port, first)
			if err != nil {{
				return report, at(err)
			}}
			if refused != nil {{
				return report, at(fmt.Errorf("the model runs `%s` here, and the service refused it: %v", strings.Join(first, " "), refused))
			}}
			refused, err = confRun(port, second)
			if err != nil {{
				return report, at(err)
			}}
			if refused != nil {{
				return report, at(fmt.Errorf("the model runs `%s` here, and the service refused it: %v", strings.Join(second, " "), refused))
			}}
			if err := confCompare(confAbstract(port.Snapshot()), next, "after `"+strings.Join(second, " ")+"`"); err != nil {{
				return report, at(err)
			}}
			if commit() == nil {{
				return report, at(fmt.Errorf("the service committed `%s`, prepared before `%s` changed a row it writes: the write must be conditional on what the row was", strings.Join(first, " "), strings.Join(second, " ")))
			}}
			if err := confCompare(confAbstract(port.Snapshot()), next, "after the refused commit of `"+strings.Join(first, " ")+"`"); err != nil {{
				return report, at(err)
			}}
			want = next
			report.Interleavings++
		default:
			return report, at(fmt.Errorf("unknown record %s", toks[0]))
		}}
	}}
	return report, nil
}}

// confRun prepares and commits a call: the refusal, or an error in the trace.
func confRun(confP ConformancePort, call []string) (error, error) {{
	commit, refused, err := confPrepare(confP, call)
	if err != nil || refused != nil {{
		return refused, err
	}}
	return commit(), nil
}}

// confCompare reports every line the store and the model's state do not share.
func confCompare(got, want []string, when string) error {{
	if strings.Join(got, "\n") == strings.Join(want, "\n") {{
		return nil
	}}
	has := func(lines []string, line string) bool {{
		for _, l := range lines {{
			if l == line {{
				return true
			}}
		}}
		return false
	}}
	out := when + ", the store differs from the model:"
	for _, l := range want {{
		if !has(got, l) {{
			out += "\n  model only: " + l
		}}
	}}
	for _, l := range got {{
		if !has(want, l) {{
			out += "\n  store only: " + l
		}}
	}}
	return errors.New(out)
}}

// confArgs reads the values of a trace record; the first error sticks.
type confArgs struct {{
	toks []string
	err  error
}}

func (a *confArgs) next() string {{
	if len(a.toks) == 0 {{
		if a.err == nil {{
			a.err = errors.New("too few values")
		}}
		return "~"
	}}
	t := a.toks[0]
	a.toks = a.toks[1:]
	return t
}}

func (a *confArgs) int(t string) int64 {{
	n, err := strconv.ParseInt(t, 10, 64)
	if err != nil && a.err == nil {{
		a.err = err
	}}
	return n
}}

func (a *confArgs) bool(t string) bool {{
	b, err := strconv.ParseBool(t)
	if err != nil && a.err == nil {{
		a.err = err
	}}
	return b
}}

func confEnum[T interface{{ Valid() bool }}](a *confArgs, v T) T {{
	if !v.Valid() && a.err == nil {{
		a.err = fmt.Errorf("not a value: %v", v)
	}}
	return v
}}

func confOpt[T any](t string, f func(string) T) *T {{
	if t == "~" {{
		return nil
	}}
	v := f(t)
	return &v
}}

func confText[T any](p *T, f func(T) string) string {{
	if p == nil {{
		return "~"
	}}
	return f(*p)
}}

func confSet[T any](p *T) string {{
	if p == nil {{
		return "~"
	}}
	return "+"
}}

"##,
        format = string_literal(TRACES_FORMAT),
    ));

    // The calls: the commit, the refusal, or an error in the trace.
    out.push_str(
        "func confPrepare(confP ConformancePort, call []string) (func() error, error, error) {\n",
    );
    out.push_str(
        "\tif len(call) == 0 {\n\t\treturn nil, nil, errors.New(\"a call names no use case\")\n\t}\n",
    );
    out.push_str("\tconfA := &confArgs{toks: call[1:]}\n\t_ = confA\n");
    out.push_str("\tswitch call[0] {\n");
    for (k, m) in port.methods.iter().enumerate() {
        out.push_str(&format!("\tcase {}:\n", string_literal(&m.action)));
        let mut args = Vec::new();
        // The harness runs each call as the first actor that performs the use case.
        let caller = port
            .contracts
            .iter()
            .find(|c| c.methods.contains(&k))
            .and_then(|c| c.caller.as_ref());
        if let Some(caller) = caller {
            args.push(format!(
                "{}()",
                issue(domain, &domain.capabilities[caller.actors[0]])
            ));
        }
        for (name, ty) in m.rows.iter().chain(&m.marks) {
            let v = local(&format!("conf_{name}"));
            out.push_str(&format!(
                "\t\t{v} := {}\n",
                parse(domain, &FieldType::Id(ty.clone()), false)
            ));
            args.push(v);
        }
        if let Some((name, ty)) = &m.event {
            let v = local(&format!("conf_{name}"));
            let fields: Vec<String> = domain
                .events
                .iter()
                .find(|e| e.name == *ty)
                .map(|e| e.fields.clone())
                .unwrap_or_default()
                .iter()
                .map(|f| {
                    format!(
                        "{}: {}",
                        type_ident(&f.name),
                        parse(domain, &FieldType::Id(f.id_ty.clone()), false)
                    )
                })
                .collect();
            out.push_str(&format!("\t\t{v} := {ty}{{{}}}\n", fields.join(", ")));
            args.push(v);
        }
        for i in &m.inputs {
            let v = local(&format!("conf_{}", i.name));
            out.push_str(&format!(
                "\t\t{v} := {}\n",
                parse(domain, &i.ty, i.nullable)
            ));
            args.push(v);
        }
        for (name, ty) in &m.choices {
            let v = local(&format!("conf_{name}"));
            out.push_str(&format!(
                "\t\t{v} := {}\n",
                parse(domain, &FieldType::Enum(ty.clone()), false)
            ));
            args.push(v);
        }
        out.push_str("\t\tif confA.err != nil {\n\t\t\treturn nil, nil, confA.err\n\t\t}\n");
        out.push_str(&format!(
            "\t\tw, refused := confP.Prepare{0}({1})\n\t\treturn func() error {{ return confP.Commit{0}(w) }}, refused, nil\n",
            m.name,
            args.join(", ")
        ));
    }
    out.push_str(
        "\t}\n\treturn nil, nil, fmt.Errorf(\"no use case %s in this port\", call[0])\n}\n\n",
    );

    // The rows of an initial state.
    out.push_str("func confSeedRow(seed *StoreSnapshot, toks []string) error {\n");
    out.push_str("\tif len(toks) == 0 {\n\t\treturn errors.New(\"a row names no entity\")\n\t}\n");
    out.push_str("\tconfA := &confArgs{toks: toks[1:]}\n\t_ = confA\n");
    out.push_str("\tswitch toks[0] {\n");
    for e in &port.entities {
        let fields: Vec<String> = e
            .columns
            .iter()
            .map(|f| format!("{}: {}", f.pascal, parse(domain, &f.ty, f.nullable)))
            .collect();
        let field = type_ident(&e.entity);
        out.push_str(&format!(
            "\tcase {}:\n\t\tseed.{field} = append(seed.{field}, {}{{{}}})\n\t\treturn confA.err\n",
            string_literal(&e.entity),
            e.ty,
            fields.join(", ")
        ));
    }
    out.push_str("\t}\n\treturn fmt.Errorf(\"no entity %s in this port\", toks[0])\n}\n\n");

    // The abstraction.
    out.push_str("// confAbstract spells the store as the traces spell a state: one line per row with the columns\n// the model tracks, per pending run mark, and per raise a triggered use case reads; sorted.\n");
    out.push_str("func confAbstract(s StoreSnapshot) []string {\n\tout := []string{}\n");
    for e in &port.entities {
        out.push_str(&format!(
            "\tfor _, row := range s.{} {{\n",
            type_ident(&e.entity)
        ));
        let mut parts = vec![format!(
            "{}+{}",
            string_literal(&format!("{} ", e.entity)),
            text(domain, &format!("row.{}", e.id.pascal), &e.id.ty)
        )];
        for (field, how) in &e.tracked {
            parts.push(format!(
                "{}+{}",
                string_literal(&format!(" {}=", field.wire)),
                abstracted(domain, field, *how)
            ));
        }
        out.push_str(&format!(
            "\t\tout = append(out, {})\n\t}}\n",
            parts.join("+")
        ));
    }
    for r in &port.runs {
        out.push_str(&format!(
            "\tfor _, m := range s.{}s {{\n\t\tout = append(out, {}+{}+\" \"+strconv.Itoa(m.Step))\n\t}}\n",
            r.name,
            string_literal(&format!("mark {} {} ", r.usecase, r.entity)),
            text(domain, "m.Id", &FieldType::Id(r.id_ty.clone()))
        ));
    }
    for ev in &port.events {
        out.push_str(&format!("\tfor _, ev := range s.{} {{\n", ev.ty));
        for (field, tracked) in &ev.ids {
            if *tracked {
                out.push_str(&format!(
                    "\t\tout = append(out, {}+{})\n",
                    string_literal(&format!("raised {} {} ", ev.ty, field.entity)),
                    text(
                        domain,
                        &format!("ev.{}", type_ident(&field.name)),
                        &FieldType::Id(field.id_ty.clone())
                    )
                ));
            }
        }
        out.push_str("\t}\n");
    }
    out.push_str("\tsort.Strings(out)\n\tdedup := out[:0]\n\tfor i, l := range out {\n\t\tif i == 0 || l != out[i-1] {\n\t\t\tdedup = append(dedup, l)\n\t\t}\n\t}\n\treturn dedup\n}\n\n");
}

fn params(m: &super::ir::PortMethod) -> Vec<String> {
    let mut out: Vec<String> = m
        .rows
        .iter()
        .chain(&m.marks)
        .map(|(n, ty)| format!("{} {ty}", local(n)))
        .collect();
    if let Some((n, ty)) = &m.event {
        out.push(format!("{} {ty}", local(n)));
    }
    for i in &m.inputs {
        let ty = field_type(&i.ty);
        let ty = if i.nullable { format!("*{ty}") } else { ty };
        out.push(format!("{} {ty}", local(&i.name)));
    }
    for (n, ty) in &m.choices {
        out.push(format!("{} {ty}", local(n)));
    }
    out
}

fn id_repr(domain: &DomainModel, id: &str) -> Scalar {
    domain
        .ids
        .iter()
        .find(|i| i.name == id)
        .map_or(Scalar::Int, |i| i.repr)
}

/// An expression that reads the next value of `confA` as `ty`.
fn parse(domain: &DomainModel, ty: &FieldType, nullable: bool) -> String {
    let value = |t: &str| -> String {
        match ty {
            FieldType::Scalar(s) => scalar_parse(*s, t),
            FieldType::Enum(name) => format!("confEnum(confA, {name}({t}))"),
            FieldType::Id(name) => match id_repr(domain, name) {
                Scalar::Int => format!("{name}(confA.int({t}))"),
                _ => format!("{name}({t})"),
            },
            FieldType::Rows(_) => unreachable!("a list of rows is no port argument"),
        }
    };
    if nullable {
        format!(
            "confOpt(confA.next(), func(t string) {} {{ return {} }})",
            field_type(ty),
            value("t")
        )
    } else {
        value("confA.next()")
    }
}

fn scalar_parse(s: Scalar, t: &str) -> String {
    match s {
        Scalar::Int => format!("confA.int({t})"),
        Scalar::Bool => format!("confA.bool({t})"),
        Scalar::String => t.to_string(),
        other => format!("{}({t})", other.name()),
    }
}

/// The text of the value `e` of type `ty` (not null), as the traces spell it.
fn text(domain: &DomainModel, e: &str, ty: &FieldType) -> String {
    match ty {
        FieldType::Scalar(Scalar::Int) => format!("strconv.FormatInt({e}, 10)"),
        FieldType::Scalar(Scalar::Bool) => format!("strconv.FormatBool({e})"),
        FieldType::Scalar(_) | FieldType::Enum(_) => format!("string({e})"),
        FieldType::Id(name) => match id_repr(domain, name) {
            Scalar::Int => format!("strconv.FormatInt(int64({e}), 10)"),
            Scalar::Bool => format!("strconv.FormatBool(bool({e}))"),
            _ => format!("string({e})"),
        },
        FieldType::Rows(_) => unreachable!("a list of rows is no column"),
    }
}

/// The tracked column `field` of `row`, abstracted as the model holds it.
fn abstracted(domain: &DomainModel, field: &Field, how: Track) -> String {
    let place = format!("row.{}", field.pascal);
    match (how, field.nullable) {
        (Track::Presence, true) => format!("confSet({place})"),
        (Track::Presence, false) => "\"+\"".to_string(),
        (Track::Exact, true) => format!(
            "confText({place}, func(v {}) string {{ return {} }})",
            field_type(&field.ty),
            text(domain, "v", &field.ty)
        ),
        (Track::Exact, false) => text(domain, &place, &field.ty),
    }
}
