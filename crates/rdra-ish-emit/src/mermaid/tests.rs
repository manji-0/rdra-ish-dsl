use super::*;
use crate::diagram_common::{declare_flow_node, model_from, render_event_flow, Declared, FlowNode};
use rdra_ish_core::build_model;
use rdra_ish_syntax::parse;

/// A view that draws every state model whole, as `--entity` of each entity does.
fn all_entities(model: &SemanticModel) -> View {
    let mut view = View::whole();
    view.state.entities = model.entities.values().map(|e| e.id.clone()).collect();
    view
}

#[test]
fn test_rdra_mermaid_layers() {
    let src = r#"
actor Customer "顧客"
buc BucOrder "注文業務"
usecase PlaceOrder "注文する"
screen OrderScreen "注文画面"
api OrderApi "注文API"
entity Order "注文" { id: Int @pk }
performs(Customer, BucOrder)
contains(BucOrder, PlaceOrder)
displays(PlaceOrder, OrderScreen)
invokes(PlaceOrder, OrderApi)
creates(OrderApi, Order)
"#;
    let model = model_from(src);
    let result = RdraMermaidEmitter
        .emit(&model, &View::whole().with_model_path("m.rdra"))
        .unwrap();
    assert!(
        result.starts_with(
            "%% rdra-ish diagram=rdra scope=whole model=m.rdra\n\
             %% layers: value=システム価値 environment=システム外部環境 boundary=システム境界 system=システム\n\
             %% kinds: actor buc usecase screen api entity\n\
             %% omitted: (none)\n\
             flowchart LR\n"
        ),
        "{result}"
    );
    let value_pos = result
        .find("  subgraph layer_value[\"システム価値\"]\n")
        .unwrap();
    let boundary_pos = result
        .find("  subgraph layer_boundary[\"システム境界\"]\n")
        .unwrap();
    let system_pos = result
        .find("  subgraph layer_system[\"システム\"]\n")
        .unwrap();
    let actor_pos = result
        .find("    actor__Customer([\"顧客\"]):::actor\n")
        .unwrap();
    let screen_pos = result
        .find("    screen__OrderScreen[\"注文画面\"]:::screen\n")
        .unwrap();
    let api_pos = result
        .find("    api__OrderApi{{\"注文API\"}}:::api\n")
        .unwrap();
    assert!(value_pos < actor_pos && actor_pos < boundary_pos);
    assert!(boundary_pos < screen_pos && screen_pos < system_pos);
    assert!(system_pos < api_pos);
    assert!(result.contains("    buc__BucOrder[\"注文業務\"]:::buc\n"));
    assert!(result.contains("    entity__Order[(\"注文\")]:::entity\n"));
    assert!(result.contains("  actor__Customer -.-> buc__BucOrder\n"));
    assert!(result.contains("  buc__BucOrder --> usecase__PlaceOrder\n"));
    assert!(result.contains("  usecase__PlaceOrder -.-> api__OrderApi\n"));
    assert!(result.contains("  api__OrderApi -.->|creates| entity__Order\n"));
    assert!(result.ends_with("  classDef api fill:#c5f6fa,stroke:#0b7285,color:#000;\n  classDef entity fill:#fff9db,stroke:#5c940d,color:#000;\n"));
    assert!(!result.contains("direction TB"), "{result}");
}

#[test]
fn test_rdra_mermaid_applies_node_and_edge_filters() {
    let src = r#"
actor Customer "Customer"
usecase PlaceOrder "Place order"
api OrderApi "Order API"
entity Order "Order" { id: Int @pk }
performs(Customer, PlaceOrder)
invokes(PlaceOrder, OrderApi)
creates(OrderApi, Order)
displays(PlaceOrder, Order)
"#;
    let model = model_from(src);
    let view = View::whole().with_graph_filters(
        vec![
            "usecase".to_string(),
            "api".to_string(),
            "entity".to_string(),
        ],
        vec!["invokes".to_string(), "creates".to_string()],
    );

    let result = RdraMermaidEmitter.emit(&model, &view).unwrap();

    assert!(result.contains("usecase__PlaceOrder(\"Place order\"):::usecase"));
    assert!(result.contains("api__OrderApi{{\"Order API\"}}:::api"));
    assert!(result.contains("usecase__PlaceOrder -.-> api__OrderApi"));
    assert!(!result.contains("actor__Customer"), "{result}");
    assert!(!result.contains("performs"), "{result}");
    assert!(
        result.contains("%% kinds: usecase api entity\n"),
        "{result}"
    );
    // The header names what the filters hid: the actor, and the displays edge whose
    // ends are both drawn.
    assert!(result.contains("%% omitted: actor displays\n"), "{result}");
}

