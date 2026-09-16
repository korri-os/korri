---
date: 2026-09-16
topic: android-removal
artifact: scope
status: measured against main 73442582; not a decision to proceed
---

# Dropping Android: what deletes, what stays, what needs a call

Measured against `main` at `73442582`. Every number below came from reading the
tree, not from memory. This document scopes the deletion; it does not authorise
it.

## Why this is worth measuring

korrid carries two launch stacks. Only one of them has vendor names in it.

| | Android stack | Linux stack |
|---|---|---|
| Entry | `launcher::local_games` / `launch_game` | `game_routes::list` / `selected_launch` |
| Dispatch | closed `match` on `@korri:android-app` and `@korri:retroarch` | `registry.native_runner(&route.runner_id)` |
| Session owner | `AndroidActiveLaunch` + intents | `host/` — systemd units, compositor focus, input seats |
| Vendor names | RetroArch, Moonlight, mGBA, hardcoded | none |

The three "remove plugin names from korrid" scopes were all describing the
left-hand column. Deleting it removes most of the work rather than doing it.

## The portal is not orphaned

`clients/linux/` already ships `korri-portal-shell`: it starts Chromium and
supplies the `window.KorriRpc` bridge before the portal loads, implementing
`contracts/bridge/korri-rpc-bridge.ts`. So `contracts/bridge/` **survives** —
it is the treaty the Linux shell honours too. Only the Kotlin implementation
of it goes.

## Whole-tree deletions

| Path | Size | Note |
|---|---|---|
| `clients/android/` | 15,680 files, 922 MB | Kotlin shell, Artemis streaming core, native pairing, WebView host |
| `plugins/mgba/android/plugin.ts` | 1 file | the Android mGBA declaration |
| `plugins/retroarch/android/` | `devshell.nix`, `sdk.nix` | Android SDK/NDK plumbing for the RetroArch build |

## korrid file deletions

2,317 lines across five files, all Android-only by purpose:

| File | Lines |
|---|---|
| `src/launcher/retroarch_control.rs` | 792 |
| `src/android.rs` | 597 |
| `src/launcher/retroarch.rs` | 513 |
| `src/launcher/android_app.rs` | 208 |
| `src/android_app_route_tests.rs` | 207 |

Integration tests that go with them: `tests/retroarch_plugin_route.rs`,
`tests/retroarch_session_actions.rs`, and `tests/session_actions.rs` pending a
check of whether it covers the Linux `host/` session too.

`73442582`, the RetroArch-family fix landed earlier today, deletes with
`retroarch.rs` and `retroarch_control.rs`.

## korrid surgery, by weight

These files stay; Android has to be cut out of them.

| File | Lines | `android` hits | `moonlight` hits |
|---|---|---|---|
| `src/lib.rs` | 9,466 | 203 | 411 |
| `src/plugin.rs` | 1,409 | 32 | 127 |
| `src/launcher/types.rs` | 1,123 | 13 | 62 |
| `src/launcher/mod.rs` | 748 | 36 | 3 |
| `src/config/resolver.rs` | 420 | 22 | 5 |
| `src/plugin_policy.rs` | 147 | 6 | 4 |
| `src/portal_access.rs` | 463 | 2 | 6 |

The named seams inside them:

- `NativePlatform` loses `EmbeddedAndroid` and becomes a single value, so every
  branch on it collapses. This is the cleanest cut line in the codebase — find
  it first and most of `lib.rs` follows.
- `launcher/mod.rs` loses `local_games`, `local_games_with_cover_assets`,
  `launch_game`, `local_launch_context` and the two-arm family `match`.
- `plugin.rs` loses `SessionControlEffect` (25 variants), `SessionControlExecutor`
  (2 variants, both Android), `SessionControlPlatform` (1 variant, `Android`),
  and `SessionControlIntegration` (2 variants).
- `plugin_policy.rs` loses `RegistrySource::Android`.
- `portal_access.rs` loses `allow_bundled_android_origin`.
- `Cargo.toml` loses `cdylib` from `crate-type` and the `jni` dependency.

## Session controls disappear rather than move

