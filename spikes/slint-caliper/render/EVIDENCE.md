# Verified render spike evidence

Branch: `c-bfe1327d24b2b260310cecf3b7689c3ba4c94e3e1d2aaeac78caa2c3a994abf4`.
Code commit: `963a5d8a9`. Workstation: zao, x86_64 Linux. No device builds,
source-package edits, additive `pub` changes, services or treaty changes.
`flake.nix` has one additive package entry. All implementation lives here.

## Build and real command

```sh
nix build .#slint-caliper-render --builders '' --max-jobs 1 --cores 4
nix run .#slint-caliper-render -- --part '*' --state all \
  --device rg35xxsp,odin2portal --check --out /tmp/slint-caliper-render-all
```

Verified output closure:
`/nix/store/ka24ff4s4fkqzw1bjh81jgn4815lmvli-slint-caliper-render-0.0.0`.
This can be copied with `nix copy --to ssh-ng://TARGET STORE_PATH` and run on
another x86_64 machine without a compiler. Copying to another machine was not
part of this workstation-only run.

**155 parts, 270 exported states, 540 device/state results, 1,080 PNGs.**
Every frame is Rendered. Both samples match for every result. CLI elapsed
**86.754 seconds**, excluding the Nix build. PNG headers were checked: all
1,080 dimensions match the requested viewport. [PARTS.md](PARTS.md) lists every
part and its exported states; the saved JSON has every per-state/device verdict.

| Package | Device | Results | Rendered | spill Review | accessibility Failed | focus Failed |
|---|---|---:|---:|---:|---:|---:|
| Pico (96 parts) | rg35xxsp | 138 | 138 | 5 | 0 | 0 |
| Pico (96 parts) | odin2portal | 138 | 138 | 2 | 0 | 1 |
| Zone (59 parts) | rg35xxsp | 132 | 132 | 42 | 12 | 5 |
| Zone (59 parts) | odin2portal | 132 | 132 | 37 | 12 | 0 |

| Check | Observed statuses (540 results) |
|---|---|
| render | 540 Passed |
| spill | 454 Passed, 86 Review |
| accessibility | 516 Review (incomplete axe coverage), 24 Failed |
| focus | 534 Inconclusive (no JSON entries), 6 Failed (selected visibility) |
| determinism | 540 Passed (sample only) |
| baseline | 540 NotRun (no product baselines approved) |
| browser | 540 NotRun (native Slint) |

The 24 accessibility failures are 12 Zone fixture states at both sizes.
Examples: ZoneGames Modern selects two elements and has an unlabelled target;
ZoneOptions Keyboard selects three; ZoneCardBand OneCard has four unlabelled
selectable slots. These are observations about the existing preview states,
not a claim that every finding is a product defect.

The six selected-visibility failures are PicoMenu Short at odin2portal, plus
ZoneOptions Default and Keyboard, ZoneActionRow Wraps, ZoneRowList Options,
and ZoneSkyBand Head at rg35xxsp. No package was changed to remove findings.
Spill may be intentional decoration or scrolling. It needs product review.

## Saved paths

- Stdout verdict: `/tmp/slint-caliper-render-all.json`.
- Timing: `/tmp/slint-caliper-render-all-runtime.log`.
- Report: `/tmp/slint-caliper-render-all/check-1794016968831843113-570445/report.json`.
- Report SHA-256: `6d8fc7fdfa4d38103c417ed704228bc5b3ff480070f9833010acd233696fa8d3`.
- PNG example: `/tmp/slint-caliper-render-all/check-1794016968831843113-570445/first/0-PicoGameDetail.page.preview.ConfirmRemoval.rg35xxsp.png`.
- PNG example: `/tmp/slint-caliper-render-all/check-1794016968831843113-570445/first/96-ZoneGames.page.preview.Find.odin2portal.png`.
- Each has a counterpart under `repeat/`.

Representative PicoButton, ZoneGames Find and real Zone view PNGs were opened
with the image reader. This is not a visual approval of all 1,080 images.
No product baseline was approved.

## Deliberate defects, restored

```sh
python3 spikes/slint-caliper/render/proof.py \
  /nix/store/ka24ff4s4fkqzw1bjh81jgn4815lmvli-slint-caliper-render-0.0.0/bin/slint-caliper-render \
  . /tmp/slint-caliper-render-nix-mutations
```

The script copies Pico to a temporary directory. It inserts a rectangle
reaching 35 px beyond the right edge into PicoButton Default, renders and
catches spill Review. It restores the source. It then inserts a selectable
button with no label, catches accessibility Failed, and restores again.
It checks the original file and both restored copies byte-for-byte. The
original packages were never modified.

Saved evidence: `/tmp/slint-caliper-render-nix-mutations/mutation-proof.json`,
`spill.json`, `missing-label.json`, and each report/PNG folder named there.

## Real JSON view and the focus gap

The helper uses the shell engine, effective preferences, and catalog window,
following surface-probe. It reads Zone's existing `preview/scenarios/games.json`
and saves the initial `view()`; it does not run scenario steps.

