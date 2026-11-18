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
- korrid places a surface's panes by ADR 0039's rule. The first pane goes on
  the main display. The panes split across displays only when the display
  arrangement matches how they relate; otherwise they all go on the main
  display. On the Thor, the Stage is on the top display and the Floor is on
  the bottom display. With two displays side by side, both Velvet panes stay
  on the main display.
- On the Thor, the AYN key turns the bottom display on and off. The Floor
  moves between the bottom and the top display at once.
- When the Floor holds a page with no game, such as Settings, the Stage shows
  that page's own side content if the page has some (for example the backup
  QR code). Otherwise the Stage shows the last lit game under a veil.
- When a game's detail opens, focus moves to Play on the Stage. The Floor
  keeps its row or set visible, without focus. Back returns focus to the same
  tile.
- The stage light reaches the Floor's display too, but dimmer, so the two
  displays read as one room.
- During play, ADR 0039 applies for now: the bottom display is off, and the
  gameplay overlay appears there after Home. The owner's direction is a
  companion on the bottom display during play. That is not designed yet. The
  work under this ADR must keep a clean path to it (see Consequences).

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
  to map its touch input. The first Thor image turns that display off. That
  device work is its own issue, and it lands before korrid's placement work.
- The clean path to a companion during play: "off during play" is korrid's
  placement policy and nothing else. The shell and the surface do not assume
  it. A companion would be another surface view that korrid places on a free
  display while a game runs, as it already places the gameplay overlay. Still
  open: where touch on the companion's display goes while the game holds the
  controller, and how the companion respects ADR 0053, which says the overlay
  never freezes the game.
