//! How `diagram` is narrowed and written: `--actor`, `--entity` beyond the state diagram,
//! `--all`, `--title`, `--legend`, and what it says on stderr about a diagram Mermaid's
//! defaults refuse or one that draws nothing.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[path = "common/root.rs"]
mod root;
#[path = "common/scratch.rs"]
mod scratch;
use root::repo;
use scratch::scratch;

const SHOP: &str = r#"
actor Clerk "Clerk"
actor Buyer "Buyer"
business Sales "Sales"
buc BucOrder "Order"
belongs(BucOrder, Sales)
usecase Place "Place order"
usecase Ship "Ship order"
usecase Audit "Audit customers"
contains(BucOrder, Place)
contains(BucOrder, Ship)
contains(BucOrder, Audit)
performs(Buyer, Place)
performs(Clerk, Ship)
performs(Clerk, Audit)
entity Customer "Customer" { id: Int @pk }
entity Order "Order" {
  id: Int @pk
  status: Enum(open, shipped) @default(open)
}
entity Line "Line" { id: Int @pk }
entity Coupon "Coupon" { id: Int @pk }
relate(Order, Customer, N:1)
relate(Line, Order, N:1)
api ShipApi "Ship API"
invokes(Ship, ShipApi)
creates(Place, Order)
updates(ShipApi, Order)
transitions(Ship, Order.status, open -> shipped)
reads(Audit, Customer)
event Shipped "Shipped"
raises(Ship, Shipped)
outbox(Shipped)
"#;

fn model(test: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(test);
    let path = dir.join("shop.rdra");
    std::fs::write(&path, SHOP).unwrap();
    (dir, path)
}

fn diagram(model: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .current_dir(repo())
        .arg("diagram")
        .arg(model)
        .args(args)
        .output()
        .expect("rdra-ish runs")
}

fn stdout(output: &Output) -> String {
    assert!(output.status.success(), "{output:?}");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// `--actor` draws the use cases the actor performs, and `--entity` the ones that read or
/// write the entity, an invoked API's writes included.
#[test]
fn actor_and_entity_pick_the_use_cases_of_a_usecase_diagram() {
    let (_, shop) = model("diagram_scope_usecase");
    let by_actor = stdout(&diagram(&shop, &["--kind", "usecase", "--actor", "Clerk"]));
    assert!(
        by_actor.starts_with("%% rdra-ish diagram=usecase scope=usecase:Audit,Ship "),
        "{by_actor}"
    );
    assert!(!by_actor.contains("usecase__Place"), "{by_actor}");
    let by_entity = stdout(&diagram(
        &shop,
        &["--kind", "robustness", "--entity", "Order"],
    ));
    assert!(
        by_entity.starts_with("%% rdra-ish diagram=robustness scope=usecase:Place,Ship "),
        "{by_entity}"
    );
    for (args, said) in [
        (
            vec!["--kind", "flow", "--actor", "Clerk"],
            "--actor applies only to --kind usecase or robustness",
        ),
        (
            vec!["--kind", "usecase", "--actor", "Nobody"],
            "unknown actor `Nobody`",
        ),
        (
            vec!["--kind", "usecase", "--actor", "Clerk", "--buc", "BucOrder"],
            "--buc and --actor cannot be combined",
        ),
        (
            vec!["--kind", "usecase", "--entity", "Coupon"],
            "no use case reads or writes Coupon",
        ),
        (
            vec!["--kind", "flow", "--entity", "Order"],
            "--entity applies only to --kind state",
        ),
    ] {
        let output = diagram(&shop, &args);
        assert!(!output.status.success(), "{args:?}");
        assert!(
            stderr(&output).contains(said),
            "{args:?}: {}",
            stderr(&output)
        );
    }
}

/// `--entity` on the data diagrams draws the entity and the ones a `relate` joins it to.
#[test]
fn entity_draws_its_neighbourhood_in_the_data_diagrams() {
    let (_, shop) = model("diagram_scope_entity");
    let er = stdout(&diagram(&shop, &["--kind", "er", "--entity", "Customer"]));
    assert!(
        er.starts_with("%% rdra-ish diagram=er scope=entity:Customer "),
        "{er}"
    );
    assert!(
        er.contains("  Customer {") && er.contains("  Order {"),
        "{er}"
    );
    assert!(
        !er.contains("  Line {") && !er.contains("  Coupon {"),
        "{er}"
    );
    let information = stdout(&diagram(
        &shop,
        &["--kind", "information", "--entity", "Order"],
    ));
    for (id, drawn) in [
        ("Customer", true),
        ("Order", true),
        ("Line", true),
        ("Coupon", false),
    ] {
        assert_eq!(
            information.contains(&format!("entity__{id}")),
            drawn,
            "{id}\n{information}"
        );
    }
}

/// A diagram with nothing to draw says so on stderr, and still prints its header.
#[test]
fn an_empty_diagram_is_noted() {
    let (_, shop) = model("diagram_scope_empty");
    let output = diagram(&shop, &["--kind", "flow"]);
    assert!(stdout(&output).contains("%% kinds: (none)"));
    assert_eq!(
        stderr(&output).lines().last(),
        Some("note: nothing to draw: the model declares nothing --kind flow draws in this scope")
    );
    assert!(stderr(&diagram(&shop, &["--kind", "er"]))
        .lines()
        .all(|l| !l.starts_with("note: nothing")));
}

/// A whole-model diagram of a large sample passes Mermaid's default limits, and the
/// warning names them and how to narrow the diagram; a BUC of it does not warn.
#[test]
fn a_diagram_past_mermaids_limits_warns() {
    let clinic = repo().join("samples/clinic-ops");
    let whole = diagram(&clinic, &["--kind", "usecase"]);
    assert!(whole.status.success());
    let warned = stderr(&whole);
    assert!(
        warned.contains(
            "Mermaid's defaults stop at 50000 characters (maxTextSize) and 500 edges (maxEdges)"
        ),
        "{warned}"
    );
    assert!(
        warned.contains("Narrow it with --buc, --usecase, --actor, or --entity"),
        "{warned}"
    );
    let one = diagram(&clinic, &["--kind", "usecase", "--buc", "BucFollowupCare"]);
    assert!(
        !stderr(&one).contains("Mermaid's defaults"),
        "{}",
        stderr(&one)
    );
}

/// `--title` opens the text with a Mermaid title naming the diagram and its scope, and
/// `--legend` adds one node per node class before the class definitions.
#[test]
fn title_and_legend_say_what_a_rendered_diagram_shows() {
    let (_, shop) = model("diagram_scope_dress");
    let text = stdout(&diagram(
        &shop,
        &[
            "--kind", "usecase", "--buc", "BucOrder", "--title", "--legend",
        ],
    ));
    assert!(
        text.starts_with(
            "---\ntitle: \"UC複合図: Order\"\n---\n%% rdra-ish diagram=usecase scope=buc:BucOrder "
        ),
        "{text}"
    );
    let legend = text.find("  subgraph legend[\"凡例\"]").expect("a legend");
    assert!(legend < text.find("  classDef ").unwrap(), "{text}");
    assert!(
        text.contains("    legend__actor[\"actor\"]:::actor\n"),
        "{text}"
    );
    // An ER diagram has no node classes: the title alone.
    let er = stdout(&diagram(&shop, &["--kind", "er", "--title", "--legend"]));
    assert!(
        er.starts_with("---\ntitle: \"ER図\"\n---\n%% rdra-ish diagram=er scope=whole "),
        "{er}"
    );
    assert!(!er.contains("legend"), "{er}");
    let plain = stdout(&diagram(&shop, &["--kind", "usecase"]));
    assert!(plain.starts_with("%% rdra-ish diagram=usecase "), "{plain}");
}

/// `--all` writes the review set under a directory and leaves out what draws nothing.
#[test]
fn all_writes_the_review_set() {
    let (dir, shop) = model("diagram_scope_all");
    let out = dir.join("set");
    let output = diagram(&shop, &["--all", "--out", out.to_str().unwrap()]);
    let said = stdout(&output);
    let mut files: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            "business.mmd",
            "context.mmd",
            "er.mmd",
            "event.mmd",
            "information.mmd",
            "robustness__BucOrder.mmd",
            "sequence__BucOrder.mmd",
            "state.mmd",
            "state__Order.mmd",
            "usecase__BucOrder.mmd",
        ],
        "{said}"
    );
    assert!(
        said.ends_with(&format!(
            "wrote 10 diagrams to {} (4 with nothing to draw left out)\n",
            out.display()
        )),
        "{said}"
    );
    let state = std::fs::read_to_string(out.join("state__Order.mmd")).unwrap();
    assert!(
        state.starts_with("%% rdra-ish diagram=state scope=entity:Order "),
        "{state}"
    );
    for (args, said) in [
        (vec!["--all"], "pass --out <directory>"),
        (
            vec!["--all", "--buc", "BucOrder", "--out", "x"],
            "--all writes every diagram of the model",
        ),
    ] {
        let output = diagram(&shop, &args);
        assert!(!output.status.success(), "{args:?}");
        assert!(
            stderr(&output).contains(said),
            "{args:?}: {}",
            stderr(&output)
        );
    }
}

