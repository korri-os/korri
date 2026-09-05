# Pico full-conversion execution plan, 2026-09-04

Companion to `pico-full-conversion-inventory-2026-09-04.md`, which names all 94
components. This is the order of work, the verification at each step, and the
conditions that end each phase.

**Scope:** 55 components on data Korri publishes today (12 atoms, 13 molecules,
3 templates, 27 organisms), plus 5 reconciliations of parts Pico already has.
The remaining 35 organisms are blocked on capability and are decided in Phase 6.

**Kiosk work is explicitly out of scope** by the owner's decision. Verification
is therefore browser and Caliper, with the device used opportunistically through
the existing ad-hoc path.

## Principles this plan holds to

- **Gates before the components they govern.** Every new rule is watched failing
  against a tripwire before it is trusted.
- **No component lands without a consumer.** The kit is converted *through* four
  screens, not ahead of them. Legacy's kit served zero routed screens; that is
  the outcome this ordering exists to prevent.
- **Nothing is drawn that Korri does not publish.** A component with no source
  is blocked, not filled with a plausible number.
- **Screenshot every screen.** Six of the ten defects in the first slice were
  invisible to a green suite.

## Phase 0 — Foundation

Nothing is authored until the scale is correct, because every part written
before the fix bakes in the wrong one.

- Fix `01M1R06TXJNZ2DHV4CSGEEZ2GX`: make the element carrying Pico's knobs
  participate in the recipe's derivation so `--intrinsic-base` comes from Pico's
  floor, anchor, ratio — not the package defaults.
- Confirm `--intrinsic-snap: 1px` reaches the base and the type ramp, so the
  bitmap font stops landing on fractional sizes.
- Confirm the three Caliper design inputs now visibly change the render.
- Re-screenshot the home screen and compare against today's, so the scale change
  is a deliberate, reviewed diff rather than a silent drift.
- Extend the gates, each proven against a tripwire first:
  - **Reconciliation:** one role, one component. No second cart, spinner, scrim
    or shell.
  - **Provenance:** a part's fixture values come from a shared fixture module
    typed against `SurfaceModel`, so no part can demonstrate a field Korri does
    not publish.
  - **Consumer:** every atom and molecule is imported by at least one organism,
    template or page. This is the rule that makes a 55-component conversion
    honest.

**Ends when:** gates are red-then-green, the scale is Pico's own, and the home
screenshot is re-approved.

## Phase 1 — Atoms (12 new, 4 reconciled)

- **Convert:** `Badge`, `BlockBar`, `Chip`, `Glyph`, `Icon`, `Stat`, `Sub`,
  `Title`, `Toggle`, and the three that need Korri-shaped rewrites.
- **Reconcile, do not duplicate:** `Btn` → `PicoButton`, `Spinner` → the
  existing barber-pole, `Dim` → extract from `PicoBackdrop`, and keep
  `PicoPixelDisc` as the shared pixel primitive.
- **Drop:** `Progress`. Korri reports no percentage for anything. Recorded in
  the inventory as the first honest casualty.
- Each lands as component + CSS beside it + `.atom.part.tsx`, one per file.
- Read the legacy CSS for each before writing, not just the TSX. Every effect
  lost in the first port was lost by reading the component and not its
  stylesheet.

**Ends when:** 12 atoms pass both gates, all appear in Caliper, and a contact
sheet of every atom is screenshotted and reviewed.

## Phase 2 — Molecules (13 new, 1 reconciled)

- **Convert:** `Card`, `DetailHead`, `GameCartUnmarked`, `GameLogo`, `List`,
  `Opt`, `PlayCta`, `Row`, `SearchQuery`, `SettingRow`, `Tabs`.
- **Reconcile:** `GameCart` → `PicoCart`, `KeyArtBackdrop` → `PicoKeyArt`.
- **Block:** `HostBadge`, `Player`, `QualityBar` — no host, player or stream
  model exists.

**Ends when:** 13 molecules pass, each imported by something, contact sheet
reviewed.

## Phase 3 — Templates (3)

