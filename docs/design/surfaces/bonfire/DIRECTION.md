# Bonfire

The look of the surface that ADR 0096 names: a dark, quiet menu made of real
materials, in the manner of the FromSoftware games. Bonfire is the name of the
surface. It is not shown to the person and it is not the name of anything on
screen. One page is drawn: the devices.

**Owner notes that bind this drawing (2026-11-23).** The first drawing was
"amateur SVG": a drawn campfire, drawn corner curls, fire words everywhere.
The owner asked for higher fidelity, no heavy-handed references, and no
"Bonfire" on the page. So the second drawing uses photographed objects and an
old etching in place of drawn art, and plain words in place of fire words.
The manner is in the materials and the type, not in quotations.

**Skeleton.** A list and its item. The left column is one row per device; the
right pane shows the chosen device as a menu shows one thing you carry: a
picture in a niche, its facts, one italic line, then a plain line. Up and
down move the choice. Actions belong to the device that serves the screen, so
only that device shows one (ADR 0096).

**Memorable element.** The object in the niche. A device with no screen is an
iron casket. A device that plays and serves is an open-work iron box. A
handheld is a small inlaid box. They are real things, lit warm, on black.

**Art, and where it comes from.** All of it is free to use. Sources and
licences are in `art/SOURCES.md`.

| Piece | Source | Treatment |
| --- | --- | --- |
| The hall behind everything | Piranesi, *Carceri d'invenzione*, plate X, 1750 (public domain) | Darkened, warmed, held at 62 % behind two scrims |
| The four corner mounts | Gilt-bronze corner ornament, The Met (CC0) | Cut from its studio ground, tarnished, mirrored |
| The device pictures | Two iron caskets and an inlaid box, The Met (CC0) | Cut or faded to black, warmed |

An image generator was the first plan for this art. Its account had no credit
on the day, so the pieces are museum photographs and an etching instead.

**Palette.**

| Name | Value | Use |
| --- | --- | --- |
| Pitch | `#060504` | The ground |
| Soot | `#100d0a` | The item pane |
| Iron | `#2a241b` | Quiet rules between facts |
| Bone | `#ddd4be` | Words |
| Ash | `#968c74` | Quiet words |
| Tarnish | `#b89a5c` | Ornament, titles, the fillet |
| Ember | `#e0782e` | The chosen row, the one action |
| Blood | `#c9563f` | What went wrong; never decoration |

**Type.** EB Garamond 12 pt design, regular and italic, with its own small
capitals for the page title and the event line. SIL OFL 1.1. `build.sh`
subsets it to Latin and inlines it and the pictures; `index.src.html` is the
source and `index.html` is the built page.

**Voice.** Facts are plain: Reachable, Searching, Unreachable. Each device
also carries one italic line in the manner of an item description. The plain
line under it says what the person needs, so nobody needs the italic line to
act.

**Encodings.**

| Visual | What it encodes | The word it keeps |
| --- | --- | --- |
| The stone beside a name | The peer's state: bright, hollow, dark | Reachable, Searching, Unreachable |
| The object in the niche | What the device runs: server part, both parts, a handheld | The "Runs" fact |
| The object greyed, the niche unlit | The device did not answer | "Unreachable" and its reason, in Blood |
| Ember band between two gold lines | The chosen row | none; it is the cursor |
| The worked-metal plaque | The one action on this page | Its own label |
| Ember value in the facts | A fact that needs the person | "Not signed in" |
| Corner mounts, fillet, the hall | Nothing. They are the manner | none |

**Eye flow.** The object in the niche first, then the device's name, then the
plaque. The list stays quiet until the person moves. The hall, the frame and
the key strip are the quietest things on the page.

**Held back.** Sound. Motion: none is designed yet, so there is no take. A
texture inside the item pane beyond its bevel. Caps for the fact labels. A
second action on a device that already has an owner.

**Layout.** `plan(width, height)` in the page decides everything:

- Width 860 and over: list and item side by side. Under it: one column, and
  the chosen row opens its item in place.
- Either side under 560: the corner mounts and wide margins go first.
- Height under 620: the niche goes, and the action moves up beside the name
  so that it stays on the surface.
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

**Text entry.** None on this page. ADR 0096 decides it for the surface.

**Open.**

- The content is mock data shaped by `PeerListEntry` and the settings facts
  in korrid. The treaty has no list of devices yet, so a peer's facts are not
  decided.
- The built page is about 850 kB because the pictures are inlined. A Slint
  package would ship them as files.
- Whether Slint can draw this: the scrims, the blur behind the pane and the
  drop shadows are CSS here. Nobody has built them in Slint.
- The iron casket is still dark at small sizes.
- Contrast of Ash on Pitch has not been measured.
