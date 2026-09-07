# Minimum config layer

This implements the first slice from `docs/briefs/2026-09-06-config-cascade-discussion.md`.
It does not implement users, config folding, cards, catalog merging, or `host.toml` integration.

## Three fixed documents

The readable root contains exactly these inputs:

| File | Facts |
| --- | --- |
| `device.yaml` | Host, storage, plugin configuration, existing device declarations, and `locations` |
| `catalog/games.yaml` | ULID-keyed games with a title and an ordered list of release keys |
| `catalog/releases.yaml` | Release-keyed content records with the owning game, system, and optional identity derivation |

Discovery mints one game ULID when a release has no game. It writes a `sha256:<hex64>` release key and `identity: file`.
An unchanged rescan preserves game IDs and document bytes. Multiple copies of the same release add locations, not games.

Locations retain legacy target fields without `kind`. Discovery writes `storage`, `path`, and `discovery.first-seen-at`.
The release has no embedded target, launcher choice, or runtime choice.

The decoder also accepts the decided provider-reference release keys, such as `@korri:android-app/com.playdigious.tmnt`.
Existing Android app fixtures use the corresponding legacy provider/ref location without `kind`.
This preserves app routing without inventing a content hash.

The decoder checks both directions of every game/release link. It rejects duplicate keys, malformed IDs, unknown fields, and explicit nulls.
Catalog releases cannot contain launch opinions or inline hook commands. Unsupported legacy device behavior still receives a diagnostic.
The runtime does not read or rewrite the old `config.yaml` and `library.yaml` paths.

## Route selection

A catalog game can launch when a release has a usable location and an available launcher/runtime combination.
Resolution follows the ordered game release list and each release's location list.
The first successful route wins. Adding another complete release does not make the game unavailable.
Reordering these lists changes the deterministic choice; this is not a personal preference fold.
Declaration collisions remain errors, including collisions found on later releases.

Launchers declare supported systems. System-agnostic launchers also obtain supported systems from enabled runtimes that name their app.
This retains RetroArch's existing split between the app and its cores.
Platform capability filters candidates before ambiguity checks. Multiple compatible launcher or runtime candidates remain an error.
There is no new priority or fallback configuration.

Filesystem resolution checks the actual readable root, including implicit `roms` storage.
A missing first copy does not hide a usable later copy. Launch materialization checks the file again.

Linux catalog publication omits unavailable individual games and reports their failures without disabling healthy games.
Invalid configuration, authorization failures, and declaration collisions remain blocking failures.
Static `host.toml` IDs still cannot collide with any declared catalog game, including an unlocated one.

## Ownership and recovery

Removing a selected storage location removes only discovery-owned locations and unreferenced discovery-owned storage records.
It retains catalog facts and game IDs, so reattaching content restores the same game.
Authored edits do not become discovery-owned merely because discovery encounters the file again.
Canonical-path checks protect authored files reached through another storage ID or overlapping root.
Reconciliation and ownership reconciliation index locations rather than scanning all locations for each candidate.

Three readable files cannot share one atomic filesystem rename. Discovery therefore records expected and candidate bytes in its private repair journal first.
It publishes games, then releases, then device locations through conflict-checked atomic file replacements.
An incomplete catalog fails link validation, and an existing snapshot coordinator retains its last known good generation.
Recovery proceeds only while each document contains either expected or candidate bytes.
It verifies all final candidate bytes before accepting ownership changes and clearing the journal.
An external edit produces a conflict rather than an overwrite.

Settings saves share the discovery write lock and reject a pending publication before changing `device.yaml`.
The user can retry after discovery recovery finishes. Settings never infers the private state path.

The snapshot initializer's pre-existing exists-then-write race is separate follow-up `01M1YA4WKXR80WDZ5Y04QWYN8B`.
Fixing it requires exclusive creation at the proseQL storage interface.

## One-off device cutover

Do not deploy the binary alone onto an old readable root and expect an automatic migration.
The old files are deliberately ignored.

1. Stop the owning daemon and preserve readable files and private state before changing device data.
2. Verify current file contents instead of relying on earlier observations in the discussion brief.
3. On Bandai, preserve `config.yaml` as `device.yaml`, remove the old `library.yaml` after backup, and rescan.
4. Expect new ULID game IDs and reset play statistics for the old slug IDs.
5. On Zao, inspect the current deployment first. Checked-in deployment fixtures now contain Wario Land 4, unlike the earlier empty-file observation.
6. Apply the intended one-off cleanup of empty old files and the dead legacy file only after verifying those exact contents.
7. Keep `host.toml` unchanged. Its integration into YAML is a separate slice.
8. Verify discovery, catalog publication, and launch on the actual device before accepting the cutover.

Deployment and device-acceptance scripts now provision, compare, and restore all three documents.
They preserve original absence and retain recovery copies when restoration fails.
Zao refuses partial or externally edited new-format configurations and unsafe catalog parent paths before service mutation.
No device migration was performed while implementing this slice.

## Verification entry points

- `nix run .#korrid-test` covers decoding, snapshot retention, discovery, recovery, settings, and route consumers.
- `services/korrid/tests/minimum_config.rs` exercises discovery through real files and route resolution, including stable ULIDs on rescan.
- `services/korrid/tests/minimum_config_regression.rs` covers copies, removal, authored aliases, multiple releases, platform selection, and a 10,000-file catalog.
- `services/korrid/src/discovery/reconcile/documents/tests.rs` interrupts production publication at each boundary and checks restart recovery.
- `services/korrid/test-checkpoint-documents.py` and `deploy/test-zao-documents.py` verify checkpoint restoration and deployment safety without device writes.
- `nix run .#korrid-check` adds contract generation, local RPC smoke, portal/Shift checks, and Android packaging.

The host-only RPC smoke uses the provider app fixture, so it needs no private ROM dump.
It isolates private runtime state under its temporary readable root and does not claim to test an installed Android app.

## Verified on 2026-09-07

- `nix run .#korrid-check` passed end to end, including local RPC smoke, contract generation, portal/Shift checks, Android JVM tests, and debug APK packaging.
- The final Rust run passed 520 tests. The new same-release copy, multi-release selection, and interrupted-publication regressions passed.
- Separate portal, Shift, and Pico tests and typechecks passed with portal-owned React resolution.
- `cargo clippy --all-targets` in the korrid development shell passed with existing warnings. Rust formatting, scoped ShellCheck, and `git diff --check` passed.
- Review fixes cover usable-copy selection, platform filtering, non-blocking unavailable games, authored aliases, settings/publication coordination, and deployment rollback.

The full check also exposed two existing runner defects: inherited shell banners in JSON stdout and missing Pico React setup.
Both were corrected in the check script. A separate test-only commit updates the stale Android bridge teardown assertion from the earlier shared-RPC change.
No hardware acceptance, device migration, or remote publication was performed.
