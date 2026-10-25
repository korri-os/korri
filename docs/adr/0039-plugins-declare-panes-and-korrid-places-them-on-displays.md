---
status: accepted
---

# Plugins declare panes and korrid places them on displays

**Owner decision, 2026-10-25.** A device can have any number of displays in
any arrangement. A game can have more than one pane (a Nintendo DS game has
an upper and a lower pane). The plugin declares its panes and how its runner
can show them. The device declares its displays. korrid chooses the
placement and carries it out. Plugins never see display facts, and korrid
holds no emulator knowledge.

## Decision

- A system declares how many panes its content has and how they relate,
  such as the upper pane above the lower pane.
- A runner declares the ways it can show those panes: one window with named
  layouts and where each pane sits in each layout, or one window per pane.
- The device declares its displays, their arrangement, its main display and
  which touch input belongs to which display, as hardware facts. A person
  arranges displays they connect later.
- korrid picks the layout that puts each pane on its own display when the
  arrangement matches the panes' relation, and otherwise the best fit on the
  main display. It passes the choice to the plugin's launch, which maps it to
  the runner's own setting. korrid then places the windows, maps touch to the
  display it belongs to, and turns off displays that show nothing.
- A display that shows nothing is off. It turns on only while Korri places
  something on it, such as a pane, the gameplay overlay or a surface.
- The gameplay overlay appears on a display that shows no pane, or over the
  game on the main display when every display shows a pane.
- A surface sees one display, the main display, unless it opts in. An opted-in
  surface receives every display with its arrangement and owns all of their
  pixels.
- A display belongs to exactly one device. The model allows panes on another
  device's display (a TV plus a handheld); building that waits for its own
  design on top of stream play.

## Considered options

- **korrid decides alone.** The smallest plugin contract, but korrid would need
  runner knowledge: which core option produces which window layout.
- **The plugin decides alone.** The most flexible, but every plugin would repeat
  the same placement logic, behaviour would differ between emulators, and
  plugins would see device facts.

## Consequences

- The plugin contract gains two declaration parts: a system's panes and a
  runner's layouts. Their schema is extracted from the first real producer,
  the melonDS runner (`melonds_screen_layout`: Top/Bottom, Left/Right, Top
  Only, Bottom Only), not designed ahead of it.
- The launch input that today carries the compositor's output size
  (`PluginDisplay`, ADR 0007 `launch.compose`) carries the chosen layout.
- The surface treaty gains an opt-in for several displays. Pico opts in first,
  with the main display for showing (selected game, art, details) and the other
  display for choosing and acting (list, menu, buttons, touch).
- The first device case is the RG DS: two 640×480 touch displays, one above the
  other, with the top display as the main display.
- Streaming a multi-pane game must feel native on the receiving device: each
  pane on its matching display, touch reaching the lower pane (owner
  requirement, using a 3DS game as the bar). Stream play carries one picture
  today; this is tracked as issue #262.
