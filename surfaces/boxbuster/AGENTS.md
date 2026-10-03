# boxbuster — surface charter

> **On `main`.** This charter was harvested unchanged from
> `legacy:product/surfaces/web/boxbuster/AGENTS.md`. Its `work/items/...` paths
> exist only on `legacy` (read them with `git show legacy:<path>`). Of the
> files it names, `controls.tsx`, `app.tsx`, `config.tsx`, and `layout.ts`
> stayed on `legacy`: Boxbuster on `main` is a treaty surface
> (`contracts/surface/`), so it is focus-driven, the host owns input, and the
> first-person WASD and mouse-look model does not apply. `BoxbusterSurface.tsx`
> is the only component that reads the treaty, and `boxbuster-store-view.ts`
> is the only conversion from it. `test/boundary-gate.test.ts` holds both rules.
> Rooms are chosen from play facts (decided 2026-10-02; legacy sorted by year,
> which the treaty lacks): resumable → the return cart by the door; last
> played within 14 days → New Releases, newest first; played before that →
> Staff Picks, most played first; never played → Classics, catalog order.
> `boxbuster-store-view.ts` owns the rule; `map.ts` only lays it out.

`boxbuster` is the PS1-era 3D video-store browse surface (`@korri:boxbuster`). It is an
attempt to make a game launcher that feels like **somewhere you go**, not **something you
query**. This file is the north star: read it before changing the experience, and use the
four tests below as the bar any change must clear.

The charter is synthesized from a single source — Saleh's *Interface Studies* essay on
**placeness** in software, which builds on Harrison & Dourish, *Re-Place-ing Space* (CSCW
1996). Keep the source's distinction in mind throughout:

> **Space is geometric arrangement; place is a space that has acquired meaning through use.**
> Placeness can be designed *for*, but not designed *in* — it develops through return, ritual,
> and use over time.

That sentence is the whole reason boxbuster exists. A grid of cover art is *space*. A store you
have a history with is *place*. We are building the conditions for the second.

## The four tests

Every location, interaction, and visual choice in this surface should be checkable against
these four properties of place. When reviewing a change, ask: does it strengthen one of these,
or quietly erode it?

1. **Stable spatial layout.** Things are where you left them. You remember where the
   new-releases wall is because it is still there when you return. → *Boxbuster's weakest point
   today: RNG shelf seeding is the single thing most directly destroying placeness. A
   deterministic, self-merchandising layout is a precondition, not a nice-to-have. See*
   `work/items/parking-lot/01KVXYT66Y5YXED3PE247PNDDF-self-merchandising-store-deterministic-layout.md`.

2. **Its own frame / recognisable identity.** A screenshot is unmistakably *this* surface, not
   "assembled from the same component library as everything else." → *Boxbuster's strongest
   asset — the PS1 aesthetic owns its outer frame completely. Protect it; do not let
   design-system grammar leak into the world.*

3. **Edges.** The thing is bounded and finite — a store, not an infinite feed. The essay's best
   line for us: *"the disk was finite, and the disk was yours."* Your **library** is finite and
   yours; the store is the room it lives in. → *Never reframe boxbuster as "browse all games";
   that is the unbounded feed the essay warns against.*

4. **Affordances tied to location.** What you can do *here* differs from *there*. A village is
   not a cave is not a workshop is not a menu screen. → *The counter is where you rewind and the
   clerk knows you; the back room holds the cult stuff; the kiosk is where you sample. Build
   rooms that each let you do something the others can't. The meta-story parking-lot items
   (`topic: blockbuster-ps1-meta-story`) are mostly applications of this test.*

## Designed *for*, not *in*: the continuity imperative

Placeness cannot be faked in a single session. The store must **persist and remember** across
visits, and the rituals only mean something across a gap of absence:

- your mess from last night is still on the floor;
- a returned tape has been refiled, spine-out, a touch more worn;
- the carpet wears a path along the route you actually walk;
- the clerk greets you differently on visit ten than on visit one.

This has a hard consequence for scope: the deferred **"rent / go home / come back" loop is not a
side feature — it is the clock every ritual runs on.** Design as if it is coming, even before it
is built. The two conditions the essay ends on are our build contract:

1. **someone arranges the environment** (its layout, frame, edges, locations) — here, that's us;
2. **and it is maintained long enough for users to build memory inside it** — here, that's the
   in-world staff plus session persistence.

## What erodes placeness — and our stance

The essay names four shifts that drained placeness out of modern software. Boxbuster exists to
resist each; do not reintroduce them in the name of consistency or velocity:

