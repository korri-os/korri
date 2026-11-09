# Korri save bridge (ADR 0054, case 2)

A small Java library that every Android APK Korri builds includes, so a
game's saves sync like Linux saves. Nothing in the game's engine changes.

- `KorriSaveBridgeProvider` goes into the game APK. Declare it with authority
  `<applicationId>.korri.savebridge`, exported, and guarded by the
  `signature`-level permission `dev.korri.permission.SAVE_BRIDGE`, so only
  Korri (same signing key) can call it. `integrate.py` does all of this.
  Name the save folders in the game's manifest, relative to
  `getExternalFilesDir(null)`, keyed like the plugin's `saveLocations`:

  ```xml
  <meta-data android:name="dev.korri.savebridge.folders" android:value="save=v190/save" />
  ```

- `KorriSaveBridgeClient` is in the Korri app. Before a launch korrid copies
  its latest save snapshot in; on End it asks the game to quit, copies the
  folders out, and takes its usual snapshot.

`Paths` holds the shared path rules and is unit-tested without a device.
