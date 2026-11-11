---
status: accepted
---

# A tap activates the target the surface drew there

**Owner decisions, 2026-11-09 (issue #394).** On the Fold7, taps did nothing
anywhere in Pico or Zone, so PLAY and the gameplay overlay's rows did not
respond to a tap. Surfaces receive semantic buttons only, and they own focus
(native surfaces contract, sections 2 and 3.2). A one-finger drag already
moves the surface's cursor (`clients/shell/src/input/drag.rs`), and taps were
ignored. Both packages already name every focus target with a string id. Pico's
targets carry an `act`, which is what Confirm runs. Zone's Confirm handler
(`zone/src/press.ts`) acts on the focused target id.

## Decision

- Every Pico and Zone screen gets touch in one slice. The mechanism is the same
  for every screen, and a shell where only some screens take a tap feels broken.
- A tap on a target moves focus there and activates it, as if the person had
  moved there and pressed Confirm. It reuses what Confirm already does on that
  target (Pico's `act`, Zone's Confirm handler).
- Exception, owner decision after the first Fold7 test (2026-11-10): on a
  game in a list (library rows, the shelf, the grid, Find's and the store's
  results), the first tap only moves focus there, and a tap on the focused
  game opens it. Buttons, menu rows, settings rows and keyboard keys still act
  on one tap.
- The package's Slint view decides which target a tap hit. Focusable atoms
  report their focus-target id through one generic root callback, a convention
  like `reveal`. The shell carries that id to logic as a new `SurfaceMessage`.
  Logic sets focus and does what Confirm does there. The shell gets no focus model,
  does no hit-testing, and simulates no buttons. The message names the target
  by its existing focus-target id, and the treaty gains one variant.
- Touch displays use the same views. Tap accuracy is measured on the Fold7 first.
  A package may read the existing `SurfaceDisplay.touch` later if targets prove
  too small.
- The variant is `Activated`. The name says what it means, not the gesture, so
  a mouse click or a later accessibility action can send it too.
- A drag scrolls content directly, so the content follows the finger. It
  replaces the cursor-step drag. The Slint view owns the scroll offset, as it
  already does for `reveal` (`PicoScroll`). The shell passes pointer events to
  the Slint window and no longer turns drags into cursor steps. Scrolling adds
  nothing to the treaty.
- Scrolling does not move focus. The next button press scrolls the focused
  target back into sight.
- Rows that draw only the items near focus (Pico's shelf, RESUME, Find's and
  the store's results; Zone's card band and save band) cannot draw past the
  near items by themselves. The treaty has a second variant,
  `Dragged { region, by }`. `region` is the package's own focus-region name
  (contract section 8.4), and `by` is the signed number of whole items moved
  since the last `Dragged`. The view keeps the row under the finger between
  whole items and runs a fling's momentum itself.
- Owner decision after the first Fold7 test (2026-11-10), replacing the
  2026-11-09 rule that a drag moved focus: scrolling never moves focus, on
  these rows too. Logic keeps a separate scroll position per row. `Dragged`
  moves that position by `by`, clamped at the row's ends. Logic draws the
  items around the scroll position and pages the catalog from it, as it pages
  from focus today. Focus stays on the selected game, even when it is out of
  sight. The next button press brings the row back to the focused game.
- While the person uses touch, the package hides its focus highlight. The
  highlight comes back on the next button press, and that press brings the
  focused target back into sight even when it does not move focus.
- Owner decision during review (2026-11-09): a plain list scroll sends
  nothing to logic, so logic could not know touch was in use. The treaty
  gains a third variant, `Touched`, with no fields. The shell sends it once
  per gesture when a finger goes down anywhere, before any `Activated` or
  `Dragged` from that gesture. It says only that touch is in use. It names
  no target and does not wake an idle surface by itself; waking stays with
  the gesture's `Activated` or `Dragged`, or the attract cover.
- Each package makes its hint chips tappable. A tapped hint is a target whose
  `act` runs the package's own handler for that button, so a touch-only person
  can reach Options and Menu. Android's back gesture already arrives as Back.
- A tap or drag on a range control sets the value at the finger. A drag sends
  `Activated` again each time the finger moves along the track. `Activated`
  gets an optional `at`, from 0 to 1 along the control's track. The view fills
  it only for range targets. Logic maps it to the range's minimum and maximum
  and snaps it to a step. The message holds no pixels.
- On a range control inside a scrolling list, the drag locks to the first axis
  the finger moves along. Sideways adjusts the control, and up or down scrolls
  the list.
- While the finger moves, the view shows the value at once. Logic sends a
  value to the plugin only when the value, snapped to a step, changes. That is
  how each controller step already sends, so a drag sends at most one call per
  step it crosses.
- `Activated` also gets an optional `released` flag. The view sets it on the
  message it sends when the finger lifts. Logic then sends the final value if
  it differs from the last one sent. The full variant is
  `Activated { target, at?, released? }`.
- Correction (2026-11-09, during the build): an earlier text of this record
  said a held value waits 180 ms on an existing hold path. No package has a
  running hold. Pico's adapter answers its wait at once, and Zone sends every
  step at once. The step rule above replaces that text. The owner's choice
  stands: the game reacts while the finger slides, and the plugin is not
  flooded.
- Every tap and drag counts as activity: it resets `Tick.idle_ms` as `Pressed`
  does.
- While the screen is idle or in attract mode, the first tap only wakes it and
  activates nothing.
- Disabled controls are not targets (the packages leave them out of
  `targets`), so a tap on one does nothing.
- The treaty change is the same on every platform. Android is wired first.
  Linux touch handhelds (Odin 2 Portal, RP Mini V2, RG DS) follow through
  korrid's input path in a later slice.
- In the gameplay overlay, a tap outside the panel does nothing. The game
  never receives touch while the overlay is open (ADR 0053, #376).

## Consequences

`main.slint`'s rule "no callbacks" changes to "one generic activation callback,
no per-control callbacks". The focus probe (contract section 8.3) must also
show that each target can be tapped at each swept size.

Because a Slint `TouchArea` also takes mouse clicks, the desktop shell gains
mouse activation through the same path.

Touch depends on no other open work. #378 and #379 also change
`contracts/surface/` and the overlay views, so they must not run at the same
time as touch. Whichever starts second rebases onto the first.

## Follow-ups

- Catalog paging learns about scrolling in the lists that keep focus still
  (the library rows above already page through focus).
- Linux touch handhelds get touch through korrid's input path.
