# Move native settings into the plugin bucket

ADR 0057 replaces root-level native settings with `settings.plugin`. The
migration tool uses the existing places from `config/place.rs` and the two
readable files from `config/snapshot.rs`: `device.yaml` and
`catalog/library.yaml`. It never moves a native value into a shared setting.

Run this once on the build host, against a staged copy of the device's files.
Do not run it on a device or add it to startup. Deploy the migrated records,
core and matching plugins together. Old plugins cannot read the new bucket;
the new core has no fallback reader for old native paths.

1. Copy both readable files into a staging folder. Retain their relative paths.
2. Inspect the actual records and select each affected contribution explicitly.
   Use `--runner` for a runner or Moonlight viewer, `--family` for a runner
   family, `--modifier` for Gamescope, and `--service` for Sunshine. Repeat a
   flag for each selected ID. Do not select an ID merely because it is installed.
3. Run the tool without `--apply`. It reports changed-place counts and hashes,
   not private values. Check that only the intended contributions change.
4. Run the same command with `--apply`. The tool backs up original bytes in
   `.before-shared-settings` siblings before replacing either file. An existing
   backup, duplicate YAML key or mixed old/new selected record stops the move.
5. Review the staged files before transferring them with the matching release.
   YAML values and null clears survive. YAML comments and formatting survive
   only in the backups; restore any useful comments in the staged result.

For one selected runner, with both variables set from the staged records:

```sh
./services/korrid/deploy/migrate-plugin-settings.py \
  --root "$STAGED_CONFIG" --runner "$RUNNER_ID"
```

The tool leaves unselected records unchanged. An already migrated record with
only the plugin bucket needs no further change. It refuses a selected record
that combines that bucket with other sections, because it cannot infer which
values are old native settings and which are newly authored shared settings.

The rollback source is the original pair of files plus the previous matching
core and plugin versions. Do not restore old records while the new core is
running. This procedure does not migrate native game TOML, saves or game data.