#[test]
fn test_rdra_mermaid_draws_one_edge_per_fact() {
    let src = r#"
screen StoreScreen "Store"
entity Store "Store" { id: Int @pk }
shows(StoreScreen, Store)
shows(StoreScreen, Store)
"#;
    let model = model_from(src);
    let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();
    let shows = result
        .lines()
        .filter(|line| line.contains("screen__StoreScreen -.-> entity__Store"))
        .collect::<Vec<_>>();
    assert_eq!(
        shows,
        vec!["  screen__StoreScreen -.-> entity__Store"],
        "{result}"
    );
}

#[test]
fn test_rdra_mermaid_draws_a_transition_from_its_use_case() {
    let src = r#"
buc BucPay "Pay"
usecase Capture "Capture"
entity Order "Order" {
  id: Int @pk
  status: Enum(awaiting, paid) @default(awaiting)
}
contains(BucPay, Capture)
transitions(Capture, Order.status, awaiting -> paid)
"#;
    let model = model_from(src);
    for view in [View::whole(), View::bucs(vec!["BucPay".to_string()])] {
        let result = RdraMermaidEmitter.emit(&model, &view).unwrap();
        assert!(
            result.contains(
                "  usecase__Capture -.->|transitions Order.status awaiting->paid| state__Order_paid\n"
            ),
            "{result}"
        );
        assert!(result.contains("    state__Order_awaiting(((\"awaiting\"))):::state\n"));
    }
    let view = View::whole().with_graph_filters(Vec::new(), vec!["contains".to_string()]);
    let result = RdraMermaidEmitter.emit(&model, &view).unwrap();
    assert!(!result.contains("|transitions"), "{result}");
    assert!(result.contains("%% omitted: transitions\n"), "{result}");
}

#[test]
fn test_rdra_mermaid_writes_descriptions_as_comments() {
    let src = r#"
actor Customer "Customer" description "Places orders"
usecase Browse "Browse" description "Finds products"
performs(Customer, Browse)
"#;
    let model = model_from(src);
    let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();

    assert!(
        result.contains(
            "    actor__Customer([\"Customer\"]):::actor\n    %% desc actor__Customer: Places orders\n"
        ),
        "{result}"
    );
    assert!(result.contains("    %% desc usecase__Browse: Finds products\n"));
    assert!(!result.contains("_description"), "{result}");
}
#[test]
fn test_er_mermaid_emit() {
    let src = r#"
entity Order "注文" { id: Int @pk  total: Money }
entity Customer "顧客" { id: Int @pk  name: String }
relate(Order, Customer, N:1)
"#;
    let model = model_from(src);
    let result = ErMermaidEmitter.emit(&model, &View::er()).unwrap();
    assert!(result.contains("erDiagram"));
    assert!(result.contains("Order {"));
    assert!(result.contains("Int id PK"));
    assert!(result.contains("Customer {"));
    assert!(result.contains("Int id PK"));
    assert!(result.contains("}}o--||") || result.contains("}o--||"));
}

#[test]
fn test_er_mermaid_opens_with_the_header() {
    let model = model_from("entity Order \"O\" { id: Int @pk }\n");
    let view = View::er().with_model_path("m.rdra");
    let result = ErMermaidEmitter.emit(&model, &view).unwrap();
    assert!(
        result.starts_with(
            "%% rdra-ish diagram=er scope=whole model=m.rdra\n\
             %% kinds: entity\n\
             %% omitted: (none)\n\
             erDiagram\n  Order {\n"
        ),
        "{result}"
    );
    let empty = ErMermaidEmitter
        .emit(&model_from("actor A \"A\"\n"), &View::er())
        .unwrap();
    assert_eq!(
        empty,
        "%% rdra-ish diagram=er scope=whole\n%% kinds: (none)\n%% omitted: (none)\nerDiagram\n"
    );
}

