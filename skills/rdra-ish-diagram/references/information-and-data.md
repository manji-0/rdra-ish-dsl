# Information, conditions, and data

Use these views for what the use cases act on. Read `information` before `er`: the
relations and state models first, then the columns.

```sh
rdra-ish diagram src/ --kind information --format mermaid
rdra-ish diagram src/ --kind condition --buc BucOrder --format mermaid
rdra-ish diagram src/ --kind er --entity Order --format mermaid
```

`--entity` on `information` and `er` draws the named entities and the ones a `relate`
joins them to.

## Information model (`information`)

- Nodes are `entity__<Id>`, `statemodel__<Entity>_<column>`, and `variation__<Id>`.
  `omitted:` starts with `columns`, which `er` draws.
- `entity__<Child> ---|N:1| entity__<Parent>` is a `relate` with its cardinality.
- An unlabelled `-.->` links an entity to each column `transitions` drive. An `Enum`
  without `transitions` is no state model.
- `-.->|variation <column>|` links an entity to the variation that column takes.

## Conditions and variations (`condition`)

- A condition links by an unlabelled `-.->` (`combines`) to each variation and state
  model it combines, and to the APIs and systems it applies to.
- To a use case the edge is labelled: `-.->|applies_to|`, or `-.->|when|` when the
  condition guards a `transitions` arm or a `chooses` value of the use case, followed by
  `%% derived ... via transitions <Entity>.<column> <from>-><to>` (or `via chooses ...`).
- A condition with a `holds` body has a `%% holds <id>: <body>` line, and each link read
  from the body is followed by `%% derived <from> -> <to> from holds`. A link without it is
  a declared `combines`.
- Each variation is `subgraph variation__<id>` with one `:::value` node per value. The
  whole model draws every variation, used or not.

## Logical data model (`er`)

- The body is an `erDiagram`; its entity names are DSL ids, not `entity__` ids.
- Each entity lists its columns with `PK` / `FK`. A variation or state model shows only as
  a column type.
- Crow's foot ends come from `relate`. A key with a minimum of 1 (`1..*:1`) draws its child
  end as one-or-more.
- `relate` generates the FK columns, one per key column of a composite parent `@pk`. Do
  not add one by hand; adopt your own with `.key(...)`.
- Read exact column metadata from `rdra-ish list src/ --kind entity --format json`, and
  API ownership from `rdra-ish csv src/ --kind api-matrix`.
