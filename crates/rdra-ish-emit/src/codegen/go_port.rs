//! The Go conformance port and harness: `StoreSnapshot`, `ConformancePort`, and `Conform`,
//! which drives a port along the traces `conformance::traces` writes.

use super::conformance::TRACES_FORMAT;
use super::go::{doc, field_type, local, struct_decl, Line};
use super::ir::{DomainModel, Field, FieldType, Scalar, Track};
use super::{string_literal, type_ident};

/// The packages the harness imports.
pub(super) const IMPORTS: [&str; 5] = ["errors", "fmt", "sort", "strconv", "strings"];

pub(super) fn print_port(domain: &DomainModel, out: &mut String) {
    let Some(port) = domain.port.as_ref().filter(|_| domain.harness) else {
        return;
    };
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

    out.push_str("// ConformancePort is what a finished service offers the conformance harness. A use case is two\n// calls: each Prepare* method loads its rows by id and calls the dispatcher or transition\n// function, writing nothing; the matching Commit* stores the rows, marks, and events the result\n// holds in one transaction, each row only where it still is what the result's <Row>Expected\n// names. Either returns an error and changes nothing when the use case does not run.\n");
    out.push_str("type ConformancePort interface {\n");
    out.push_str(
        "\t// Seed empties the store and stores these rows: the initial state of a trace.\n",
    );
    out.push_str("\tSeed(rows StoreSnapshot)\n");
    out.push_str("\t// Snapshot returns everything the store holds now.\n");
    out.push_str("\tSnapshot() StoreSnapshot\n");
    for m in &port.methods {
        out.push_str(&format!(
            "\t// Prepare{0} reads what {1} (the action {2}) writes; Commit{0} writes it.\n",
            m.name, m.usecase, m.action
        ));
        out.push_str(&format!(
            "\tPrepare{}({}) ({}, error)\n",
            m.name,
            params(m).join(", "),
            m.write
        ));
        out.push_str(&format!("\tCommit{}(write {}) error\n", m.name, m.write));
    }
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
    for m in &port.methods {
        out.push_str(&format!("\tcase {}:\n", string_literal(&m.action)));
        let mut args = Vec::new();
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