#[test]
fn test_state_mermaid_names_an_enum_a_sets_writes_apart_from_omitted() {
    let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, published) @default(draft)
  kind: Enum(memo, report) @default(memo)
  tone: Enum(plain, formal)
}
usecase Publish "公開する"
transitions(Publish, Doc.status, draft -> published)
sets(Publish, Doc.kind == report)
"#;
    let model = model_from(src);
    let result = StateMermaidEmitter
        .emit(&model, &View::whole(), &Explored::default())
        .unwrap();
    assert!(
        result.contains(
            "%% omitted: Doc.tone (Enum without transitions)\n\
             %% set-only: Doc.kind (Enum a `sets` writes and no `transitions` drive: a state axis of check, not a state model)\n"
        ),
        "{result}"
    );
}

#[test]
fn an_entity_scoped_state_diagram_names_only_that_entitys_unmodeled_columns() {
    let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, published) @default(draft)
  tone: Enum(plain, formal)
}
entity Memo "メモ" {
  id: Int @pk
  kind: Enum(note, todo) @default(note)
  mood: Enum(calm, tense)
}
usecase Publish "公開する"
usecase Jot "書く"
transitions(Publish, Doc.status, draft -> published)
sets(Jot, Memo.kind == todo)
"#;
    let model = model_from(src);
    let mut view = View::whole();
    view.state.entities = vec!["Doc".to_string()];
    let result = StateMermaidEmitter
        .emit(&model, &view, &Explored::default())
        .unwrap();
    assert!(result.contains("%% omitted: Doc.tone "), "{result}");
    assert!(!result.contains("Memo."), "{result}");
    assert!(!result.contains("set-only"), "{result}");
}

#[test]
fn test_er_mermaid_snapshot() {
    let src = r#"
entity Customer "顧客" { id: Int @pk  name: String }
entity Order "注文" { id: Int @pk  total: Money }
relate(Order, Customer, N:1)
"#;
    let (ast, _) = parse(src);
    let (model, _) = build_model(&ast);
    let result = ErMermaidEmitter.emit(&model, &View::er()).unwrap();
    insta::assert_snapshot!(result);
}

#[test]
fn a_child_side_minimum_draws_one_or_more_at_the_child_end() {
    let src = "entity Order \"O\" { id: Int @pk }\nentity Line \"L\" { id: Int @pk }\nentity Tag \"T\" { id: Int @pk }\nrelate(Line, Order, 1..*:1)\nrelate(Order, Tag, 1:1..*)\n";
    let (ast, _) = parse(src);
    let (model, _) = build_model(&ast);
    let mermaid = ErMermaidEmitter.emit(&model, &View::er()).unwrap();
    assert!(
        mermaid.contains("  Line }|--|| Order : \"\"\n"),
        "{mermaid}"
    );
    assert!(mermaid.contains("  Order ||--|{ Tag : \"\"\n"), "{mermaid}");
}

#[test]
fn test_state_mermaid_emit() {
    let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, published) @default(draft)
  kind: Enum(memo, report)
}
usecase Publish "公開する"
transitions(Publish, Doc.status, draft -> published)
"#;
    let model = model_from(src);
    let result = StateMermaidEmitter
        .emit(
            &model,
            &all_entities(&model).with_model_path("m.rdra"),
            &Explored::default(),
        )
        .unwrap();
    assert_eq!(
        result,
        "%% rdra-ish diagram=state scope=entity:Doc model=m.rdra\n\
         %% kinds: statemodel state usecase\n\
         %% omitted: Doc.kind (Enum without transitions)\n\
         %% legend: stateunreachable = no reachable state holds it, statedeadlock = an instance cannot leave it, stateend = no transition out\n\
         stateDiagram-v2\n  \
           state \"Doc.status\" as statemodel__Doc_status {\n    \
             [*] --> state__Doc_draft\n    \
             state \"draft\" as state__Doc_draft\n    \
             state \"published\" as state__Doc_published\n    \
             %% transition usecase__Publish\n    \
             state__Doc_draft --> state__Doc_published : 公開する\n    \
             state__Doc_published --> [*]\n  \
           }\n  \
           classDef stateend fill:#dfd,stroke:#383,color:#141\n  \
           class state__Doc_published stateend\n"
    );
}

