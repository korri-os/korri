# Android application plugin schema checkpoint

The device gate now reads the minimum-config fixture:

| File | Owner and content |
| --- | --- |
| `device.yaml` | Device title and the installed package location |
| `catalog/games.yaml` | TMNT title, game ULID, and release reference |
| `catalog/releases.yaml` | Provider release identity and Android system |
| `android-app.plugin.ts` | Retired record of the declaration-only provider, system, and launcher contribution |

The game id is `01K4J6K8Y00000000000000001`. Its release is
`@korri:android-app/com.playdigious.tmnt`. The package remains
`com.playdigious.tmnt`. The identity split follows the approved minimum layer in
`docs/briefs/2026-09-06-config-cascade-discussion.md`.

These are device-gate inputs, not fresh-install defaults. The bundled plugin
source remains `services/korrid/plugins/android-app.plugin.ts`.

The checkpoint copy of the plugin is now a frozen record of the retired
launcher/runtime contract. The runner cut removed that contract, so the two
copies are deliberately different and the earlier byte-parity requirement no
longer applies. `services/korrid/tests/plugin_policy.rs` asserts that the
retired shape fails to load; nothing reads the checkpoint copy at runtime.
The three fixture documents are still live gate inputs.

## Current route boundary

The catalog owns game and release facts. `device.yaml: locations` owns the
provider and package reference. The enabled plugin supplies the runner.
The Rust route mapper selects the `@korri:android-app` integration and emits
runner `android-app`, the package name, an empty Activity class, extras `{}`,
directories `[]`, and files `[]`. The existing signer adds integrity. Android
owns the installed-package and Activity checks.

`command: android-app` is an integration token, not a generic executable.
Disabling the plugin must remove the route's runner contribution.

The dedicated `services/korrid/android-app-route-check.sh` gate normally uses
the two-game RetroArch checkpoint. To select this one-game checkpoint, supply
all three `KORRI_ANDROID_APP_ROUTE_CHECKPOINT_DEVICE`,
`KORRI_ANDROID_APP_ROUTE_CHECKPOINT_GAMES`, and
`KORRI_ANDROID_APP_ROUTE_CHECKPOINT_RELEASES` paths. Partial overrides fail.
The gate verifies all three files and restores their prior bytes or absence.
Failed restoration retains the backup and device lock for manual recovery.
The general Android smoke does not provision these documents.

## Validation

Current production snapshot and route probes:

```sh
services/korrid/config-snapshot-review.sh
services/korrid/plugin-route-review.sh
```

Offline script and fixture checks:

```sh
services/korrid/android-device-script-review.sh
services/korrid/test-minimum-config-fixtures.py
services/korrid/test-checkpoint-documents.py
```

These commands do not prove an installed package or a foreground launch.
The explicit-device gate owns that proof and changes the selected test device.

## Historical legacy proof

The original checkpoint passed the unchanged strict legacy schema and readable
cascade resolver at legacy revision `0e4cec9d`, against main baseline
`c58733d4`. It proved that plugin-contributed Android application routing did
not need a legacy schema extension. The current three-document fixture is the
later approved minimum-config cut, not the original legacy input.

That harness (`validate.sh` and `validate-legacy.ts`) was removed by the runner
cut. It replayed the legacy `launchers` registry shape and asserted byte parity
with the production plugin; neither holds now. Read it at its own revision if
the historical result matters. It was never a minimum-config acceptance command.

The historical exercise also found that legacy's implicit own-provider payload
failed its later strict `ProviderRecord` decoder silently. The explicit provider
in this plugin avoids that mismatch. Production plugin decoding must reject
malformed contributions instead of reproducing the silent drop.
