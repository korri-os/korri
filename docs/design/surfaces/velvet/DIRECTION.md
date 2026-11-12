# Velvet

A cinema before the film: a dark house, lit by the game in front of you.
Built: `clients/shell/packages/velvet/` (#423, #424, #425). The mockup that
led to it holds real box art and so lives outside the repository
(`~/code/sandbox/korri-mockups/theater`, `DECISIONS.md` there); nothing in the
package or its previews comes from it. The package's own README has the
structure, the plan, the limits and the list of screens.

**Skeleton.** Depth, not pages. Home is one game filling the stage. Down is
the row under it, then the lobby's shelves, then a set of games; Confirm on
any game opens it over the whole stage, and Confirm again starts it. Back
goes up one level, always.

**Memorable element.** The stage light. The house is near black, and what
lights it is the focused game: its hero as a band across the top, its colour
as a glow where there is no hero, a colour from its title where there is no
art at all. Move the focus and the house changes colour.

**Palette.**

| Name | Value | Use |
| --- | --- | --- |
| House down | `#040406` | The stage, every ground |
| House light | `#f3eee6` | Titles, the focus edge, the one filled button |
| Usher | `#b3ada3` | Facts and quiet words |
| Ink | `#0a0a0c` | Words on House light |
| Exit | `#f0655a` | What cannot be undone, what went wrong; never decoration |
| Velvet | House down with alpha | Scrims and veils |
| The light | from the art's `color`, else one of twelve gels by title | Saturation 0.30 to 0.62; as bright as lets House light keep 4.5 to 1 on its core |

No blue accent: the only saturated token is Exit.

**Type.** Big Shoulders ExtraBold, caps, for display (titles, heads, the
no-art card). Archivo, semi-condensed, 400 and 700, for everything read as a
sentence or a fact. Both SIL OFL 1.1; the licence texts are in the package's
`fonts/`.

**Text.** A logo instead of a title where Korri has one. Facts (system, last
played, time played, where it plays) only after the focus rests 450 ms. No
description, no genre, no year: Korri publishes none.

**Text entry (C29).** A keyboard over the lower part of the stage: five rows
of ten, letters, a symbols layer, Space, Delete, Done; the draft and what it
is for stand over it. A secret shows as dots and has "Clear key". The stage
goes dark under it; Back puts it away.

## Motion

| What | How | Token |
| --- | --- | --- |
| Focus moves | The tile grows 1.08 from its foot and takes the edge; neighbours never shift | `t-focus` 240 ms, `cubic-bezier(0.16, 1, 0.3, 1)`, no overshoot |
| The stage changes game | The band and the light cross-fade, no dark dip | `t-backdrop` 380 ms, `t-light` 420 ms |
| A level down or up | The page slides a window's height | `t-layer` 420 ms, `t-level` 360 ms |
| A tile arrives | Opacity 0 to 1 and scale 0.95 to 1 | `t-reveal` 420 ms, `t-stagger` 40 ms |
| A row not in focus | Under a veil at about half | `t-focus` |
| Facts | After the focus rests | `dwell` 450 ms, then `t-meta` 300 ms |
| The detail | On the stage inside | `t-detail` 150 ms |
| A sheet | From the right edge | `t-sheet` 220 ms |
| Rest | After 2 s the hero drifts, 18 s each way, 10 % at most; any press stops it at once | `drift` |
| Reduced motion | Every change lands at once; no drift | `velvet.motion`, mark `device` |

No live blur anywhere (the authoring gate).

## Capability map

```
C1  Loading: the house lights and nothing else, the Menu pill in focus. Error: THE LIBRARY DID NOT OPEN, Try again. Empty: THE STAGE IS YOURS, Find games focused, Open settings beside it. Ready: the pick.
C2  The pick, the row, the lobby's shelves, a set. A section that is not Korri's own is a shelf, "Plays on <section>". Art of any shape: hero as the stage, logo as the name, poster as the tile, a square cover over a field, else the no-art card (a system poster, or a pattern tile).
C3  Facts under the name after the dwell: Ready to resume, when it was last played, time played. The Not started shelf holds games never played.
C33 The detail's note: "Needs a 1.2 GB download before it can start", then its progress, or that it waits behind another.
C4  The detail. One place: Play (Resume). Two or more: one button a place, "Play on <place>"; none is chosen for the person.
C5  The detail's More, and `options`: a sheet of the game's actions; a destructive one asks first.
C6  No answer yet: text search is out of scope for the first cut. Sort (recent, A to Z, most played) and the system filter stand in.
C7  The curtain: the house goes dark, the kicker in caps, the game's name over it, Cancel when Korri offers it.
C8  The curtain, "Playing now", no button.
C9  The curtain with the Exit mark: what went wrong in Korri's words (paged when long), Dismiss, and Korri's actions.
C10 The error page's Try again; the house menu's device actions.
C31 The curtain as a question: Korri's words, its two answers, the safe one focused.
C32 A bar under the kicker with Korri's words about it; launch steps as a row of bars, "Step 3 of 4".
C42 A notice at the stage's foot when an invite arrives; Invites in the house menu, each with its answers.
C43 The curtain: the session is paused elsewhere, with Korri's actions.
C44 The curtain: the session ended, with Korri's words and Dismiss.
C45 A fact in the overlay, under the clock: who is playing, by name.
C34 A fact in the detail: Saves in sync, Saves not in sync, Saves kept by Steam, Two saves to choose from.
C35 The detail's Choose a save, and the curtain: each save a button with its device and when.
C36 Saves, a list page: each save a row, restore asks first.
C11 Run with, a list page: one row a runner, the one in force marked.
C37 Launch settings, a list page: Applies to steps the scope; rows by group; Reload and Close at its foot.
C38 The page's subtitle names the profile in force; the scope row steps through places and profiles.
C12 Settings, a list page: a quiet head a group, a row an item, its value at the right.
C13 Choice: Left and Right step through the choices and send at once; Confirm steps forward. Range: a track. Text, number and secret: the keyboard. Action: Confirm; a destructive one asks on the curtain.
C14 The row says Saving; a problem stands under its row in Exit.
C15 The house menu's lower rows: Allow controllers, Restart Korri, Sleep, Power off (in Exit).
C16 Identity and backup: Back up this identity, the password on the keyboard, then the QR code and the text beside the rows.
C17 Switch to a backup: the text, then the password, then what to do with the old identity.
C18 Switch to a bunker: its address on the keyboard.
C19 Retired identities as rows; one opens the curtain: Cancel, Back it up, Delete.
C20 The curtain: working, done, or what went wrong.
C21 The Wi-Fi page's subtitle; the house menu's Wi-Fi row names the network.
C22 Wi-Fi, a list page: Scan, then a row a network with its bars, Saved, Open or Needs a password.
C23 Confirm on a network; its password on the keyboard; a wrong one opens the keyboard again; Hidden network asks for a name.
C24 A panel on the left of the running game: PAUSED or the game's name, the clock, Continue first, Korri's controls by group, Exit in Exit.
C25 A list page a step, "Step 2 of 4": controllers, Wi-Fi, names, the starter pack when offered, then Finish.
C39 Find games, a list page: a row a provider.
C40 A provider's page: Search opens the keyboard; results as rows.
C41 An entry's page: its words, Get, then what the download needs, its bar, Cancel.
C26 The clock beside the Menu pill on home; the build at the foot of Settings.
C27 After 2 s of rest the stage drifts. There is no attract screen.
C28 `confirm` opens and starts; `back` goes up; `menu` opens the house menu; `options` opens sort and filter in a set and a game's actions in the detail; `system` opens Settings.
C29 The keyboard (above).
C30 One focus, always in sight: a white edge and a tile grown from its foot; a filled pill; a filled row. Hidden while a finger is in use.
```