#[test]
fn test_state_mermaid_draws_the_label_of_a_value() {
    let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft "下書き", published) @default(draft)
}
usecase Publish "公開する"
transitions(Publish, Doc.status, draft -> published)
"#;
    let model = model_from(src);
    let result = StateMermaidEmitter
        .emit(&model, &all_entities(&model), &Explored::default())
        .unwrap();
    assert!(
        result.contains("    state \"下書き\" as state__Doc_draft\n"),
        "{result}"
    );
    assert!(
        result.contains("    state \"published\" as state__Doc_published\n"),
        "{result}"
    );
}

#[test]
fn test_state_mermaid_names_the_conditions_that_guard_a_transition() {
    let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, published) @default(draft)
  reviewed: Bool @default(false)
}
usecase Publish "公開する"
transitions(Publish, Doc.status, draft -> published)
condition Reviewed "レビュー済み"
holds(Reviewed, Doc.reviewed == true)
applies_to(Reviewed, Publish)
condition Traced "追跡のみ"
applies_to(Traced, Publish)
"#;
    let model = model_from(src);
    let result = StateMermaidEmitter
        .emit(&model, &all_entities(&model), &Explored::default())
        .unwrap();
    assert!(
        result.contains(
            "    %% transition usecase__Publish\n    state__Doc_draft --> state__Doc_published : 公開する [レビュー済み]\n"
        ),
        "a condition without a body guards nothing: {result}"
    );
}

/// A use case that moves two columns is one step; each column's edge says so.
#[test]
fn test_state_mermaid_names_the_other_column_a_use_case_moves() {
    let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, published) @default(draft)
  period: Enum(open, closed) @default(open)
}
usecase Publish "公開する"
usecase Reopen "開き直す"
transitions(Publish, Doc.status, draft -> published)
transitions(Publish, Doc.period, open -> closed)
transitions(Reopen, Doc.period, closed -> open)
"#;
    let model = model_from(src);
    let result = StateMermaidEmitter
        .emit(&model, &all_entities(&model), &Explored::default())
        .unwrap();
    assert!(
        result.contains(
            "state__Doc_status_draft --> state__Doc_status_published : 公開する (Doc.period と同時)\n"
        ),
        "{result}"
    );
    assert!(
        result.contains(
            "state__Doc_period_open --> state__Doc_period_closed : 公開する (Doc.status と同時)\n"
        ),
        "{result}"
    );
    assert!(
        result.contains("state__Doc_period_closed --> state__Doc_period_open : 開き直す\n"),
        "{result}"
    );
}

/// An arm's own `.when(Cond)` joins the use case's guards on that arm only.
#[test]
fn test_state_mermaid_names_the_guard_of_one_arm() {
    let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, published, dropped) @default(draft)
  reviewed: Bool @default(false)
}
usecase Decide "判断する"
condition Reviewed "レビュー済み"
holds(Reviewed, Doc.reviewed == true)
transitions(Decide, Doc.status, draft -> published).when(Reviewed)
transitions(Decide, Doc.status, draft -> dropped)
"#;
    let model = model_from(src);
    let result = StateMermaidEmitter
        .emit(&model, &all_entities(&model), &Explored::default())
        .unwrap();
    assert!(
        result.contains("state__Doc_draft --> state__Doc_published : 判断する [レビュー済み]\n"),
        "{result}"
    );
    assert!(
        result.contains("state__Doc_draft --> state__Doc_dropped : 判断する\n"),
        "{result}"
    );
}

#[test]
fn test_state_mermaid_draws_one_composite_state_per_state_model() {
    let src = r#"
entity Order "Order" {
  id: Int @pk
  status: Enum(open, paid, closed) @default(open)
}
entity Ticket "Ticket" {
  id: Int @pk
  phase: Enum(new, done) @default(new)
}
usecase Pay "Pay"
usecase Close "Close"
usecase Reopen "Reopen"
usecase Finish "Finish"
transitions(Pay, Order.status, open -> paid)
transitions(Close, Order.status, paid -> closed)
transitions(Reopen, Order.status, closed -> open)
transitions(Finish, Ticket.phase, new -> done)
"#;
    let model = model_from(src);
    let result = StateMermaidEmitter
        .emit(&model, &all_entities(&model), &Explored::default())
        .unwrap();
    assert_eq!(result.matches("as statemodel__").count(), 2, "{result}");
    let order = result
        .split("state \"Order.status\" as statemodel__Order_status {\n")
        .nth(1)
        .and_then(|rest| rest.split("  }\n").next())
        .unwrap();
    assert!(order.contains(
        "%% transition usecase__Reopen\n    state__Order_closed --> state__Order_open : Reopen"
    ));
    assert!(!order.contains("Ticket"), "{order}");
    // Every Order state leaves, so none is an end state; Ticket ends at done.
    assert!(!order.contains("--> [*]"), "{result}");
    assert!(
        result.contains("    state__Ticket_done --> [*]\n"),
        "{result}"
    );
    assert!(!result.contains("state__Ticket_new --> [*]"), "{result}");
}

