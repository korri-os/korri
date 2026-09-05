# Pico surface bring-up, 2026-09-04

## Result

Korri has a second surface. Pico — an 8-bit PICO-8-palette handheld theme — is
built, gated, reviewable in Caliper, selectable by the portal, and running on
the RG353M panel.

- Surface: `surfaces/pico`, 120 tests, `nix run .#pico-check`
- Portal: surface selection via `?surface=pico`, 235 tests
- Device: RG353M at 192.168.1.239, Chromium kiosk on the Korri Sway compositor
- Commits: `3bf622a7` … `d0f12b15` on `main`

The demo on the device is **ad hoc** and does not survive a reboot. See
"Remaining work".

## What was built

### Gates first

Two test files were written **before** the components they govern, and every
assertion was observed failing against a deliberate tripwire before being
trusted.

- `test/decomposition-gate.test.ts` — every rendered unit is a component with a
  part beside it at every layer; one component per file; no `className` literal
  or class selector defined in two files.
- `test/authoring-gate.test.ts` — parts default-export a function and a name;
  atom/molecule/organism parts root in one imported component so they emit real
  Inspector contracts; no raw colour or length outside `pico-tokens.css`; no
  inline styles; no forbidden imports; no design-part registry; no story files.

The gates caught real problems throughout, including two of their own: the
sealed-root check broke on an arrow function in props, and the class-selector
check matched names inside comments. Both were fixed to key off structure.

Scope is `surfaces/pico` only. Shift would be red today.

### The surface

16 parts across all five layers. Home shows a shelf of cartridges; status
outranks the catalog, so starting, running, and failing each take the screen
rather than being drawn over a shelf that invites a second launch.

Deliberate omissions, each with a reason rather than an oversight:

- **No battery or radio.** The treaty publishes neither. Shift renders both and
  is fed by nothing in production — repeating that would mean inventing a
  reading.
- **No gameplay overlay.** `PicoSurface` renders nothing for that presentation
  rather than drawing the library over a running game.
- **No sound.** Legacy's boot chime and navigation blips need a gesture gate and
  a mute the treaty has no setting for.
- **Launch locations are asked, never guessed.** Korri publishes them only when
  there is a real choice, and starting a game on the wrong machine is the one
  launch mistake a user cannot undo from the couch.

### The effects the port had dropped

The first version rendered in fallback monospace with no dither, no motion, no
backdrop and identical flat carts. Every one of these existed in legacy and was
lost in the port; each is now on a real consumer.

| Effect | What it does |
|---|---|
| Cartridge labels | Two palette colours and a dither step hashed from the game id |
| Dithering | Hard two-colour checker, cell size derived from the type anchor |
| Palette remap | Real art sampled to a coarse grid and mapped to the sixteen |
| Key art | The focused game's `wideArtUrl` behind the shelf, with a scrim |
| Backdrops | Drifting parallax starfield; stepped dither weave for working states |
| Motion | Six shared keyframes, all `steps()`, all in container units |
| Barber pole | Travelling stripes that never fill — Korri publishes no percentage |
| Palette cycle | The launch kicker steps through the bright half of the sixteen |
| Marquee | Only when a caption measurably does not fit |

All motion is disabled under `prefers-reduced-motion`, which costs nothing
because every state it dresses is already stated in text.

### Caliper

An external profile (no file in the checkout) renders the real surface at true
physical size on three panels, using legacy's calibrated seeds rather than
invented numbers: RG353M 72×52 mm, THOR 132×76 mm, Odin 2 Portal 156×85 mm. All
16 parts are discovered and contracts generated.

Pico's tunable design inputs are declared as registered custom properties, so a
tool can offer them as live controls and the surface still renders correctly
with no tool present.

### Portal surface selection

A registry names both surfaces and, more importantly, **what each presents**.
Shift does catalog and gameplay-overlay; Pico does catalog only. The portal
picks per presentation, so a themed catalog whose author has not written an
overlay still owns the catalog and the overlay falls back to a surface that has
one. Selection is a query parameter that persists, so a device that can only
load a fixed URL keeps the choice.

## Defects found, and what found them

Ten bugs, grouped by the thing that caught them. The distribution is the
lesson.

**Only a browser could see these.** The suite was green for all of them.

- `container-type: size` contains both axes, so `.pico-screen` could not take
  its height from its contents and collapsed to zero. Every screen was blank.
- No `box-sizing: border-box`, so every `height: 100%` region with padding
  overflowed by exactly its padding and cut off the last line of a caption.
- A bare `1fr` row refuses to shrink below its content, so the shelf pushed the
  position tally off a short screen.
