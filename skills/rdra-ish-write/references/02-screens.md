# Step 2: Screens

Use this context when actors and use cases are stable enough to ask what the actor sees
and does in each use case.

## Goal

Add the screens each use case works through, and the transitions between them, before
deciding which data each use case touches. In RDRA the screen sits with the use case on
the system boundary, nearest the actor, so it is settled before the information inside
the system.

## Ask For

<!-- stage-spec: asks screens -->
- Which screen does the actor work on in each use case?
- Which screen leads to which, and through which use case?
- Which items on a screen does the actor enter or check, and which does the system fill in?
- Is any use case started by an event or timer, with no actor at a screen?
<!-- /stage-spec -->

## Procedure

1. Add a `screen` for each surface an actor works on. Name it for what the actor does
   there, not for a UI component.
2. Connect each use case to its screen with `displays(UseCase, Screen)`. A use case
   that an event or timer starts needs no screen: model it with `triggers(Event, UseCase)`
   or `triggers(Timer, UseCase)` instead.
3. Add `navigates(FromScreen, ToScreen).via(UseCase)` for each move between screens the
   UI offers, naming the use case whose operation causes it. `diagram --kind screen`
   draws them, and fills the gaps with dashed `業務上の順番` edges from the flows.
4. Add `field` and `contains(Screen, Field)` only when a screen item needs review as a
   model element. Leave `maps_field` for Step 3, once the entity exists.
5. Keep entities, CRUD, `shows`, APIs, and permissions out of this step.

## Minimal Pattern

```rdra
screen CartScreen "Cart"
screen CheckoutScreen "Checkout"

displays(PlaceOrder, CheckoutScreen)
displays(CancelOrder, CartScreen)
navigates(CartScreen, CheckoutScreen).via(PlaceOrder)
```

## Validation

<!-- stage-spec: review screens -->
```sh
rdra-ish check src/
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
rdra-ish diagram src/ --kind screen --format mermaid --buc BucOrder
rdra-ish list src/ --kind screen --format table
```
<!-- /stage-spec -->

Also run:

```sh
rdra-ish lint src/ --format table
```

`lint` reports `field-unmapped` for a field with no `maps_field` yet; at this step that
is expected.

## Achievement Conditions

- Every use case an actor starts names the screen it works on.
- Every use case with no screen is started by an event or timer.
- The screen diagram reads as the actor's path through the BUC.
- No entity, CRUD, or API was guessed to make the screens look complete.

## Next Step

Load `references/03-data-touchpoints.md` when reviewers can name the business objects
each use case touches and each screen shows.
