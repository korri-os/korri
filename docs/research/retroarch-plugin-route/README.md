# RetroArch plugin route checkpoint

These reviewed inputs use the approved minimum-config split:

| File | Content |
| --- | --- |
| `device.yaml` | Device title and release locations |
| `catalog/games.yaml` | TMNT and Wario Land 4 game identities and titles |
| `catalog/releases.yaml` | Provider/file release identities and systems |

Wario Land 4 uses game `01K4J6K8Y00000000000000002` and release
`sha256:d16c7bf6e62bb84049fff1b387108fbd1e6e2cd38ca994ab5310dd9cbf9ba414`.
Its device location remains `storage: roms`, `path: wl4.gba`.
TMNT uses game `01K4J6K8Y00000000000000001` and release
`@korri:android-app/com.playdigious.tmnt`.

The files do not select a launcher or runtime with launch opinions.
`@korri:mgba` declares GBA support and its `@korri:retroarch/retroarch` app.
`@korri:retroarch` supplies the Android package, Activity, and launcher.
Korrid resolves the device location and performs the configuration and launch
effects. The identity split is grounded in
`docs/briefs/2026-09-06-config-cascade-discussion.md`.

## Device gates

The Android app route gate uses both games. RetroArch and overlay acceptance
need a one-item catalog. `services/korrid/prepare-wario-checkpoint.py` selects
the exact reviewed Wario game, release, and location into a temporary
three-file checkpoint. It does not mint another identity or rewrite the source
fixtures. The installed semantic-input target remains Wario Land 4.

Each destructive gate creates `catalog/`, verifies a complete backup before
writing, checks all three installed documents, and restores prior bytes or
absence. RetroArch and overlay gates also retain their existing save, state,
preferences, process, and session safety checks. Failed restoration retains
recovery files and the device lock.

Fresh storage initializes only `device.yaml`, `catalog/games.yaml`, and
`catalog/releases.yaml`, each containing `{}`. These fixtures are never baked
into fresh-install defaults.

Offline checks, with no ADB or SSH effects:

```sh
services/korrid/test-minimum-config-fixtures.py
services/korrid/test-checkpoint-documents.py
plugins/retroarch/android/test-acceptance-contract.sh
services/korrid/android-device-script-review.sh
services/korrid/deploy/test-zao-remote.sh
```

These checks do not prove a physical-controller journey or an installed launch.
Run the explicit-device gates only with the device owner's approval.