- `ScreenShell` → reconcile with `PicoScreenShell`.
- `PanelScreen` → new, for settings and system panels.
- `GameOverlay` → new. **The highest-value single item in the whole plan:** it
  is the one presentation Korri routes that Pico does not serve, and it ends the
  portal's fallback to Shift for overlays.

**Ends when:** the registry declares Pico serves `gameplay-overlay`, and the
portal test proving the Shift fallback is replaced by one proving Pico serves it.

## Phase 4 — The four screens that consume the kit

Built page-first. Each is a real destination, not a gallery.

- **Game detail.** The second route legacy actually shipped. Consumes
  `DetailHead`, `GameLogo`, `PlayCta`, `Stat`, `QuickLook`, `Title`, `Sub`,
  `KeyArtBackdrop`. Surface-owned navigation inside the `catalog` presentation.
- **Gameplay overlay.** Consumes `GameOverlay`, `HudOverlay`, `Modal`, `Btn`,
  `Toggle`, `Opt` against real `SurfaceGameplayControl` groups.
- **Settings / system panel.** Consumes `PanelScreen`, `SettingRow`, `Opt`,
  `Toggle`, `BlockBar`, `SystemGrid`, `ControlCenter`, `FailureList`, and
  `buildLabel`, against real `settings` groups and `settingsStatus`.
- **Search, filter, collections.** Consumes `SearchQuery`, `OnScreenKeyboard`,
  `SearchResults`, `FiltersPanel`, `FilterSortPanel`, `CollectionList`, `Chip`,
  `Tabs` — all local over the catalog, no new Korri data.

**Ends when:** each screen is reachable by input, screenshotted in the portal at
device size, and its states are covered.

## Phase 5 — Organisms, tiers A and B (27)

- **Tier A, fed by today's treaty (11):** `ContinueList`, `ControlCenter`,
  `DualPrimaryStage`, `HudOverlay`, `LastPlayedHero`, `LaunchingStage`, `Modal`,
  `QuickLook`, `RunningGame`, `SystemGrid`, `FailureList`.
- **Tier B, surface-local over the catalog (16):** `AttractLoop`,
  `CollectionList`, `CoverflowRail`, `FeaturedToday`, `FiltersPanel`,
  `FilterSortPanel`, `Hero`, `LaunchTube`, `LibraryRail`, `MiniHome`,
  `OnScreenKeyboard`, `ReactiveStage`, `SearchResults`, `ShelfGrid`,
  `SpotlightHero`.
- Convert in dependency order; several are already consumed by Phase 4 screens
  and land there.
- `AttractLoop` needs an idle policy the surface owns — decide the timeout and
  the wake input explicitly rather than copying legacy's number blindly.

**Ends when:** all 27 pass gates, each is reachable or consumed, and the full
set is reviewed in Caliper at true device size.

## Phase 6 — Tier C decision point (35 organisms)

Not a build phase. A decision I will bring back with evidence, per capability
area: friends, seats, achievements, store, streaming, saves, remapping, boot.

- For each, state what korrid would have to publish and what the treaty
  extension looks like.
- Recommend one of: extend the treaty as its own slice; build into the harness
  as design exploration; or drop.
- **I will not convert these as UI shells on fixtures**, because that produces
  exactly the unbacked components already filed against Shift.

## Phase 7 — Close-out

- Full suite: `pico-check`, `portal-check`, `shift-check`.
- Caliper pass over all parts at RG353M, THOR and Odin 2 Portal sizes.
- Device demo through the existing ad-hoc path.
- Update the bring-up doc with what the conversion changed.
- Close or re-file every backlog item this touched.

## Cadence

- One atomic commit per component or reconciliation, conventional commits.
- One merge to `main` per phase, with the phase's screenshots reviewed first.
- Tests green at every commit; screenshots at every screen.
- Any component that turns out to be unbacked mid-conversion moves to tier C
  rather than being completed on invented data.

## Honest sizing

55 components across seven phases. The first slice was 16 components and found
ten defects, four of which were only visible in a browser. This is roughly three
and a half times that, against a foundation that is now proven and gates that
now exist. The long pole is not the atoms — it is Phase 4, where four screens
have to be designed as destinations rather than assembled from parts.
