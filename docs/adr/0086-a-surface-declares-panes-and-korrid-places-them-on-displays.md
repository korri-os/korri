---
status: accepted
---

# A surface declares panes and korrid places them on displays

**Owner decisions, 2026-11-18.** This is the per-display surface design that
the 2026-10-27 amendment to ADR 0039 left for later. It applies ADR 0039's
model for game panes to surfaces.

## Decision

- A surface divides what it draws into panes and declares how they relate.
  korrid places the panes on the device's displays, as it places a game's
  panes. A surface never receives display facts, just as it never receives
  key codes.
- One window can hold several panes. When the panes share a display, the
  surface draws them together under its own layout plan. Its layout remains a
  pure function of each window's size.
- A surface has one focus across all its panes. The d-pad crosses from one
  display to another through region exits (surface contract 8.4). Touch works
  on every display that has it.
- When displays change while a surface is open, such as a display that turns
  on or off, the surface reflows at once and keeps focus on the same item.
- `SurfacePackageDisplays` (`main` / `all`) and `SurfaceModel.displays` are
  removed in the same change that adds panes. No compatibility path is kept.
  Pico, the only user, is parked. It keeps its single-display layout and loses
  its two-display split until someone ports it.
- Velvet has two panes. The **Stage** holds the lit game, the detail and
  Korri's status. The **Floor** holds the row, the lobby, sets, list pages,
  sheets and the keyboard. On one display, the Floor stands under the Stage,
  as it does today.
- The first device is the AYN Thor, with its top and bottom displays. The RG
  DS is permanently offline.

## Considered options

- **The surface receives display facts for each window and places its panes
  itself.** This was Pico's model. The treaty is smaller, but every surface
  repeats the placement logic and depends on display facts.
- **The host gives one window per pane, even on one display.** This takes
  the layout away from the surface and breaks the rule that the surface owns
  every pixel.
- **One focus for each display.** This adds a mode and a switch button that a
  person with one display never needs.

## Consequences

- The treaty needs a way for a surface to declare its panes and their
  relation, and a way to tell each window which panes it holds and how large
  it is. No schema is decided here. Take it from Velvet's real split and from
  the shell's existing two-window mechanism (Surface and Overlay).
- The shell must run one engine with any number of windows. korrid's display
  placement must put surface windows on more than the main display, and must
  power on the displays they use.
- Panes must stay usable for a later case in which a pane is on another
  device's display, such as a TV and a handheld. Each pane draws only from the
  surface state and the model. Focus belongs to the state, not to a window. No
  pane may assume that another pane is visible.
- Placing the Thor's bottom display requires the device image to light it and
  to map its touch input. The first Thor image turns that display off.
