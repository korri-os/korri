---
status: accepted
---

# The management surface opens as a drawer beside any surface

**Owner decisions, 2026-11-23.** ADR 0096 left open how a person reaches a
capability that the surface in front does not draw. The owner first imagined
the surface shrinking to a small picture, then chose a **drawer**
(`CONTEXT.md`): the management surface comes in from a screen edge, and the
person never feels moved to another place. The scout found that a surface
cannot be drawn inside another and cannot be captured to an image, which the
small-picture form needed. Prior art agreed with the drawer: the Xbox guide,
the Steam Deck quick access menu, the PlayStation 5 control center and the
Switch quick settings are all panels from an edge.

This changes ADR 0096 in one place: the management surface has no name yet.
"Bonfire" is now the name of a look for a surface that faces the person who
plays (`docs/design/surfaces/bonfire/`), and no longer names the management
surface.

## Decision

- The management surface is one design with two stops. **Part open**, it is a
  narrow column. **Fully open**, it fills the display. Both come from the same
  layout plan, as a function of the space given. A web screen (ADR 0096) shows
  it fully open, with nothing beside it.
- Part open is flat: one screen with no pages under it. It shows the one thing
  the person came for, or a short list of what matters now. Anything deeper
  pulls the drawer fully open.
- While the drawer is open, the surface behind cannot be used and is dimmed
  heavily. It is never resized and it does not move: the drawer covers it.
  The owner tried both forms on a mockup and chose this one.
- All input goes to the drawer while it is open. Back goes up one level and
  then closes it. Closing keeps what the person typed. When it closes, the
  surface behind has its focus where the person left it.
- One host action opens the drawer, and no surface can block it. It replaces
  the L1 + R1 surface cycle, which was a testing aid and never a product
  decision. Choosing a surface is a view inside the drawer. 
- Guide opens the drawer. A tap of Guide stays the surface's own, as today.
  Holding Guide always opens the drawer. A surface that has no menu of its
  own lets the tap open the drawer too. F2 does the same on a keyboard. The
  hold is the way out that no surface can block, so first-time setup teaches
  it.
- The drawer is a strip of two columns. Part open shows one of them; fully
  open shows both. Neither column changes between the stops, so the move from
  one stop to the other swaps no content and the focus stays where it is.
- The drawer has few words. A row is a name and its present value. A value
  is drawn as a mark where a mark says it faster. No sentence stands where a
  row would do.
- A surface may ask the host to open the drawer at one capability. This is a
  new request in the treaty.
- A surface may draw every capability itself and need the drawer for nothing.
  A surface may also leave any capability to the drawer. The completeness
  rule in `docs/design/surfaces/CAPABILITIES.md` becomes: every capability
  has an answer in the surface, or the drawer has it.
- A person learns of the drawer from the surface's own way in, drawn in the
  surface's own manner, and once during first-time setup. The host draws no
  mark over a surface.
- The drawer does not open over a running game at first. A game keeps the
  gameplay overlay of ADR 0053.
- The management surface belongs to Velvet's family: its palette, its type,
  and no saturated colour except for danger. Its own problem is hierarchy:
  the device's state and the one or two things that matter come first, and
  the rest is folded away until it is needed.
- One identity serves a television and a desk because the host scales the
  window for the display. The surface receives no display fact (ADR 0086).

## Considered options

- **The surface shrinks to a small live picture.** Rejected: the shell cannot
  draw one surface inside another or capture it, and the owner preferred the
  drawer once both were described.
- **A modal.** Rejected: a modal suits one question, not a place a person
  stays in.
- **The surface is squeezed beside the drawer and stays usable.** Rejected:
  the owner never wants both usable at once. Xbox One removed its Snap mode
  in 2017 for the cost of resizing a running game.
- **The host draws a mark at the drawer's edge on every surface.** Rejected:
  a surface owns every pixel.

## Consequences

- The shell holds one surface at a time today and unloads the one it leaves,
  which then starts again from nothing. A drawer beside a live surface needs
  two surfaces running at once. Every device runs Sway, and korrid already
  places windows through it; nobody has yet placed two surfaces that way.
- Building the drawer in the shell is not scheduled. The provisional order is
  to design both stops now and to ship the fully open form on the web screen
  first.
- Android allows an app to open a system panel at one setting; Apple forbids
  it and no console offers it. Opening the drawer at one capability is the
  less common choice, taken for small specialised surfaces.
- A hidden panel is found less often than visible navigation. A surface that
  shows no way in relies on the person knowing the button.
- The mockup draws the drawer from the left edge, so that its first column
  stays where it is and the second appears beside it, as in the Xbox guide.
  Velvet's own sheets come from the right. The owner has not ruled on the
  edge.
- Sony swapped tap and hold on its system button between two consoles and
  met years of complaints. A tap of Guide therefore keeps its present
  meaning in every surface.
