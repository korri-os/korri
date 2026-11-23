# Bonfire

The menu you open at a fire in a FromSoftware game: a dark, quiet place to
put your things in order before you go back out. First drawing, 2026-11-23,
for the surface that ADR 0096 names. One page is drawn: the devices. It is a
homage made from our own art and words. Nothing here is taken from the games.

The owner asked for a maximalist match to Demon's Souls, Dark Souls,
Bloodborne and Elden Ring. This drawing anchors on Dark Souls for the mood
and on Elden Ring for the readable layout. The owner has not yet confirmed
that anchor.

**Skeleton.** A list and its item. The left column is one row per device;
the right pane shows the chosen device as the games show one item: a picture,
its facts, a line of lore, then a plain line. Up and down move the choice.
Actions belong to the device that serves the screen, so only that device
shows one (ADR 0096).

**Memorable element.** The fire itself. A device that answers is a lit fire.
One that Korri is still looking for is kindling. One that did not answer is
cold ash. The same mark sits in the list row and, large, in the item pane.

**Palette.**

| Name | Value | Use |
| --- | --- | --- |
| Pitch | `#070605` | The ground |
| Soot | `#13100c` | The item pane |
| Iron | `#2b251c` | Quiet rules between facts |
| Bone | `#dcd3bd` | Words |
| Ash | `#91876f` | Quiet words |
| Tarnish | `#b59555` | Ornament, titles, the frame |
| Ember | `#ec7a2b` | The chosen row, a lit fire, the one action |
| Cold | `#66707a` | A fire that is out |
| Blood | `#c4523e` | What went wrong; never decoration |

**Type.** EB Garamond 12 pt design, regular and italic, with its own small
capitals for the page title and the event line. SIL OFL 1.1. `build.sh`
subsets it to Latin and inlines it; `index.src.html` is the source and
`index.html` is the built page.

**Voice.** Facts are plain. Each device also carries one italic line in the
manner of an item description. The plain line under it says the same thing
without the manner, so nobody needs the lore to act.

**Encodings.**

| Visual | What it encodes | The word it keeps |
| --- | --- | --- |
| Flame mark in a row, and the large fire | The peer's state: ready, loading, failed | Lit, Kindling, Cold |
| Ember band and gold lozenge | The chosen row | none; it is the cursor |
| Filled ember plaque | The one action on this page | Its own label |
| Ember value in the facts | A fact that needs the person | "Not signed in" |
| Blood value in the facts | A failure and its reason | "Cold. No path to the device" |
| Corner pieces and double frame | Nothing. They are the manner | none |

**Eye flow.** The large fire first, then the device's name, then the ember
action. The list stays quiet until the person moves. The frame and the key
strip are the quietest things on the page.

**Held back.** Sound. Embers that rise (no designed motion yet, so no take).
A second ornament inside the item pane. Caps for the fact labels: the games
use title case there, and caps are a generated-page habit. A "Join a
federation" action on a device that already has an owner.

**Layout.** `plan(width, height)` in the page decides everything:

- Width 860 and over: list and item side by side. Under it: one column, and
  the chosen row opens its item in place.
- Either side under 560: the corner pieces and wide margins go first.
- Height under 620: the large fire goes, and the action moves up beside the
  name so that it stays on the surface.
- The help line in the foot gives way to the keys. Its words also stand in
  the item's plain line.

Every threshold is a guess until it is measured in the real container.

**Stress set.**

| Case | Shown as |
| --- | --- |
| One device | `?state=alone` |
| Fifteen devices, a 60-character name, one unbroken 45-letter word | `?state=many`; the row cuts with an ellipsis and the item pane shows the name whole |
| A peer still loading | `?sel=portal` |
| A peer that failed, with its error | `?sel=oneplus` |
| The event line | `?banner=on` |
| No devices | Cannot happen: the device that serves the screen is always in the list |
| The whole view loading or failing | Not drawn yet |

**Text entry.** None on this page. ADR 0096 decides it for the surface: a
package may use Slint's text field, drawn to fit the design.

**Open.** The content is mock data shaped by `PeerListEntry` and the settings
facts in korrid. The treaty has no list of devices yet, so the facts shown
for a peer (what it runs, its system, its network) are not decided. Contrast
of Ash on Pitch and of Tarnish on Pitch has not been measured.
