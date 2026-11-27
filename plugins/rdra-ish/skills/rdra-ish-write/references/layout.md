# Layout

<!-- derived-from ../../../docs/incremental-modeling.md#model-directory-layout -->

Where each file and declaration goes, and who performs a use case.

## Files

Start small. Split a file only when reviewability or ownership demands it.

```text
src/
  shared/
    actors.rdra      # module shared.actors
    biz.rdra         # module shared.biz
    entities.rdra    # module shared.entities
  buc/
    buc_<name>.rdra  # module buc.<name>
```

A larger model splits `shared/` by responsibility: `shared/entities/order.rdra`
(`module shared.entities.order`), `shared/lifecycle/order.rdra`, `shared/rules.rdra`.

- Start every file with `module <dotted.name>` mirroring its path.
- An import names a module, not a file: `import buc.order.{OrderScreen}` finds the file
  whose `module` line says `buc.order`, so a BUC can `navigates` to another BUC's screen.
  Forms: `import shared.actors`, `import shared.actors as a`,
  `import shared.actors.{Staff}`, `import shared.actors.{Staff as S}`.
- Import a declaration another file has; never redeclare it.
- Keep broad imports while exploring; narrow them after shared files split.

## Where each declaration goes

| `shared/` | `buc/buc_<name>.rdra` |
|---|---|
| `actor`, `extsystem`, `business`, `medium`, `permission`, `system`, `timer` reused across BUCs | `buc`, `usecase`, `screen`, `field`, `flow`, `step` |
| Stable `requirement`, `goal`, `nfr`, `quality`, `constraint`, `adr` | BUC-local `api`, `dto`, `event` |
| `entity`, `variation`, `relate` | CRUD, `displays`, `shows`, `navigates`, `maps_field`, `invokes`, `request` / `response` |
| Cross-BUC `event`; `transitions` on a shared entity, near the entity | `raises`, `triggers`, `sets`, `coordinates`, `has_permission`, `requires_*` |
| Rules over entities and variations only (`shared/rules.rdra`) | A `condition` or `property` that names a use case (`applies_to`, `.fair(UC)`), with its `holds` body |

A condition that names a use case stays in that use case's BUC file: in
`shared/rules.rdra` it would import the BUC module, which imports `shared.rules`, a
circular import `check` warns about.

## Who performs a use case

A use case's actors are its direct performers plus the performers of every BUC that
contains it, or whose flow has a step that `covers` it. `check` requires each of them to
hold the use case's permissions. So write `performs(Actor, Buc)` only for an actor who runs
every use case of the BUC; for an actor who runs some, write `performs(Actor, UseCase)` on
those. A use case a timer or event starts, with no direct performer, is the system's work
and needs no actor's permission.
