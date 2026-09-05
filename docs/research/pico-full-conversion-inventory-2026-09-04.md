# Pico full-conversion inventory, 2026-09-04

Complete atomic breakdown of legacy's Pico kit for conversion into
`surfaces/pico`. Every component in `product/surfaces/web/pico/ui` is listed
once, with its legacy dependencies and the Korri data that would feed it.

## Correction on scope

The kit is **94 components**, not 280 files. Legacy stores three files per
component (`X.tsx`, `X.story.tsx`, `X.<layer>.part.tsx`), and `.story.tsx` does
not convert — Pico's authoring gate forbids story files, and Caliper reads parts.

| Layer | Legacy components | Pico has | To convert |
|---|---|---|---|
| Atoms | 13 | 8 | 13 |
| Molecules | 16 | 4 | 16 |
| Organisms | 62 | 3 | 62 |
| Templates | 3 | 1 | 3 |
| **Total** | **94** | **16** | **94** |

Pico's existing 16 are not a subset of legacy's 94 — they are Korri-shaped
(`PicoLocationPicker`, `PicoLaunchStage`, `PicoTally`) and have no legacy
counterpart. Conversion adds to them; it does not replace them.

## The constraint that shapes the whole plan

Korri publishes exactly this, and a surface may show nothing else:

- `catalog` — games with `id`, `title`, `section`, `subtitle`, `coverArtUrl`,
  `wideArtUrl`, `resumable`, `lastPlayedAt`, `playCount`,
  `totalPlaytimeSeconds`, `launchLocations`
- `status` — idle / launching / running / problem
- `actions` — device-level `SurfaceAction`s
- `settings` + `settingsStatus` — grouped device facts and editable settings
- `clockLabel`, `buildLabel`
- `presentation` — `catalog` or `gameplay-overlay` (with controls and groups)

Three tiers follow, and every component below is tagged with one:

- **[A] Feeds today.** A real field exists. Convert now.
- **[B] Surface-local.** Derived from the catalog by the surface itself —
  filtering, sorting, sectioning, focus. No new Korri data. Convert now.
- **[C] No source.** Korri publishes nothing that feeds it. Converting it means
  inventing a fact, which is the one rule the surface treaty does not bend.
  Each needs a korrid capability and a treaty extension *first*.

## Atoms (13)

| Component | Purpose | Tier | Pico counterpart |
|---|---|---|---|
| `Badge` | Small status pill | A | new |
| `BlockBar` | Segmented block meter | A | new (settings ranges) |
| `Btn` | Labelled button | A | `PicoButton` exists; reconcile |
| `Chip` | Filter/tag token | B | new |
| `Dim` | Scrim / dimming layer | A | folded into `PicoBackdrop`; extract |
| `Glyph` | Large decorative pixel glyph | A | new |
| `Icon` | Pixel icon by name | A | new |
| `Progress` | Determinate progress bar | C | **blocked** — Korri publishes no percentage |
| `Spinner` | Indeterminate activity | A | exists as barber-pole; extract |
| `Stat` | Label + value pair | A | new |
| `Sub` | Secondary text | A | new |
| `Title` | Primary heading | A | new |
| `Toggle` | On/off control | A | new (settings) |

`Progress` is the first honest casualty. Legacy used it for downloads and
installs, neither of which Korri reports. It converts only as the barber-pole
already built, which deliberately never fills.

## Molecules (16)

| Component | Legacy deps | Tier | Note |
|---|---|---|---|
| `Card` | — | A | container |
| `DetailHead` | — | A | game detail header |
| `GameCart` | — | A | `PicoCart` exists; reconcile |
| `GameCartUnmarked` | — | A | `PicoCart` without label text |
| `GameLogo` | — | A | wordmark over key art |
| `HostBadge` | — | C | no host/streaming model |
| `KeyArtBackdrop` | — | A | `PicoKeyArt` exists; reconcile |
| `List` | — | A | vertical run |
| `Opt` | — | A | option row for settings |
| `PlayCta` | — | A | primary launch call |
| `Player` | — | C | no player/seat model |
| `QualityBar` | — | C | no stream quality model |
| `Row` | — | A | list row |
| `SearchQuery` | — | B | filters the catalog locally |
| `SettingRow` | — | A | `settings` groups |
| `Tabs` | — | B | sectioning over catalog |

## Organisms (62)

### [A] Feeds from today's treaty — 11

