# Step 2: Screens

<!-- derived-from ../../../docs/incremental-modeling.md#stage-2-screens -->

Add the screens each use case works through, and the moves between them, before the data.
In RDRA the screen sits with the use case on the system boundary, nearest the actor.

## Ask

<!-- stage-spec: asks screens -->
- Which screen does the actor work on in each use case?
- Which screen leads to which, and through which use case?
- Which items on a screen does the actor enter or check, and which does the system fill in?
- Is any use case started by an event or timer, with no actor at a screen?
<!-- /stage-spec -->

## Do

1. Add a `screen` per surface the actor works on, named for what the actor does there.
2. Add `displays(UseCase, Screen)`. A use case an event or timer starts needs no screen.
3. Add `navigates(From, To).via(UseCase)` for each move the UI offers.
4. Add `field` and `contains(Screen, Field)` only for items that need review. Leave
   `maps_field` for step 3, once the entity exists; `lint` `field-unmapped` is expected now.
5. Keep entities, CRUD, APIs, and permissions out.

```rdra
screen CartScreen "Cart"
screen CheckoutScreen "Checkout"

displays(PlaceOrder, CheckoutScreen)
displays(CancelOrder, CartScreen)
navigates(CartScreen, CheckoutScreen).via(PlaceOrder)
```

## Validate

<!-- stage-spec: review screens -->
```sh
rdra-ish check src/
rdra-ish diagram src/ --kind rdra --format mermaid --buc BucOrder
rdra-ish diagram src/ --kind screen --format mermaid --buc BucOrder
rdra-ish list src/ --kind screen --format table
```
<!-- /stage-spec -->

## Done when

- Each use case an actor starts names its screen; each one without a screen is started by
  an event or timer.
- The screen diagram reads as the actor's path through the BUC.
- No entity, CRUD, or API was guessed.

Next: `references/03-data-touchpoints.md`, once reviewers can name the objects each use
case touches.
