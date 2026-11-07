# render (throwaway)

Native `caliper-render` loop for Korri's existing `*.preview.slint` parts.
Builds only on the x86_64 workstation. No device build or deployment.

```sh
nix build .#slint-caliper-render
nix run .#slint-caliper-render -- \
  --part clients/shell/packages/pico/ui/atoms/PicoButton.atom.preview.slint \
  --state all --device rg35xxsp --device odin2portal --check \
  --out /tmp/slint-render --baselines "$PWD/.caliper/baselines"

# Every Pico and Zone part, all exported states, two devices:
nix run .#slint-caliper-render -- --part '*' --state all \
  --device rg35xxsp,odin2portal --check --out /tmp/slint-render-all

# Inspect BOTH PNGs before approving saved evidence. Never rerenders.
nix run .#slint-caliper-render -- --approve /tmp/slint-render/check-RUN/report.json \
  --baselines "$PWD/.caliper/baselines"

# Copy the built closure to another x86_64 machine (parent chooses target):
nix copy --to ssh-ng://TARGET "$(nix build .#slint-caliper-render --no-link --print-out-paths)"
```

`--list --part DIR` discovers exported states. `--state '*'` also means all.
Without a state it uses `Default` if exported, otherwise the first export.
`--device` repeats or accepts comma-separated ids; `'*'` selects all five.
One JSON object goes to stdout; elapsed seconds and count go to stderr.
`--check` returns 0 when the report was written, **not** when findings pass.
Without it, Empty/Failed frames return 1. Invalid requests return 2.

## Grounding (not a new schema)

- Caliper `README.md`, `docs/guide.md` “Let an agent see a part”, “Report
  automatic checks”, “Approve reviewed images”, `bin/caliper-render.mjs`,
  `src/render/{checks,check-contract,worker-contract,accessibility}.js`,
  `src/checks/output.js` and `~/.agents/skills/caliper-render/SKILL.md` define
  the CLI flags, JSON fields, statuses and review semantics used here.
- Output is `{ results }`, or `{ results, report, reportPath }` with checks.
  The report uses Caliper's existing **v1** visual-report form. Every check
  has `name`, `status`, `detail`. PNGs sit in unique `check-RUN/first/` and
  `repeat/` folders; `report.json` retains both image paths and SHA-256s.
- Baseline directory `.caliper/baselines`, version-1 records, keys
  `sha256(JSON.stringify([project, part, state, device]))`, and content-addressed
  PNG names come from Caliper `src/render/checks.js`. Project is `korri`.
  Approval validates all images before modifying records, rejects changed
  images, duplicate identities, broken/unstable samples and takes, and updates
  records atomically one at a time. It never hides other findings.
- Korri `examples/surface-probe.rs previews` defines state enumeration,
  the uniform RGB565-buffer empty rule, software renderer, and 500 ms settle.
  `src/backend/headless.rs` supplies manual time and frozen `animation-tick`.
  `src/targets.rs` supplies semantic focus and target visibility. `src/verify.rs`
  establishes that focused-entry visibility is an independent requirement.
- Existing `preview/scenarios.ts` (Pico and Zone) and Zone's saved
  `preview/scenarios/*.json` ground the real-view proof, not a new fixture
  record shape. They are **not** executed in the part sweep.

| Device | Logical pixels, scale 1 | Existing source |
|---|---|---|
| rg35xxsp | 640 × 480 | `docs/acceptance/stream-cadence-rg35xxsp-2026-10-23.md`, verified active mode |
| rg35xxpro | 640 × 480 | `docs/acceptance/h700-moonlight-v4l2request-2026-10-27.md`, both fullscreen viewers and Pro capture |
| rpminiv2 | 1240 × 1080 | `nix/devices/rpminiv2/hardware.nix`, rotated 1080 × 1240 native panel |
| rg353m | 640 × 480 | Caliper `src/client/device-frame.js` HANDHELD_DEVICES |
| odin2portal | 1920 × 1080 | Caliper `src/client/device-frame.js` HANDHELD_DEVICES |

No requested device size remains unresolved. Physical size and color fidelity
are not measured by this CLI.

## Checks and deliberate differences

