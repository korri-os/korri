---
date: 2026-11-24
topic: moonlight-removal
artifact: scope
status: removed and published; product pin and ibuki cutover in progress
---

# Dropping Moonlight Embedded: what goes, what stays

The owner's decision is ADR 0099: remove the client now, accepting the loss of
streaming on `tabs6` and `op6` until the Rust viewer gains V4L2 M2M decoding.
The Android, webOS and Artemis removal briefs are the precedents.

`moonlight-final` marks the last Korri commit carrying the client integration.
The corresponding plugins baseline is
`e3343461a76eff0aa0e8ef7b8a8761d3c359918a`. Restore missing behaviour deliberately
from those revisions; do not restore the client as a fallback.

## What goes

| Repository/path | Removal |
| --- | --- |
| Korri `services/korrid/src/linux_viewer/selection.rs` | Moonlight fallback and dual-client launch routing |
| Korri `linux_viewer/production.rs` | Moonlight enum arm, non-socket start/status/snapshot adapter and unit attribution |
| Korri `services/korrid/viewer-vm-test.nix` | Moonlight package and fallback cases; missing/mismatched viewer cases now fail |
| Korri `nix/product/plugin-selection.nix`, `image-plugins-check.nix` | Moonlight from all ten ARM image defaults and its positive inclusion assertion |
| Korri `nix/stream-cadence` | Moonlight-only `run` command and LD_PRELOAD client shim; `view` uses the Rust viewer |
| Korri `clients/viewer/default.nix` | `bin/moonlight` compatibility aliases; measurement candidates use `bin/korri-viewer` |
| Plugins `plugins/moonlight/`, `services/moonlight/` | Client declaration, native recipe, 14 vendored patch files and client-only checks |
| Plugins Nix composition/publication | Client outputs and their gates; catalog batches no longer select them |

Viewer settings move intact from `plugins/moonlight/settings.ts` to
`plugins/viewer/settings.ts`, with the existing viewer runner identity. Their
schema, revision and native arguments are unchanged. The settings tests move
with them. No persisted settings are copied or deleted.

## What stays on purpose

| Part | Why |
| --- | --- |
| Moonlight protocol, Sunshine, pairing, host UUID, port 47989 | Rust viewer and Android app consumers |
| `app.moonlight.certificate.*`, certificate credential/env and #476/#502/#510 rules | Authentication, endpoint binding and server certificate pinning are not the removed client |
| `.config/moonlight` and controller mapping location | Existing protocol identity and Rust viewer artifacts; no runtime migration |
| Rust viewer's carried C code, network core alternative and upstream source pin | Live consumers and GPL provenance, not the Embedded program |
| Old ADRs, briefs, proofs, baselines and acceptance evidence | Historical measurements; Moonlight-only reports are not Rust acceptance |
| Existing releases, installed receipts and user settings | No remote data deletion or implicit settings migration |

The carried input code still suppresses its quit chord when korrid owns the
session. Its switch is now `KORRI_VIEWER_DISABLE_GAMEPAD_QUIT`; the certificate
switch `KORRI_MOONLIGHT_SERVER_CERTIFICATE` stays unchanged.

## Published release and operational gate

The matching plugins release is `build-1b865cbd5461-no-moonlight`, built from
`1b865cbd546161464fb019ed3dc260c87102434a` against Core
`a27386cb0d2efcd7bf2e20dc909f30b19ae0fee0`. Dry run `45481298144` passed all
four jobs. Publication `45505023459` passed after two GitHub HTTP 504 failures;
normal retries verified and kept matching draft assets, without replacement.
Pages deployment `45508255945` passed on its second attempt after its uploaded
artifact became visible to the API.

The public immutable catalog and Pages catalog both have SHA-256
`f18201cc234adb2a0c59b0120ff64bad999b104bce2b5f0d0dd8add7f6c2f7a0`
(`sha256-8YIBzCNK2yoMWbASD/ZLrZmbEEvOK18NDdit1/bC96A=`). It contains
34 plugin IDs and no `@korri:moonlight`; every other previous ID remains.
The product pin uses these exact bytes. The Tailscale publisher output was
renamed to `korri-plugin-tailscale`, without an alias; its plugin identity
and store paths are unchanged.

Do not deploy the removal to ibuki with its old viewer treaty. The owner lands
the product pin, then ibuki receives the matching signed viewer, bundle and
CLI. Its installed viewer's immutable old source requires the host's existing,
approval-checked explicit repository source switch; no receipt rewrite,
publisher trust change or signature bypass is used. The owner must approve
the digest shown by `korri plugins review` before that switch.

No successful ibuki stream is claimed by this pin preparation. No device
decoder is added by this removal.

## Preparation evidence

- Selection regression failed before the cut: an absent viewer returned
  `Moonlight` instead of an error. The focused Linux viewer tests now pass
  (35 tests). The real `viewer_probe` command with no enabled viewer returns
  the explicit install/enable error before starting a unit.
- `stream-cadence` passes 485 tests, Ruff and mypy. Its real `--help` lists
  only `view`, `verdict` and `compare`. The native viewer input/audio check
  passes, including quit ownership and no input discovery in view-only mode.
- The moved viewer settings pass 9 tests (94 assertions) and strict TypeScript.
- The requested full `korrid-check` ran once. Its library lane reports
  2,068 passed, 5 failed and 25 ignored; all five failures are in unchanged
  peer-save/host-save tests. Four pass in isolation; the nonce/deadline test
  still fails alone. The command stops there, before later integration and
  smoke lanes. Strict clippy also reports errors in unchanged acquisition and
  surface code. These gates are not green.
- The image check cannot materialize a path in the existing published catalog:
  `/nix/store/zs5zqcfblb8mbz34r8z1saqkz2szmvc1-korri-plugin` has no available
  substituter. This does not establish image-build acceptance.
- Read-only ibuki checks find both plugins enabled. Its Rust viewer is release
  `build-a7448c0a8ea2-viewer-rust-audio`; its treaty is
  `e9178296902fad5842fdd2d5eac1d7111ba14851a2c0e1d98a87b07df3fd8565`.
  The current treaty is
  `834af1f4a4f1f2beecf6a75bbe2d1ef69933a5c34d62ccca7b1cb1014c9c9cc9`.
  No ibuki deployment or successful stream is claimed yet.