- `scrollIntoView` walks up and scrolls every scrollable ancestor, so keeping
  the hero centred also scrolled the host's page.
- A positioned element with `z-index: 0` paints over ordinary in-flow content,
  so the key art covered the shelf.
- An absolutely positioned child of a grid is sized against its grid area, not
  the container.

**Only the device could see this.** Caliper and the harness could not.

- `pico.css` imported the generator by package name. The portal has no
  `@korri/intrinsic-design` installed under that name, so the import silently
  dropped, every derived token was empty, and the carts computed to zero width.
  It worked in the surface's own tooling only because the surface's own
  `node_modules` happened to exist. Shift hit this first and says so in a
  comment.
- Sizing carts from width let the derived height overflow a wide, short body,
  and flex squashed the box until a cartridge was wider than it was tall.

**Only a test could see this.**

- `>>` is a signed shift, so any game id hashing above 2³¹ produced a negative
  offset, landing the label's accent on its fill and drawing exactly the flat
  rectangle the feature existed to remove. `picoLabelFor("spelunky")` hit it.

**Only rendering the file next to a known-good one could see this.**

- The vendored fonts were the **Cyrillic** and **Vietnamese** subsets.
  `fonts.googleapis.com/css2` returns several `@font-face` blocks per family
  distinguished only by `unicode-range`, and taking the first URL vendors
  whichever Google lists first. Both files fetch with 200, carry the `wOF2`
  signature, and resolve `document.fonts.load()` successfully. They contain no
  Latin glyphs, so every character fell back and every check stayed green.
  Provenance and the failure mode are now recorded beside the files.

## Process failures

Recorded because they cost more than any of the defects.

- **Ported components without their design decisions.** The button hints, the
  effects, the backdrops and the fonts were each caught by the owner rather than
  by me. I read `PicoStatusBar.tsx` and never read the `.pico-key` rules beside
  it, which said in a comment exactly what they were for.
- **Reinterpreted instead of reading.** Told the buttons looked wrong, I
  redesigned them twice from my own judgement before rendering the legacy
  stylesheet to see what it actually drew.
- **Baselined in a git worktree.** Chased six phantom regressions that main did
  not have; the failures persisted with my changes stashed. The `caliper-parts`
  skill warns about this specific trap.
- **Overwrote a working solution.** Replaced a preload that already pinned one
  React across surface packages with a weaker version of the same idea, and
  restored it only after reading what it did.

## Remaining work

### Blocking a real device experience

- **The device demo does not survive a reboot.** It is a reverse SSH tunnel to a
  static server on a laptop plus a transient `systemd-run` unit. There is no
  NixOS module that serves the portal and runs a kiosk on a Linux Korri device.
  This is the single largest gap between "it renders" and "it ships".
- **No real library on Linux.** The portal uses the real korrid client only when
  `window.KorriNative` is present, which is the Android shell. On the RG353M it
  renders in-memory fixtures, so the demo shows Skate 3 and not the device's own
  games.

### Filed

- `01M1R06TXJNZ2DHV4CSGEEZ2GX` — **Pico's intrinsic knobs are inert.** The
  recipe derives `--intrinsic-base` at `:where(:root, .intrinsic)`, and Pico's
  screen carries `pico-theme pico-screen`. The scale is computed from the
  package's neutral defaults; Pico's own floor, anchor and ratio never apply,
  and `--intrinsic-snap: 1px` never reaches the base, so the bitmap font is
  landing on fractional sizes. It looks fine, which is why it went unnoticed —
  and it makes the Caliper sliders move nothing.
- `01M1N42XR11KT03RAAZYGJECXR` — Shift's battery and network atoms are fed by
  nothing in production.
- `01M1N4375QCH4JF8FC93KTR570` — Decompose Shift behind these gates and lift
  them to `surfaces/*`.
- `01KZ6QB5A612V82PJ47M2YZQ7S` — Audit the Shift port against legacy. Filed
  before this work; Pico's experience is direct evidence that it is worth more
  than its priority suggests.

### Pico's own gaps

- **Sound.** Needs a mute; the treaty has no setting for one.
- **Game detail.** Home is the only screen. Pressing A launches; there is
  nowhere to look at a game.
- **`options`, `menu`, `system`.** Only `back` is wired.
- **The marquee has never been seen moving.** Its measurement is tested; its
  motion is not.
- **Ground colour.** Kept navy. Three measured alternates were rejected; the
  question is open and Caliper cannot offer it, because `<color>` knobs return
  null there by design.
- **Legacy screens not ported.** Attract mode, personality, showcase and the
  mascot remain in legacy, along with the effects that only those screens use.
