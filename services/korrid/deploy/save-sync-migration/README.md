# One-off save migration

Follow [ADR 0014's deployment cut](../../../../docs/adr/0014-saves-sync-by-session-not-by-file.md#one-off-deployment-cut)
for the stopped-daemon backup, account move, dry runs, review and activation.
These scripts are offline operator tools, not runtime compatibility code.

## Stranded SP identity switch

The owner decision dated **2026-10-22**, ADR 0023, **“An owned device joins by
identity switch, keeping its data”**, grounds the explicit account repair.
The shipped switch moved the SP's play log but left its live saves and private
snapshot history under the retired person. The ordinary `users/default` move
cannot repair that layout. Use `--adopt-retired-person` only with consent and
`--local-signer-state-root`; the script refuses missing or ambiguous switch
proof. See [SP account repair](SP-ACCOUNT-REPAIR.md) for exact records, code
paths, report guarantees, commands, and the required stopped-signer cleanup
of its stale runtime public-key projection. A signer restart alone does not
clear that projection. Never rename a different person's saves by assumption.

## Report and coverage

`migrate-retroarch.py` recursively inventories every regular file under the
owner's old `saves/`, `states/` and `screenshots/`. Each file has one result:

- `MOVE source -> destination`: a unique catalog game match. In a dry run this
  is a proposed move, not a completed move.
- `RETAINED (per-game root)`: already under `saves/<encoded catalog game id>/`.
  Reruns do not nest these files again. The save ACL still applies.
- `UNMATCHED` or `AMBIGUOUS (left in place)`: unresolved. Ambiguous lines list
  the candidate game IDs. These bytes stay untouched, but the new runner will
  **not** load them from the old location. Resolve them before playing.

`CLASSIFIED total=… moved=… retained=… unresolved=… unclassified=0` accounts
for the entire inventory, including unknown files nested below core folders
and screenshots. Missing or duplicate classifications refuse the migration
before any move or ACL change. An unresolved file is classified, not silently
ignored; its presence still needs operator review. A successful command alone
is not permission to discard unresolved files.

The ordinary filename rules remain `.srm` flat or one core folder deep,
`states/<raw runner id>/<stem>.*`, and flat plain/dated RetroArch screenshots.
Unknown deeper files are reported, not matched merely by their basename.
Only known per-game roots are retained as already migrated.

## FAKE-08 destination: consumer proof

Sources inspected in `korri-os/plugins` at
`adaccd5610ce5b997670251eb4a117c83d93ce8f`:

1. `plugins/libretro/fake08/package.nix` pins FAKE-08
   `814991a2571ad3970e386cef48f3b148aa1c27b9` and z8lua
   `e6928578d46b61fd5ea30cfcf547e855a30a0553`. It applies
   `eris-save-states.patch` and `libretro-autoload-state.patch`. The patches and
   README concern Lua/RAM savestate persistence and loading the queued cart
   before auto-restore. Neither changes cartdata paths or names.
2. `plugins/libretro/core-plugin.nix` supplies the FAKE-08 runner through
   `retroarch.ts::prepareLaunch`. That consumer sets `savefile_directory` to
   `input.saveRoot + "/saves"` and declares that whole folder as key `saves`.
3. `services/korrid/src/launcher/linux_plugin.rs` constructs `saveRoot` as
   `users/<encoded person>/saves/<encoded playable game id>`. The codec is
   `host/play_log.rs::encode_uri_component`, also used by this migration.
   This is the per-person/per-game layout in ADR 0014, not a new migration
   schema. `services/korrid/src/saves/access.rs::prepare_account` supplies the
   shared save access; `host/session_state.rs` prepares it before the save
   session and declared locations are used.
4. The frontend is RetroArch 1.22.2. Its `config.def.h` defaults
   `sort_savefiles_enable=true`, `sort_savefiles_by_content_enable=false`.
   `runloop.c::runloop_path_set_redirect` appends `system.info.library_name`
   to the configured save directory. Its `GET_SAVE_DIRECTORY` environment
   handler returns the resulting `savefile_dir`. The FAKE-08 core reports
   `library_name="fake-08"` (`platform/libretro/libretro.cpp`).
5. FAKE-08 `retro_init` takes `RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY`, appends
   `/` if needed, and calls `Host::overrideLogFilePrefix`. In
   `platform/libretro/libretrohost.cpp` that creates `cdata`. In
   `source/hostCommonFunctions.cpp::getCartDataFile`, **both reading and writing**
   use `_logFilePrefix + "cdata/" + cartDataKey + ".p8d.txt"`.
   `source/vm.cpp::vm_cartdata` passes the cart's Lua `cartdata("<id>")` string
   to this consumer. It does not use the cartridge filename.

Thus, for the observed core-sorted layout, the exact destination is:

```
users/<person>/saves/<game>/saves/fake-08/cdata/<cartdata id>.p8d.txt
```

The migration preserves the observed `fake-08/cdata/` suffix. It does not add a
fallback, compatibility read, second save copy, or cartridge-stem rename.
Custom RetroArch sorting overrides are outside this observed deployment;
review the actual configuration before using this migration elsewhere.

## Cartridge identity audit

The unchanged developer PNGs come from
`plugins/starter-pack/cartridges.nix` and its cartridge payload. Lua was
extracted with the pinned FAKE-08 `Cart(filename, "").LuaString` decoder,
without running or rewriting the cartridges, then searched with
`grep -a -n -E 'cartdata[[:space:]]*\(' *.lua`. All 25 PNGs (24 games, including
Into Ruins' two-cart pair) were inspected. Ten contain literal calls:

| Cartridge | Lua line | `cartdata` id |
|---|---:|---|
| `23208.p8.png` (Worm Nom Nom) | 222 | `i_k_wormnomnom` |
| `37216.p8.png` | 1104 | `toad_spaceman8` |
| `48406.p8.png` | 534 | `nusan_defacto` |
| `56576.p8.png` | 9 | `sweetbuns1` |
| `cherrybomb-0.p8.png` | 8 | `cherrybomb` |
| `highstakes-2.p8.png` | 16 | `highstakes` |
| `marble_merger-5.p8.png` (Marble Merger) | 3 | `2darray_marble_merger` |
| `mot_taxi-7.p8.png` | 9 | `mot_whiplashtaxi` |
| `pigments-0.p8.png` | 12 | `disc` |
| `vitreous-0.p8.png` | 1010 | `mush_vitreous_1` |

`migration.py::FAKE08_CARTDATA` records the SHA-256 release identity of those
exact original bytes and the observed string. This is a bounded one-off audit,
not a parser or a new catalog field. The tool joins those existing `pico8`
release hash identities to the actual `catalog/library.yaml` game IDs. It
never hard-codes a destination game ID or infers cartdata use from a filename.
Renamed cartridges still match their identity. Multiple releases of one game
collapse to one candidate. Several catalog games using the same audited cart
identity are ambiguous; a missing identity or an unaudited cartdata ID is
unmatched. Neither case is guessed or copied to several games. Other payloads
need their own cartridge audit before adding a mapping.

For the RG35XX Pro backup from 2026-10-23, each id occurs in exactly one
cartridge and joins exactly one library game:

| Old save suffix | Library game | New save suffix |
|---|---|---|
| `saves/fake-08/cdata/2darray_marble_merger.p8d.txt` | `marble-merger` | `saves/marble-merger/saves/fake-08/cdata/2darray_marble_merger.p8d.txt` |
| `saves/fake-08/cdata/i_k_wormnomnom.p8d.txt` | `worm-nom-nom` | `saves/worm-nom-nom/saves/fake-08/cdata/i_k_wormnomnom.p8d.txt` |

The fixture uses their real release hashes and cartdata filenames. Its save
bytes are authored test data, not the owner's progress.

## Checks and rehearsal on a build host

Run the focused tooling tests (temporary fixture trees only):

```sh
nix-shell -p 'python3.withPackages (p: [ p.pyyaml ])' \
  --run 'python3 services/korrid/deploy/save-sync-migration/test_migration.py'
```

For a copied backup on a host without the product `korrid`/`korri` accounts,
use the same CLI parser and orchestration with the backup device's **verified**
numeric UIDs. The tests use this seam too. This does not alter the product CLI
or create local product users. First make a restricted, separate working copy
of the backup; never supply the original backup or a live device's roots.
From this script directory, with PyYAML available, run the account dry run and
then account migration **on that copy only**, as in ADR 0014. Then:

```sh
python3 -c 'from migration import run; raise SystemExit(run("retroarch", save_users=(976, 1000)))' \
  --storage-root /absolute/path/to/working-copy/var/lib/korri \
  --private-state-root /absolute/path/to/working-copy/var/lib/korrid --dry-run
```

`976,1000` are the verified RG35XX Pro daemon/runtime UIDs, not portable
defaults. The original CLI still requires the actual product accounts.
Expect all seven old save/state files in this backup to be proposed moves,
including both 520-byte cartdata files, with zero unresolved or unclassified
files. Compare hashes and file counts before/after any copy-only apply, and
verify that everything outside the migrated account paths is retained. This
rehearsal does not replace the stopped-device backup or the post-deployment
in-game progress check.