| Affordance | Implementation / limit |
|---|---|
| Compile / draw | Slint compiler diagnostics in `problems`, Failed on compile/create/bind errors; uniform draw is Empty + Review, as in surface-probe. A compile failure has no PNG (`png: ""`), rather than a fabricated browser error screenshot. |
| Spill | Every valid, non-hidden ElementHandle with positive size, not only accessible controls. Reports actual boxes outside the requested window, with 0.5 px rounding tolerance. Clipped/scrolling content is still reported, just as Caliper requires human review. No text-ink overflow or shadow-ink measurement. |
| Accessibility | Every non-hidden selectable element must have a non-None role and nonempty label. At most one selected element may exist, including hidden selections. No findings is Review, never an axe pass. |
| Focus | Selected targets must be drawn and visible using `targets::elements` plus actual viewport intersection. **Preview exports carry nested literal props, not JSON view entries**, so a focused entry selecting no element cannot be detected. `focus: Inconclusive` states this. With `--view FILE`, Binder plus `targets::drawn` checks the actual entries and catches absent/hidden/clipped targets. |
| Baseline | Pixel comparison, dimension/frame comparison and digest validation against an approved PNG. Missing/changed images are Review, unreadable/foreign-environment records are Inconclusive. |
| Determinism | Two fresh component instances, not fresh browser contexts/processes. Fixed animation tick; normal property animations settle for 500 ms. Shared compiler/global state and longer animations are limits. |
| Browser | NotRun: no native equivalent of browser console/network errors. Native compile/bind/draw errors are reported separately. Approval permits this explicit native NotRun, unlike Caliper's browser-Passed requirement. |

The native `focus` check extends Caliper's check-name enum. Consumers that
validate that enum need one addition. Existing frame, viewport, problems,
console, spill, baseline and check-status shapes remain. There is no `--url`,
Chromium, take overlay, expectations/authored-check execution, source revision
monitor, interruption report, or source-output directory restriction in this
spike. Output must be kept outside source. Baseline environment identifies
Slint, platform, RGB565 and this spike's check version, not Chromium/axe.
Use Nix for the pinned fontconfig/DejaVu environment. Imported package fonts
still load from the part's own sources.

**All 67 axe rules that Caliper enables remain a gap.** `axe-gaps.txt` names
all of them and their descriptions, extracted from the installed axe-core
4.13.0 with the exact WCAG tag selection and disabled document rules in
Caliper's `src/render/accessibility.js`. This includes contrast, text
alternatives, relationships, names beyond selectable targets, keyboard and
focus semantics, language, text scaling/spacing, target size and media rules. The two Slint
rules are not substitutes for any full axe audit.

Production path for full preview focus coverage: **run scenarios to get the
real views**, then use the shell's target matching. The spike does not parse
Slint literals into a competing view model.

## Focused proof

`tests/smoke.py` runs the actual CLI on temporary Slint sources. It proves
compile errors, empty draw, spill, selected visibility, missing role/label,
duplicate selection, baseline approval/match/change, tampered-image refusal
and invalid requests. Only synthetic images are auto-approved in that test.
No product images are approved by the acceptance sweep.

A plain Slint Window does not clip its children in ElementHandle::is_visible().
The smoke test exposed this: a fully off-screen selected rectangle was reported
visible by that API. This spike keeps the shell ancestor-clip rules and adds an
explicit viewport intersection. It does not change the shell. Slint itself
rejects selectable/label properties with an omitted accessible role; this is
reported as a compile failure before the tree audit.

`examples/view.rs` saves a real initial view using the shell's QuickJS engine:

```sh
# Run cargo through the repository's Nix toolchain, e.g. nix shell --inputs-from .
# nixpkgs#cargo nixpkgs#rustc nixpkgs#gcc -c bash, then:
cargo run --release --manifest-path spikes/slint-caliper/render/Cargo.toml \
  --example view -- clients/shell/packages/zone \
  clients/shell/packages/zone/preview/scenarios/games.json /tmp/zone-view.json
nix run .#slint-caliper-render -- --part clients/shell/packages/zone/main.slint \
  --state Surface --view /tmp/zone-view.json --device rg35xxsp --check --out /tmp/slint-view
```

See `EVIDENCE.md` for the actual run, verdict totals, output paths and shell
full-suite result. No `clients/shell/src`, package, treaty or service changes
are needed; no additive `pub` changes were made. `flake.nix` adds one package
entry; all build logic stays here.
