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
  target back into sight. Catalog paging is still driven by focus, so a long
  fling stops at the end of the loaded catalog window until a follow-up teaches
  paging about scrolling.
- While the person uses touch, the package hides its focus highlight. The
  highlight comes back on the next button press. Logic can tell `Activated`
  from `Pressed`, so this adds nothing to the treaty.
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
- While the finger moves, the view shows the value at once. Logic sends it
  through the existing hold-and-wait path that controller steps use (Pico's
  `SteppedRange`, `Waited`, `LeftRange`; 180 ms after the last change), so the
  plugin is not flooded.
- `Activated` also gets an optional `released` flag. The view sets it on the
  message it sends when the finger lifts. Logic then sends a held range value
  at once instead of waiting 180 ms. The full variant is
  `Activated { target, at?, released? }`.
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

Code waits for the overlay contract work (#377, #378, #379) to land, because
both changes touch `contracts/surface/` and the overlay views.

## Follow-ups

- Catalog paging learns about scrolling, so a long fling loads more games.
- Linux touch handhelds get touch through korrid's input path.