/// `diagram --help` says what the command does (#247): `--all` reads no `--kind` and refuses
/// the scopes and the rdra filters, and `--kind sequence` takes one `--usecase` or `--buc`.
#[test]
fn help_says_what_all_and_sequence_take() {
    let output = Command::new(env!("CARGO_BIN_EXE_rdra-ish"))
        .args(["diagram", "--help"])
        .output()
        .expect("rdra-ish runs");
    let help = stdout(&output)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for said in [
        "--kind is not read. Not together with --buc, --usecase, --entity, --actor, --node-kind, or --edge-kind",
        "Takes one --usecase, or --buc for the BUC's use cases one after another",
    ] {
        assert!(help.contains(said), "{said}\n{help}");
    }
    for stale in ["Not together with --kind", "needs exactly one --usecase"] {
        assert!(!help.contains(stale), "{stale}\n{help}");
    }

    // The command does what the help says.
    let (dir, shop) = model("diagram_scope_help");
    let out = dir.join("set");
    stdout(&diagram(
        &shop,
        &["--all", "--kind", "er", "--out", out.to_str().unwrap()],
    ));
    assert!(out.join("sequence__BucOrder.mmd").exists());
    for args in [
        vec!["--all", "--node-kind", "entity", "--out", "x"],
        vec!["--all", "--edge-kind", "reads", "--out", "x"],
        vec!["--all", "--actor", "Clerk", "--out", "x"],
    ] {
        assert!(!diagram(&shop, &args).status.success(), "{args:?}");
    }
    stdout(&diagram(
        &shop,
        &["--kind", "sequence", "--usecase", "Place"],
    ));
    stdout(&diagram(
        &shop,
        &["--kind", "sequence", "--buc", "BucOrder"],
    ));
    let two = diagram(
        &shop,
        &[
            "--kind",
            "sequence",
            "--usecase",
            "Place",
            "--usecase",
            "Ship",
        ],
    );
    assert!(!two.status.success());
}