| Shift the essay names | Our stance in this surface |
|---|---|
| **The browser flattened the frame** (tabs, chrome, back button) | boxbuster owns its full frame — a WebGL world, not a page. The 2D HUD is minimal and adapts via container queries (`boxbuster.css`), never standard web chrome. |
| **Design systems flattened the inside** (Material/HIG/Tailwind sameness) | The scene is an opaque R3F-native leaf, deliberately *outside* the atoms/molecules catalog (see `config.tsx`). Keep it that way — shared components belong on the HUD edges, not in the world. |
| **Feeds replaced pages** (a stream that regenerates each visit) | The store is a stable location, not a feed. This is why RNG seeding is a charter violation, not a rough edge. |
| **Generation replaced arrangement** (answers synthesized, not authored) | Layout is *authored / deterministic arrangement*, so knowledge attaches to location — you remember the aisle, the endcap, the shelf. Do not make placement incidental. |

## Rules of engagement

- **Match ceremony to the test, not to taste.** Before adding a flourish, name which of the four
  properties it serves. If it serves none, it is decoration — fine as delight, but it is not the
  reason this surface exists.
- **The fast path stays fast.** Placeness is *opt-in depth*. A user who wants to resume and play
  in three seconds must never be taxed by the world. Every ritual is a layer you may descend into,
  never a toll. (See the resume-first item:
  `work/items/parking-lot/01KVXYT670HYFHEX9K2C7QG8M1-resume-first-open-on-next-play.md`.)
- **Never the only way in.** Diegetic browse is layered *over* a fast index and a legible,
  reduced-motion, high-contrast fallback rendered from the same state — not instead of one. PS1
  dithering is art in the world and a liability on text; zone it. (See
  `work/items/parking-lot/01KVXYT670G29HW4P4PQ2BBD2K-zone-the-diegesis-ambient-world-functional-panel.md`.)
- **Treat objects as stateful.** A tape carries *how you left it* — returned, rewound, worn,
  misfiled. Continuity lives in the user's treatment of the object, not in the media's content
  (this is also how the rewind ritual translates to games with no narrative).
- **Rooms are curation.** The store is a *connected map* — a hub lobby with themed rooms opening
  off it through wide archways (New Releases / Staff Picks / Classics), plus the fixed Viewing
  Room behind the hub. See `map.ts` (`computeMap`): it partitions the library across rooms and
  emits the whole floorplan (floors / wall segments with archway gaps / lights / gondolas / nav
  rects) as deterministic data; `scene.tsx` renders it, `controls.tsx` navigates it. The *place*
  tells you what you're looking at — no filters, no badges. Each room carries its own density +
  accent so it *feels* distinct. (`layout.ts`'s `computeLayout` — the earlier single-room,
  library-sized model — is kept for reference but the map is the live structure.) Adding a room is
  a new entry in `ROOMS_BASE` + its placement; v1 uses fixed room boxes, content-driven room
  sizing is the next step.
- **Lived-in over lined-up.** Games are spread across the shelves with natural, irregular gaps —
  "rented", never repeated to fill space and never packed wall-to-wall. The slight not-lined-up
  irregularity is load-bearing: in a stylized, deliberately-unrealistic world it's what makes the
  store read as *real and inhabited*. Don't "tidy" it into a perfect grid.
- **Verify before claiming absence.** Standard repo rule — if you say a mechanic, prop, or piece
  of state doesn't exist here, read the source (`scene.tsx`, `vhs.tsx`, `controls.tsx`,
  `app.tsx`) first.

## Where the charter is applied

- Parent ideation (full grounding, the 7 survivors, rejection summary):
  `work/items/parking-lot/01KVX589XXVGE3MM0G5GHY042Y-blockbuster-ps1-launcher-browsing-ux.md`
- The meta-story / placeness layer: all parking-lot items with `topic: blockbuster-ps1-meta-story`
  (worn path, returns chute, new-release cadence, hold shelf, regular→keyholder, misfiled tape,
  closing time, be-kind-rewind, back room, demo kiosk, deferred overdue).

## Source

Saleh, *Interface Studies* — "when software starts to feel like somewhere instead of something."
Drawing on Harrison, S. & Dourish, P., *Re-Place-ing Space: The Roles of Place and Space in
Collaborative Systems*, CSCW 1996. Touchstones the essay holds up as places that survived:
the DK Eyewitness CD-ROM encyclopedias, PostHog's OS-style site, Not Boring Software, The
Elements / Shakespeare's Sonnets, and the Explorable Explanations tradition (Bret Victor,
Bartosz Ciechanowski).