/// Two lifecycle columns of one entity are two state models, and a value both hold is a
/// state of each, with the label its own column gives it.
#[test]
fn test_state_mermaid_keeps_a_value_two_lifecycle_columns_share_apart() {
    let src = r#"
entity D "D" {
  id: Int @pk
  status: Enum(open, done) @default(open)
  review: Enum(none "未審査", open "審査中") @default(none)
}
usecase Close "Close"
usecase Review "Review"
transitions(Close, D.status, open -> done)
transitions(Review, D.review, none -> open)
"#;
    let model = model_from(src);
    let result = StateMermaidEmitter
        .emit(&model, &all_entities(&model), &Explored::default())
        .unwrap();
    let block = |column: &str| {
        result
            .split(&format!("as statemodel__D_{column} {{\n"))
            .nth(1)
            .and_then(|rest| rest.split("  }\n").next())
            .unwrap()
            .to_string()
    };
    let (status, review) = (block("status"), block("review"));
    assert!(
        status.contains("    [*] --> state__D_status_open\n"),
        "{result}"
    );
    assert!(
        status.contains("state \"open\" as state__D_status_open\n"),
        "{result}"
    );
    assert!(
        review.contains("    [*] --> state__D_review_none\n"),
        "{result}"
    );
    assert!(
        review.contains("state \"審査中\" as state__D_review_open\n"),
        "{result}"
    );
    assert!(!review.contains("state__D_status"), "{result}");
}

#[test]
fn test_state_mermaid_buc_scope_draws_the_state_models_its_use_cases_move() {
    let src = r#"
buc BucPay "Pay"
buc BucShip "Ship"
usecase Pay "Pay"
usecase Ship "Ship"
entity Order "Order" {
  id: Int @pk
  status: Enum(open, paid, shipped) @default(open)
}
entity Parcel "Parcel" {
  id: Int @pk
  status: Enum(packed, sent) @default(packed)
}
contains(BucPay, Pay)
contains(BucShip, Ship)
transitions(Pay, Order.status, open -> paid)
transitions(Ship, Order.status, paid -> shipped)
transitions(Ship, Parcel.status, packed -> sent)
"#;
    let model = model_from(src);
    let result = StateMermaidEmitter
        .emit(
            &model,
            &View::bucs(vec!["BucPay".to_string()]),
            &Explored::default(),
        )
        .unwrap();
    assert!(result.contains("scope=buc:BucPay"), "{result}");
    assert!(result.contains("statemodel__Order_status {"), "{result}");
    assert!(
        result.contains("state__Order_paid --> state__Order_shipped : Ship"),
        "{result}"
    );
    assert!(!result.contains("Parcel"), "{result}");
}

#[test]
fn test_state_start_is_the_creation_status_not_a_graph_source() {
    let src = r#"
entity Doc "文書" {
  id: Int @pk
  status: Enum(draft, review, published) @default(draft)
}
usecase Submit "提出"
usecase Publish "公開"
usecase Reopen "戻す"
creates(Submit, Doc)
sets(Submit, Doc.status == review)
transitions(Publish, Doc.status, review -> published)
transitions(Reopen, Doc.status, published -> review)
"#;
    let model = model_from(src);
    let result = StateMermaidEmitter
        .emit(&model, &all_entities(&model), &Explored::default())
        .unwrap();
    assert!(result.contains("[*] --> state__Doc_review"), "{result}");
    assert!(!result.contains("[*] --> state__Doc_draft"), "{result}");
}

#[test]
fn test_state_start_of_a_preexisting_entity_is_its_base() {
    let src = r#"
entity Acct "口座" {
  id: Int @pk
  status: Enum(active, frozen) @default(active)
}
usecase Freeze "凍結"
usecase Thaw "解除"
transitions(Freeze, Acct.status, active -> frozen)
transitions(Thaw, Acct.status, frozen -> active)
"#;
    let model = model_from(src);
    let result = StateMermaidEmitter
        .emit(&model, &all_entities(&model), &Explored::default())
        .unwrap();
    assert!(result.contains("[*] --> state__Acct_active"), "{result}");
    assert!(!result.contains("[*] --> state__Acct_frozen"), "{result}");
}