`SessionControlPlatform` has exactly one variant, `Android`. Both
`SessionControlExecutor` variants — `AndroidMoonlight` and `RetroarchControl` —
are Android. There is no Linux in-game control path: `host/` owns session
lifecycle (start, stop, freeze, focus, input seats), not in-game menus.

So the closed effect vocabulary is not replaced by `session.describe` and
`session.control`. It is deleted. When Linux wants in-game controls, that is a
fresh design against a Linux transport, and the plugin operations can be built
then against a real case — which is what the repository guard asks for anyway.

Open sub-question: plugin declarations still carry a `sessionControls` block,
and the eight generated cores each declare two controls. With no consumer, the
field either stays as inert declaration or is removed from the contract. That is
a contract change, so it needs a decision rather than a default.

## Moonlight splits in two

**Client side — already dead on Linux.** `resolve_moonlight_outcome` returns
`MoonlightUnavailable("Artemis is unavailable on this platform")` whenever
`NativePlatform` is `Standalone`. So `app.moonlight.resolve`,
`app.moonlight.launch.prepare` and `app.moonlight.launch.cancel`, plus all 23
Moonlight `SessionControlEffect` variants and `AndroidMoonlightEffect`, are
unreachable on Linux today. They delete.

**Host side — live and in active development.** `host/moonlight_certificate.rs`
(1,244 lines) brokers client certificates to Sunshine over a unix seqpacket
socket. Recent `main` commits (`fix(inputd)`, `perf(sunshine)`) are Linux
Sunshine work. A Linux korrid serving streams is real.

The three certificate RPCs sit between the two halves, and that is the decision
below.

## Decisions needed before any code moves

**1. Do the three `app.moonlight.certificate.*` RPCs survive?**
They exist so a client can pair with a Korri Sunshine host. I found no non-Korri
caller, but I did not read the 1,087 Artemis-referencing files in
`clients/android/` to prove none exists. If Korri's own Android client was the
only caller, then `host/moonlight_certificate.rs`, the Moonlight blocks in
`upstreams.rs` (81 hits) and `upstream_native.rs` (72 hits), and those three
RPCs all delete too — and Moonlight leaves korrid almost entirely. If a
third-party Moonlight client is meant to pair with Korri's Sunshine, all of it
stays and only the client half goes.

**2. Does `sessionControls` stay in the plugin declaration contract?**
See above. Inert field, or contract change.

**3. What happens to the eight-core RetroArch catalogue?**
The cores are Linux packages and route through the clean Linux stack, so they
survive untouched. But `plugins/retroarch/` also ships the Android SDK/NDK
plumbing and a patched Android RetroArch build. Confirm the Linux RetroArch
build does not depend on that tree before deleting it.

**4. Federation.** `relay.rs` advertises a `moonlight_address`, and
`federation/routing_tests.rs` has 30 Moonlight references. Whether a peer
korrid still advertises a Moonlight-compatible stream endpoint depends on
decision 1.

## Suggested order

1. Settle decision 1. It is the difference between deleting ~1,400 lines of
   Moonlight host code and keeping it.
2. Collapse `NativePlatform` to one value. Let the compiler find the branches.
   This alone removes most of the `lib.rs` surgery.
3. Delete the five Android-only korrid files and their tests.
4. Delete `SessionControlEffect` and its three companion enums, then settle
   decision 2 for the declaration contract.
5. Delete `clients/android/` and the Android Nix targets:
   `android-apk`, `android-apk-dev`, `android-jvm-check`,
   `android-bridge-contract-check`, `android-app-route-check`,
   `android-federation-acceptance-check`, `android-game-discovery-check`,
   `android-sdk-env`, `android-tools`, and the NDK toolchain composition.
6. Drop `cdylib` and `jni` from `services/korrid/Cargo.toml`.
7. Re-run the matrix. `android-jvm-check` is expected to disappear, not pass.

## What remains as genuine vendor-name work afterwards

Only SteamGridDB: 18 files including `enrichment/steamgriddb.rs`, its tests and
its asset downloader, plus the two `system.settings.steamgriddbCredential.*`
RPCs. It is platform-neutral, so Android removal does not touch it. It still
needs the unbuilt provider operation group (`provider.validate`, `claims.*`) and
a write-only secret seam, because `settings.describe` is a runner-scoped
launch-time schema and cannot carry a background provider's credential.
