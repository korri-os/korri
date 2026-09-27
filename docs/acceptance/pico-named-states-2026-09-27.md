# Pico named states, 2026-09-27

The audit covers all 52 existing parts. It adds 137 named states to 50 parts,
for 189 states including the defaults. All 52 default export bodies remain
unchanged. No existing production component or stylesheet changed.

## Coverage

| Layer | Parts audited | Named states added |
|---|---:|---:|
| Atoms and molecules | 28 | 48 |
| Organisms | 14 | 40 |
| Pages and templates | 10 | 49 |

`PicoPaletteBar` and `PicoPixelDisc` have no inputs. Their default examples
are their only states. Other examples use existing component inputs and the
surface treaty. The part exports are the state list, not a separate registry.

Examples cover missing artwork, content length, selection, disabled actions,
card tones, loading, empty collections, errors, launch outcomes, confirmations,
settings status, and identity status. Shared variants live in
`surfaces/pico/src/fixtures/named-states.ts` and reuse `fixture-host.ts`.
Identity backup and key examples are deliberately inert strings.

Four states intentionally return null:

| Part | State |
|---|---|
| CartShelf | `EmptyLibrary` |
| GameActions | `NoActions` |
| IdentityDialog | `Closed` |
| ResumeList | `NoResumableGames` |

Caliper reports these as `Empty`, with a visible warning. The test asserts
absence rather than adding explanatory markup to hide the actual behavior.
CartGrid's empty collection renders an empty list, so its verdict is `Rendered`.

## Verification

| Check | Result |
|---|---|
| Initial named-state gate | 50 failures and two fixed-ornament passes before implementation. |
| Pico checks | `nix run .#pico-check`: 560 passing tests, zero failures, clean TypeScript. |
| Exception guard | A deliberately stale null-state name fails the exception-discovery test. Restoring the name passes. |
| Source review | All default export bodies are unchanged. The only added layout is a fixture container. |
| Original defaults | 104 captures, 52 parts on both devices, with no render failures or console errors. |
| Final named-state sweep | 378 captures at 640×480 and 1920×1080. 370 `Rendered`, eight expected `Empty`, zero `Failed`, zero console errors. |
| Caliper browser gate | All checks pass, including 12 workspace sizes, reload on save, named-state selection, All states, isolated errors, and CLI verdicts. |
| Take overlay | A temporary take changes Badge.Warning text and its component CSS. The original frame and source files stay unchanged. The take is discarded. |
| Scroll reachability | Playwright trial clicks reach all 176 enabled controls across eight selected cases on both devices. No action is invoked. |

The screenshot review found two isolated-example defects. CoverArt's missing
artwork lacked its documented size properties. Cart's hero lift started above
the frame. `fixtures/PicoPartFrame.tsx` supplies those container constraints
without changing either component. Both examples fit at 640×480, 1920×1080,
320×240, 480×800, and 1280×300. Captures were checked again after that change.

The final sweep reports 30 spills. These include the doubled Attract animation
rail, the LaunchStage cartridge behind its slot, and scrollable shelves, lists,
facts, dialogs, and cards. The selected control-reach checks cover Overlay
problems, RunnerPicker.Ready, GameDetail.LongTitle, GameHero.LongTitle,
IdentityDialog.BackupReady, PauseMenu.RetryableProblem, and
LibraryBrowser.RecentlyPlayed. A spill alone does not establish a defect;
absence of a spill also does not establish full visibility.

## Limits

The examples do not enumerate every combination of props. Controlled Library
examples are fixed snapshots; their callbacks do not update the query. Shelf
selection, identity form completion, and the unsupported settings text-entry
notice still require interaction with the real component. Tests exercise some
of those interactions without adding preview-only production props.

The reachability check uses browser auto-scrolling and pointer hit testing. It
does not prove hardware d-pad navigation. Full render determinism, physical
panel fidelity, host effects, and Reach analysis are outside this change.

## Reproduce

From Korri, run `nix run .#pico-check`. From `surfaces/pico`, start
`bun run caliper`. With Chromium and the linked Caliper CLI available:

```sh
caliper-render --url http://localhost:5173 --list
caliper-render --url http://localhost:5173 \
  --part src/pages/PicoHome.page.part.tsx --state '*' --device '*'
```

Set `CHROMIUM` to the Chromium executable. Caliper's
`scripts/verify-browser.mjs` accepts `--url`, `--root`, and `--out` for the
workspace checks.

Local evidence from this run is under `/tmp/pico-named-states/`: `baseline/`,
`states/sheets/`, `final/`, `focused/report.json`, and `browser-gates/`.
These are temporary artifacts, not checked-in golden images. The live-preview
verification after merge is recorded in the final change summary.