| Component | Legacy deps | Korri source |
|---|---|---|
| `ContinueList` | Dim, GameCartUnmarked, List, Row | `resumable`, `lastPlayedAt` |
| `ControlCenter` | BlockBar, Toggle | `settings`, `actions` |
| `DualPrimaryStage` | Btn, GameCartUnmarked, Icon, Title | launch + `gameActions` |
| `HudOverlay` | Icon | `gameplay-overlay` |
| `LastPlayedHero` | GameLogo, KeyArtBackdrop, PlayCta | `lastPlayedAt`, `wideArtUrl` |
| `LaunchingStage` | GameCartUnmarked, Spinner, Title | `status: launching` |
| `Modal` | — | problems, confirmations |
| `QuickLook` | Btn, Stat | `playCount`, `totalPlaytimeSeconds` |
| `RunningGame` | — | `status: running` |
| `SystemGrid` | Stat | `settings`, `buildLabel` |
| `FailureList` | — | `status: problem`, `settingsStatus` |

### [B] Surface-local over the catalog — 12

| Component | Legacy deps | Derived from |
|---|---|---|
| `AttractLoop` | GameCartUnmarked | idle timer + catalog |
| `CollectionList` | Badge, List, Row | `section` |
| `CoverflowRail` | GameCartUnmarked | catalog order |
| `FeaturedToday` | Badge, Btn, GameCartUnmarked, Icon, Title | catalog pick |
| `FiltersPanel` | Chip, Opt | local filter state |
| `FilterSortPanel` | Chip, Dim, Opt, Row, Toggle | local filter state |
| `Hero` | Glyph, Title | focused game |
| `LaunchTube` | GameCartUnmarked | launch transition |
| `LibraryRail` | GameCart | catalog |
| `MiniHome` | — | catalog |
| `OnScreenKeyboard` | — | local text entry |
| `ReactiveStage` | Dim, GameCartUnmarked | focus reaction |
| `SearchResults` | Icon, List, Row | local filter over catalog |
| `ShelfGrid` | GameCartUnmarked | catalog (grid form of `PicoCartShelf`) |
| `SpotlightHero` | GameCartUnmarked, GameLogo, KeyArtBackdrop, PlayCta | focused game |

### [C] No Korri source — 35, blocked

Each row names the capability that must exist before the component can be
converted without inventing a fact.

| Capability Korri lacks | Blocked organisms |
|---|---|
| **Friends / presence** | `FriendsList`, `FriendsPanel`, `InviteList`, `PlayerToast`, `CompanionCard` |
| **Multiplayer seats & lobby** | `CrewLobby`, `SeatList`, `SeatAssignList`, `InlineSeatStrip`, `PlayersHub`, `PlayerStyleMatrix`, `SessionDock`, `SessionPlayersHud`, `LobbyArtStage`, `CountdownStage`, `JoiningStage` |
| **Achievements & profiles** | `AchievementList`, `LeaderboardTable`, `CommunityStatPanel`, `ProfileCard` |
| **Store, downloads, installs** | `StoreView`, `AppChoiceList`, `DownloadConfirmCard`, `DownloadProgress`, `InstallProgress`, `ReleaseList`, `RepairProgress`, `UpdatePanel` |
| **Remote hosts & streaming** | `HostCardList`, `HostScanList`, `StreamPanel` |
| **Save slots & captures** | `SaveSlotGrid`, `ScreenshotGallery`, `MomentHero` |
| **Input remapping** | `RemapList` |
| **Boot/first-run sequence** | `BootStepper` |

That is **35 of 62 organisms**, 56% of the layer. They are not hard to draw —
they are unbacked. Converting them produces screens that can only ever show
fixtures, which is the failure this port was built to avoid.

## Templates (3)

| Component | Tier | Note |
|---|---|---|
| `ScreenShell` | A | `PicoScreenShell` exists; reconcile |
| `GameOverlay` | A | serves `gameplay-overlay`; Pico's real gap |
| `PanelScreen` | A | settings/system panels |

## Conversion order

Dependencies run atoms → molecules → organisms → templates, so the order is
forced. Within each tier, tier A and B before C.

1. **Atoms, 12 of 13.** All but `Progress`. Reconcile `Btn`/`Spinner`/`Dim`
   against Pico's existing parts rather than duplicating them — the
   decomposition gate rejects a class name defined in two files.
2. **Molecules, 13 of 16.** All but `HostBadge`, `Player`, `QualityBar`.
3. **Templates, 3.** `GameOverlay` is the highest-value single item in this
   document: it is the one presentation Korri routes that Pico does not serve,
   and it ends the fallback to Shift.
4. **Organisms A and B, 27.** Real screens on real data.
5. **Organisms C, 35.** Blocked. Each needs a korrid capability first.

## Recommendation on tier C

Do not convert tier C as UI-only shells. Two options that keep the rule intact:

- **Extend the treaty per capability**, one at a time, with korrid actually
  publishing the data. Eight capability areas, each its own slice.
- **Convert them into the harness, not the surface.** They are excellent design
  exploration and legacy kept them exactly that way — 15 prototype `pages/`
  directories that no route imported.

Legacy shipped **two routes** (`PicoHomeRoute`, `PicoGameDetailRoute`) out of
this entire kit. That is the strongest available evidence for what the kit is
for.