```sh
nix shell --inputs-from . nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc -c \
  cargo run --release --manifest-path spikes/slint-caliper/render/Cargo.toml \
  --example view -- clients/shell/packages/zone \
  clients/shell/packages/zone/preview/scenarios/games.json /tmp/slint-caliper-zone-view-ready.json
nix run .#slint-caliper-render -- --part clients/shell/packages/zone/main.slint \
  --state Surface --view /tmp/slint-caliper-zone-view-ready.json \
  --device rg35xxsp --check --out /tmp/slint-caliper-render-nix-view
```

Verified: one focused JSON entry; `targets::drawn` passes. Saved verdict:
`/tmp/slint-caliper-render-nix-view.json`. Report:
`/tmp/slint-caliper-render-nix-view/check-1794016989167274078-572829/report.json`.

A second input keeps `cardMarks[0].focused` but clears `cards`. The check fails:
`the focused entry at view.cardMarks[0] ("band") selects no element with
accessible-item-selectable in the window`. Saved verdict:
`/tmp/slint-caliper-render-nix-view-absent.json`; its report path is in that JSON.
This temporary input does not change a package.

For the normal preview sweep, entry-to-element coverage remains Inconclusive:
preview exports carry nested literal props, not JSON view entries. The
production path is **running scenarios to get the real views**.

## Affordances

| Affordance | Verdict | Evidence / cost |
|---|---|---|
| One command, JSON verdict, device/state PNGs | works | 540 results, 1,080 PNGs; ten more renders cover both PicoButton states on all five device sizes. |
| Compile errors and empty draw | works | Real-process smoke checks compile failure and uniform RGB565 Empty; no such failures in the product sweep. |
| Element spill | works | 86 Review findings plus the deliberate 35 px spill. Geometry only, not text/shadow ink. |
| Focused entry drawn and visible | partly works | Real Zone JSON view passes; removing its drawing fails. Previews only check selected visibility and report the entry-mapping gap. |
| Role/label and single selected element | works | Missing label injection fails; missing role is refused by Slint's compiler; duplicate selection test and existing Zone fixtures fail. Only the two native rules. |
| axe-equivalent audit | does not work | All 67 enabled axe rules are named as gaps in axe-gaps.txt. Clean native checks are Review, not an axe pass. |
| Baseline approval and pixel difference | works | Nix smoke approves saved synthetic evidence, matches it, detects changed pixels, rejects changed saved images and broken reports. No unreviewed product approval. |
| Caliper skill/output transfer | partly works | Same flags, result fields, statuses and v1 baseline form. No Vite/Chromium; native focus extends the check-name enum, browser is NotRun, native approval permits that NotRun. |
| Full Caliper check context | does not work | No authored checks, expectations, source monitor, take overlay, browser audit or fresh-process isolation. |

## Checks

- `cargo fmt --check`: passed through Nix.
- `cargo clippy --release --all-targets -- -D warnings`: passed through Nix.
- `cargo test --release`: passed, one real-process integration test covering
  the CLI paths above. Nix package check reran it successfully (1.15 s).
- `nixfmt --check flake.nix spikes/slint-caliper/render/package.nix`: passed.
- `python3 -m py_compile` for proof and smoke: passed.
- `git diff --check`: passed; pre-commit gitleaks passed (no bypass).
- Nix build and runtime: passed; paths and measured sweep above.
- Full shell suite invocation: **failed** in `nix build .#korri-shell`, whose
  check phase runs all release tests with `fixture`. Cargo reported **105 passed,
  1 failed** across 16 completed result groups, then stopped at
  `pico_package::calls_stay_well_inside_the_budget_on_a_slow_clock`.
  Later test targets did not run because the normal package check is fail-fast.
  No second full-suite run was made.

Logs: `/tmp/slint-caliper-render-focused.log`,
`/tmp/slint-caliper-render-nix-final.log`,
`/tmp/slint-caliper-render-full-shell-suite.log`.

The initial `nix run .#shell-check` was stopped during setup/compilation, before
running tests. The package build runs the full release suite with `fixture`
(`clients/shell/package.nix`) on this branch. It also builds the shipped dist
binary first, which adds a substantial LTO cost.

The failure is an instruction-budget assertion, not an inferred busy-machine
wall-clock failure. Seven medians exceed the test's limits, including grid menu
update 28.7 ms vs 26 ms, opening Find view 22.1 ms vs 20 ms, and left/right view
10.2/10.8 ms vs 10 ms. Test duration: 10.63 s. No limit was relaxed.

Verified: the starting main commit `1c152cece` and this spike resolve to the
**identical shell derivation**:
`/nix/store/3wmcqsrwbsb8l0ms0gwdi88d208hjnzc-korri-shell-0.0.0.drv`.
Both `nix eval ...#korri-shell.drvPath` outputs were compared byte-for-byte.
This establishes that the failed shell build is unchanged by the spike.
The shell suite remains a failed gate, not a claimed pass.

## Follow-ups (not changed here)

High: the baseline Pico instruction-budget failure blocks the shell Nix
package check. It needs investigation outside this spike. Later shell targets
remain unverified by this child run.

Medium: a plain Window does not clip children in ElementHandle::is_visible().
The off-screen synthetic target exposed this. The spike adds explicit viewport
intersection while keeping the shell's hidden/ancestor-clip rules. The shell
should get its own focused regression and fix; this spike changes no shell code.

Tooling emits its existing once-per-process missing-debug-info warning when an
item has no element metadata. Geometry/accessibility checks still run; unnamed
elements use run-local `element#N` targets. They are not stable source selectors.