#[test]
fn test_rdra_mermaid_draws_a_condition_with_its_derived_links() {
    let src = r#"
variation Method "Method" {
  cash
  card
}
entity Pay "Pay" {
  id: Int @pk
  method: Method
  status: Enum(open, paid) @default(open)
}
usecase Collect "Collect"
transitions(Collect, Pay.status, open -> paid)
condition Insured "Insured"
holds(Insured, Pay.method == card, Pay.status == paid)
applies_to(Insured, Collect)
"#;
    let model = model_from(src);
    let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();
    assert!(
        result.contains("condition__Insured{\"Insured\"}:::condition"),
        "{result}"
    );
    assert!(
        result.contains("variation__Method[\\\"Method\"\\]:::variation"),
        "{result}"
    );
    assert!(
        result.contains(
            "  condition__Insured -.-> variation__Method\n  \
             %% derived condition__Insured -> variation__Method from holds\n"
        ),
        "{result}"
    );
    assert!(
        result.contains(
            "  condition__Insured -.-> entity__Pay\n  \
             %% derived condition__Insured -> entity__Pay from holds\n"
        ),
        "{result}"
    );
    assert!(
        result.contains("condition__Insured -.->|applies_to| usecase__Collect"),
        "{result}"
    );
}

#[test]
fn test_rdra_mermaid_draws_a_when_guard_from_the_condition_to_the_use_case() {
    let src = r#"
entity Pay "Pay" {
  id: Int @pk
  insured: Bool
  status: Enum(open, paid, waived) @default(open)
}
usecase Collect "Collect"
condition Insured "Insured"
holds(Insured, Pay.insured == true)
transitions(Collect, Pay.status, open -> waived).when(Insured)
transitions(Collect, Pay.status, open -> paid)
"#;
    let model = model_from(src);
    let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();
    assert!(
        result.contains(
            "  condition__Insured -.->|when| usecase__Collect\n  \
             %% derived condition__Insured -> usecase__Collect via transitions Pay.status open->waived\n"
        ),
        "{result}"
    );
    let usecase = UsecaseMermaidEmitter.emit(&model, &View::whole()).unwrap();
    assert!(
        usecase.contains("  condition__Insured -.->|when| usecase__Collect\n"),
        "{usecase}"
    );
    assert!(usecase.contains("condition__Insured{\"Insured\"}:::condition"));
}

#[test]
fn test_rdra_mermaid_draws_a_declared_combines_without_derived() {
    let src = r#"
variation Method "Method" {
  cash
  card
}
condition ByMethod "By method"
combines(ByMethod, Method)
"#;
    let model = model_from(src);
    let result = RdraMermaidEmitter.emit(&model, &View::whole()).unwrap();
    assert!(
        result.contains("condition__ByMethod -.-> variation__Method"),
        "{result}"
    );
    assert!(!result.contains("%% derived"), "{result}");
}

#[test]
fn test_event_mermaid_renders_timers_and_extsystems() {
    let src = r#"
usecase Book "Book"
usecase Receive "Receive"
usecase Sweep "Sweep"
extsystem Gateway "Gateway"
extsystem Lab "Lab"
timer Nightly "Nightly"
event EvBooked "Booked"
event EvResult "Result"
raises(Book, EvBooked)
triggers(EvBooked, Gateway)
raises(Lab, EvResult)
triggers(EvResult, Receive)
triggers(Nightly, Sweep)
"#;
    let model = model_from(src);
    let result = EventMermaidEmitter
        .emit(&model, &View::whole().with_model_path("m.rdra"))
        .unwrap();
    assert!(
        result.starts_with(
            "%% rdra-ish diagram=event scope=whole model=m.rdra\n\
             %% kinds: event usecase extsystem timer\n\
             %% omitted: (none)\n\
             flowchart LR\n"
        ),
        "{result}"
    );
    assert!(
        result.contains("  extsystem__Lab -.-> event__EvResult\n"),
        "{result}"
    );
    // An event delivered to an external system is the DSL's `triggers`.
    assert!(
        result.contains("  event__EvBooked -.-> extsystem__Gateway\n"),
        "{result}"
    );
    assert!(
        result.contains("  timer__Nightly -.-> usecase__Sweep\n"),
        "{result}"
    );
    assert!(result.contains("  timer__Nightly((\"Nightly\")):::timer\n"));
    assert!(
        result.ends_with("  classDef timer fill:#ffe8cc,stroke:#d9480f,color:#000;\n"),
        "{result}"
    );
}

#[test]
fn test_event_mermaid_renders_triggered_buc() {
    let src = r#"
buc BucBillingClaims "Billing Claims"
usecase SignEncounter "Sign Encounter"
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
triggers(EncounterSigned, BucBillingClaims)
"#;
    let model = model_from(src);
    let result = EventMermaidEmitter.emit(&model, &View::whole()).unwrap();

    assert!(
        result.contains("event__EncounterSigned -.-> buc__BucBillingClaims"),
        "{result}"
    );
    assert!(result.contains("  buc__BucBillingClaims[\"Billing Claims\"]:::buc\n"));
}

#[test]
fn test_event_mermaid_declares_each_node_once_and_draws_no_states() {
    let src = r#"
usecase SignEncounter "Sign Encounter"
usecase ReviewClaim "Review Claim"
buc BucBillingClaims "Billing Claims"
entity Encounter "Encounter" {
  id: Int @pk
  status: Enum(pending, signed) @default(pending)
}
entity Claim "Claim" {
  id: Int @pk
  status: Enum(open, reviewed) @default(open)
}
event EncounterSigned "Encounter Signed" description "The doctor signed the record"
raises(SignEncounter, EncounterSigned)
triggers(EncounterSigned, ReviewClaim)
triggers(EncounterSigned, BucBillingClaims)
transitions(SignEncounter, Encounter.status, pending -> signed)
transitions(ReviewClaim, Claim.status, open -> reviewed)
"#;
    let model = model_from(src);
    let flow = rdra_ish_core::collect_event_flows(&model).pop().unwrap();
    let mut out = String::new();
    let mut declared = Declared::default();

    render_event_flow(&mut out, &mut declared, &model, &None, &flow);
    render_event_flow(&mut out, &mut declared, &model, &None, &flow);

    assert_eq!(
        out.matches("  event__EncounterSigned>\"Encounter Signed\"]:::event\n")
            .count(),
        1,
        "{out}"
    );
    assert!(out.contains("  %% desc event__EncounterSigned: The doctor signed the record\n"));
    assert!(out.contains("usecase__SignEncounter -.-> event__EncounterSigned"));
    assert!(out.contains("event__EncounterSigned -.-> usecase__ReviewClaim"));
    assert!(out.contains("event__EncounterSigned -.-> buc__BucBillingClaims"));
    assert!(!out.contains("state"), "{out}");
    assert_eq!(declared.kinds, vec!["event", "usecase", "buc"]);

    let result = EventMermaidEmitter.emit(&model, &View::whole()).unwrap();
    assert!(
        result.contains("%% omitted: state transitions\n"),
        "{result}"
    );
}

#[test]
fn test_event_mermaid_ids_follow_the_contract() {
    let src = r#"
usecase SignEncounter "Sign Encounter"
buc BucBillingClaims "Billing Claims"
extsystem Lab "Lab"
timer Nightly "Nightly"
event EncounterSigned "Encounter Signed"
"#;
    let model = model_from(src);
    let key = |id: &str| model.symbols.get(id).cloned().unwrap();
    let mut out = String::new();
    let mut declared = Declared::default();
    for (node, id) in [
        (key("EncounterSigned"), "event__EncounterSigned"),
        (key("SignEncounter"), "usecase__SignEncounter"),
        (key("BucBillingClaims"), "buc__BucBillingClaims"),
        (key("Lab"), "extsystem__Lab"),
        (key("Nightly"), "timer__Nightly"),
    ] {
        let flow_node = match node {
            NodeRef::Event(k) => FlowNode::Event(k),
            NodeRef::UseCase(k) => FlowNode::UseCase(k),
            NodeRef::Buc(k) => FlowNode::Buc(k),
            NodeRef::ExtSystem(k) => FlowNode::ExtSystem(k),
            NodeRef::Timer(k) => FlowNode::Timer(k),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            declare_flow_node(&mut out, &mut declared, &model, flow_node).as_deref(),
            Some(id)
        );
        assert_eq!(contract_id(&model, &node).as_deref(), Some(id));
    }
}

#[test]
fn test_event_mermaid_visibility_skips_hidden_event_scope() {
    let src = r#"
usecase SignEncounter "Sign Encounter"
event EncounterSigned "Encounter Signed"
raises(SignEncounter, EncounterSigned)
"#;
    let model = model_from(src);
    let flow = rdra_ish_core::collect_event_flows(&model).pop().unwrap();
    let mut out = String::new();
    let mut declared = Declared::default();
    let reachable = Some(HashSet::new());

    render_event_flow(&mut out, &mut declared, &model, &reachable, &flow);

    assert!(out.is_empty());
    assert!(declared.ids.is_empty());
}

#[test]
fn test_er_mermaid_buc_filter() {
    let src = r#"
buc BucA "業務A"
usecase UcA "ユースケースA"
entity EntityA "エンティティA" { id: Int @pk }
entity EntityB "エンティティB" { id: Int @pk }
contains(BucA, UcA)
reads(UcA, EntityA)
"#;
    let model = model_from(src);
    let view = View {
        state: Default::default(),
        scope: crate::Scope::Bucs(vec!["BucA".to_string()]),
        filter: crate::Filter::Er,
        model_path: None,
        node_kinds: Vec::new(),
        edge_kinds: Vec::new(),
    };
    let result = ErMermaidEmitter.emit(&model, &view).unwrap();
    assert!(result.contains("EntityA"), "EntityA should be included");
    assert!(!result.contains("EntityB"), "EntityB should be excluded");
}

const ANOMALIES: &str = r#"
entity Order "注文" {
  id: Int @pk
  status: Enum(pending, paid, shipped, lost) @default(pending)
}
usecase Place "注文確定"
usecase Pay "支払い"
usecase Ship "発送"
usecase Express "即日発送"
buc BucOrder "Order"
contains(BucOrder, Place)
contains(BucOrder, Pay)
contains(BucOrder, Ship)
contains(BucOrder, Express)
creates(Place, Order)
updates(Pay, Order)
updates(Ship, Order)
updates(Express, Order)
transitions(Pay, Order.status, pending -> paid)
transitions(Ship, Order.status, paid -> shipped)
transitions(Express, Order.status, pending -> shipped)
condition Packed "梱包済み"
holds(Packed, Order.status == pending)
applies_to(Packed, Ship)
"#;

fn anomaly_view(entities: &[&str]) -> (View, Explored) {
    let model = model_from(ANOMALIES);
    let report = rdra_ish_core::lifecycle::derive_states(
        &model,
        &rdra_ish_core::lifecycle::Scope::ALL,
        rdra_ish_core::lifecycle::Caps::STATES,
    );
    let mut view = View::whole();
    view.state.entities = entities.iter().map(|e| e.to_string()).collect();
    (view, Explored::from_report(&report))
}

#[test]
fn test_state_full_model_colours_unreachable_deadlock_and_end() {
    let model = model_from(ANOMALIES);
    let (view, explored) = anomaly_view(&["Order"]);
    let out = StateMermaidEmitter.emit(&model, &view, &explored).unwrap();
    assert!(out.contains("%% legend:"), "{out}");
    assert!(
        out.contains("class state__Order_lost stateunreachable"),
        "{out}"
    );
    assert!(
        out.contains("class state__Order_paid statedeadlock"),
        "{out}"
    );
    assert!(out.contains("class state__Order_shipped stateend"), "{out}");
    assert!(!out.contains("state__Order_lost --> [*]"), "{out}");
    for name in ["stateunreachable", "statedeadlock", "stateend"] {
        assert!(out.contains(&format!("classDef {name} ")), "{out}");
    }
    assert!(out.contains("scope=entity:Order"), "{out}");
}

#[test]
fn test_state_whole_model_is_an_index() {
    let model = model_from(ANOMALIES);
    let (view, explored) = anomaly_view(&[]);
    let out = StateMermaidEmitter.emit(&model, &view, &explored).unwrap();
    assert!(out.contains("4 状態 / 3 遷移 (--entity Order)"), "{out}");
    assert!(!out.contains("-->"), "{out}");
    assert!(!out.contains("class "), "{out}");
}
