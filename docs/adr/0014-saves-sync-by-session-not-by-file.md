# Saves sync by session, not by file

**Owner decision, 2026-10-20.** Design in progress; the open questions at the
end are not decided.

A file syncer such as Syncthing loses progress without a warning. It sees only
writes, not which save a game loaded. A game that started from save v1 can
write over v2a that arrived while it ran, and the syncer treats that as an
ordinary later edit. Korri knows when every live session starts and ends, so it
can record what each session started from.

## Decision

- korrid reads and writes a game's saves only at session boundaries: before a
  launch and after the game exits. It never changes save files during a live
  session, including a frozen one.
- One person's saves for one game form one save history. It spans every release
  of the game and every device. Each save snapshot records the release it ran.
- korrid takes a save snapshot after the game exits, never on Leave. The
  snapshot holds the game's in-game saves, savestates and screenshots as one
  unit, and names the snapshot its session started from.
- After a crash or power loss, korrid makes an interrupted snapshot from the
  files the game wrote, on the snapshot the session started from, at its next
  start.
- A split is two save snapshots that started from the same snapshot. korrid
  finds splits by ancestry, never by clocks or arrival order. Snapshots made
  offline that meet later are checked the same way.
- On a split, Korri shows a notice when it finds it, and requires a choice
  before that game next launches. The person picks one snapshot to continue
  from. The other stays in the save history and can be restored. Sync never
  deletes a snapshot.
- Before a launch, korrid gives an in-use warning when another device of the
  same person holds a pending launch or a live session (frozen included) of the
  game, or has newer saves that this device has not received. The warning has
  no expiry, names the device and since when, and always offers to launch
  anyway. It never blocks; the split check still protects the launch.
- Before a launch, korrid waits up to 3 seconds for the person's other devices,
  with a Skip action. When none answers, it asks before launch only if the last
  news from another device said it held the game in use or had newer saves.
  Otherwise it launches and shows that it did not check with other devices.
- When the latest snapshot ran a different release than the one about to
  launch, Korri names both releases, warns, and allows the launch.
- Snapshots move right after they are made, to every reachable device of the
  person; whenever two of the person's devices come in contact; and before
  every launch.
- Each device keeps the latest snapshot of every game it can play and both
  sides of every open split. Older snapshots stay on the device that made them.
  Korri keeps the last 10 snapshots of each game, plus every side of a split
  that was not picked.
- "Delete save" records an empty snapshot that syncs everywhere. Earlier
  snapshots stay restorable.
- After an identity switch, the previous owner's saves stay on the device,
  untouched, and stop syncing. They sync again if that person owns the device
  again.
- The runner plugin declares the save locations of each launch in its launch
  declaration. korrid snapshots them; the plugin performs no effect.
- For a program that does not let the plugin choose where it saves, the plugin
  declares where the program writes and which save location that maps to.
  korrid binds the save location there for that launch only. The plugin
  declares the bind; korrid performs it.
- A game whose runner declares no save locations launches as today and shows
  that its saves are not synced.
- Every game gets its own save locations. A runner whose folder is shared by
  every game (RetroArch saves and screenshots, PPSSPP's memory stick, ZQuest)
  points the program at a folder for that game on each launch, as legacy
  RetroArch did. A save location is always a whole folder.
- A save location is identified by a stable key that the plugin declares, not
  by its path. Runners that declare the same key share those files; different
  keys never mix. A plugin update may move a folder without breaking history.
  The keys themselves are proposed from real plugins and approved later.
- The person cannot turn sync off for a game, runner or system. Every game
  whose runner declares save locations syncs.
- When a device shuts down from light sleep with a frozen session, korrid stops
  the game, makes an interrupted snapshot during shutdown, and pushes it if a
  peer is reachable.
- The split choice shows, for each side, the device that made it, when the
  session ended by that device's clock (display only), how long it lasted, and
  its newest screenshot or savestate thumbnail.
- A save location is always inside the person's own folder. A program that
  expects its saves elsewhere gets a folder bind or an overlay from there.
- A plugin may declare folder binds and overlays. An overlay keeps a game
  folder read-only and sends everything the game writes to the person's save
  location; it serves games that save beside their own files. Single-file binds
  are not allowed: a save written as a temporary file and renamed over the old
  one can fail with `EBUSY` on a mount point (rename(2)).
- korrid builds the mount namespace by running the launch through bubblewrap
  inside the existing systemd scope. A scope cannot take systemd's
  `BindPaths=` (systemd.scope(5)). Which launches run under it is ADR 0015.
- Snapshots live in a korrid-native store: files by SHA-256 under korrid's
  storage root, and snapshot records that name the snapshots they started
  from. Not Git objects: history is short, dropping old snapshots must be a
  plain delete, and `gix` was not tested with 0.5 to 2 GB savestates.
- When sync first starts, games whose saves are byte-identical on two devices
  join without a question. Every other game with saves on more than one device
  is listed once in a one-time review, and the person picks per game.
- Two snapshots with byte-identical content are not a split, even when they
  started from the same snapshot. Korri compares content hashes, never clocks.
- Screenshots never cause a split by themselves. Both sides' screenshots are
  kept as a union. Saves and savestates follow the strict rule.
- Changes to different files of one game on two devices are a split, never a
  file-level merge: every snapshot equals something a real session wrote. This
  holds when two devices played the game through different runners with
  different save keys.
- When the person turns bubblewrap off for a runner whose plugin declares binds
  or overlays, its games launch and show that their saves are not synced.
- Restoring an older snapshot makes a new snapshot with that content, which
  syncs to every device. Two different restores made apart are a split.
- A device never drops a snapshot that no other device has confirmed
  receiving, whatever retention says and whether or not the game is still on
  the device.
- On an identity switch away from an automatic identity, Korri offers to bring
  its saves to the new owner. Each game then goes through the first-sync review.
  A real person's saves stay theirs (above).
- When a Link joins two games that each have a save history, the first-sync
  rule applies: byte-identical saves join, otherwise the person picks, and both
  histories are kept.
- Peer RPC carries snapshot records. File content moves by hash over a separate
  HTTP transfer between the person's devices, authenticated like peer RPC, and
  only content the other device lacks is sent. Transfers run on battery too.
- A split found during a live session is shown after the session ends, or
  earlier if the person opens the gameplay overlay.
- Each person gets their own `account_root` under `users/`, keyed like the play
  log keys a person (`services/korrid/src/host/play_log.rs`). Today every person
  shares `users/default` (`services/korrid/src/launcher/linux_plugin.rs:110`).
  At deploy, a one-off operator migration moves `users/default` to the device
  owner's folder; no compatibility path stays.
- Steam games are left to Steam Cloud, and Korri says so for those games. korrid
  keeps no access to Steam saves (`steam-session.sh` ACLs).
  Deployment limitation (2026-10-21): named korrid sharing applies only under
  each person's `saves/`. The former account's runtime-group defaults remain
  unchanged for AccountRoot prepared games such as Fallout CE. As before this
  slice, korrid in that group can reach runtime-created descendants, including
  `STEAM_HOME`, except where Steam narrows access. This slice adds no named
  korrid grant there and never visits that subtree. Strict Steam credential
  isolation remains a follow-up (#139); the pinned Steam package is unchanged.
- The split notice shows on every device of the person that knows about the
  split, until it is resolved.
- Picking a side of a split makes a new snapshot with the picked content that
  names both sides as where it started. It syncs like any session. Two
  different picks made apart are themselves a split.
- When the latest snapshot's savestates came from a different runner than the
  one about to launch, Korri warns that they may not load and allows the launch.
- Snapshots move only between the person's own devices. A hosted store is out
  of scope until a separate spike.
- Every runner plugin's games are in scope, not only emulators.

## Grounding

| Element | Comes from | Delta and why |
|---|---|---|
| Sync before launch and after exit | legacy `work/parking-lot/01KSRGFP090VJBVE7MNCGWYF9R-steam-like-savestate-sync.md` ("pull the latest before launch … on session exit, upload") | None |
| Ask before launch when progress is not safe | legacy `docs/jobs/safe-game-resume.md` SGR-O1 | A split always requires a choice. When no device answers, Korri asks only on a known risk, not on every unverified launch: saves move only between the person's devices, so an offline handheld could never verify and would ask on every launch. |
| Choice between two saves side by side | legacy `product/surfaces/web/pico/pages/future/CloudSaveConflict.tsx` | Renamed from conflict to split (CONTEXT.md) |
| Per-person save location | `PluginLaunchInput.account_root` (`services/korrid/src/launcher/plugin_launch.rs`) | None |
| A save folder per game | legacy `product/plugins/retroarch/src/materializer.ts:251-294` (a save and a state folder per system and game identity) | main's `services/korrid/examples/libretro-retroarch.ts` shares one `saves/` folder across games; this returns to legacy's per-game folders |
| Session start and exit hooks | `HostSessionControl::prepare_inner` and the completion path in `services/korrid/src/host/session_state.rs` | None |
| Peers of the same person | owner roster on the relay (`services/korrid/src/relay.rs`), peer RPC (`services/korrid/src/peer_rpc.rs`) | Peers exchange no data today; this adds the first replication |

Research behind the choice, with sources and an audit:
`~/.cache/research/deterministic-save-sync-20261020/` (outside the repo).

## Changes from issue #7

Issue #7, "Design deterministic plugin-declared peer data sync", holds the
owner's earlier notes. This decision keeps its core and changes the rest. The
later, explicit answers win; #7 stays open for the owner to close or reshape.

Kept from #7: plugins declare and korrid acts; a declared group is the unit of
choice and files from two runs never merge; no clocks decide anything;
identical content does nothing; a choice spreads as data; two groups that
chose differently while apart are asked again.

| #7 said | This decision |
|---|---|
| The core syncs opaque data and knows no save, state or profile concepts | The core knows saves, savestates and screenshots: screenshots merge as a union, and savestate and release mismatches warn |
| The losing package is discarded after the choice; no user-facing history | The other side stays in the save history and can be restored; the last 10 snapshots are kept; a restore screen is in the first slice |
| Launcher paths stay device-local, with fixed paths, several roots and launcher-private data allowed | A save location is always inside the person's own folder; other paths get a bind or overlay under bubblewrap (ADR 0015) |
| Any peer or an untrusted relay may store and forward | Each device keeps snapshots for games it can play; a hosted or relay store waits for a separate spike |
| Full compressed packages first; chunking later | Content moves by hash, and only content the other device lacks is sent |
| A narrow first proof: two devices, one data group, no cloud-save features | The first slice holds every feature listed below |

## First slice

RetroArch with a GBA game. The owner kept every feature in it (2026-10-21). It holds: per-person save folders, a save folder
per game, snapshots after exit, interrupted snapshots, push, meet and pull
between the person's devices, split detection with notice and choice, the
first-sync review, browsing and restoring older snapshots, the "saves not
synced" mark, the in-use warning, and the release and runner mismatch
warnings. Binds come in a later slice, with Fallout 1 CE.

## Plugin survey (2026-10-20)

Every runner on main, in the owner's `korri-plugins` repository, and on legacy
was checked: about 40 runners. On Linux, none of them is impossible to direct.

- Native redirect (env var, argument, config key or working folder): RetroArch,
  PPSSPP, melonDS, RPCS3, Ryubing, Dolphin, MAME, Solarus, ZQuest Classic,
  Chromium-based runners, Godot games, SRB2, Neverball, and most personal
  plugins. OpenGOAL needs `XDG_CONFIG_HOME` set (inferred from upstream, not
  tested). Skate 3 redirects, but its saves share a folder with extracted game
  files.
- Bind needed, because saves sit next to game files: Fallout 1/2 CE
  (`SAVEGAME/` in the working folder), GMLoader, many PortMaster ports, 3dSen,
  and Wine fangames that save beside the executable (`psycho-waluigi`).
- Out of reach: Steam games inside pressure-vessel (left to Steam Cloud), and
  the Android app sandbox (Android was dropped on 2026-09-16).

## Considered Options

- **Wrap a file syncer (Syncthing, Unison, rclone bisync, git-annex).** Each one
  either picks a winner, renames files so the game cannot find them, or skips
  the conflict without a choice. None knows what a session started from.
- **Locks or leases (Perforce exclusive checkout, Git LFS locks, WebDAV
  LOCK).** A lock needs a network, so an offline device could not play.
  Kept only as the advisory in-use warning.
- **Last write wins by timestamp (iroh-docs, a CRDT register).** Drops one run
  without a warning.
- **Path rewriting instead of mounts (LD_PRELOAD, PRoot, seccomp user
  notification, FUSE).** LD_PRELOAD misses static, Go and FEX-run programs;
  PRoot and FUSE slow every file operation; seccomp makes korrid carry out each
  file operation. Research: `~/.cache/research/force-app-folder-context-20261021T000404/`.
- **A hosted store with compare-and-swap writes (S3, R2).** Deferred to a spike:
  it needs an outside service, and some S3-compatible stores do not document the
  write condition.
- **This decision.** A snapshot history per person and game, checked by
  ancestry, synced at session boundaries between the person's devices.

## Consequences

Two devices that play the same game apart still make two runs. Korri cannot
merge them; it makes sure the person sees both and chooses. korrid gains its
first peer-to-peer data replication and its first store for user files.
Savestates of 0.5 to 2 GB make storage and transfer cost real.

## Slice 1 as built (#126, 2026-10-21)

The owner approved the slice-1 contract: the per-person `account_root`, the
per-game `saveRoot`, `saveLocations` with kinds, the private snapshot store,
the content-derived records, the peer and local `app.saves.*` methods, the
`/peer-saves-object` route, and the surface facts. Limits that ship with it:

- The split choice shows no screenshots yet. No local route serves images to
  the portal (#129); the records keep every screenshot hash.
- The release and runner mismatch warnings name the game and the device, not
  the releases or the runner, because neither has a display name yet. They name
  both once display names exist.
- A transfer resumes after a dropped connection, but not after a korrid
  restart.
- A game whose route can't be resolved yet is skipped for baseline capture and
  retried at the next start or contact. Its local files are never touched.

## Open

Not decided yet, and not to be assumed by an implementation:

- The shape of the bind and overlay declarations in the plugin contract
  (ADR 0015).
- Strict Steam credential isolation from korrid (#139).

## One-off deployment cut

Deploy the new korrid and RetroArch plugin together. The plugin now receives
`saveRoot` and declares `saves`, `states`, and `screenshots`; an old host cannot
supply that input. There is no runtime fallback to `users/default` or the old
shared save folders.

On each device, before starting the new korrid:

1. End every session, including frozen sessions. Let the old korrid finish its
   journal recovery. Stop `korrid.service` and keep it stopped through both
   migrations. Back up `users/` and the private state root while stopped.
2. Use the scripts in `services/korrid/deploy/save-sync-migration/`. Supply the
   actual `services.korri.storageRoot` and `privateStateRoot`, not a new empty
   root. The defaults are `/var/lib/korri` and `/var/lib/korrid`:

   ```sh
   ./migrate-account.py --storage-root /var/lib/korri \
     --private-state-root /var/lib/korrid --dry-run
   ./migrate-account.py --storage-root /var/lib/korri \
     --private-state-root /var/lib/korrid
   ./migrate-retroarch.py --storage-root /var/lib/korri \
     --private-state-root /var/lib/korrid --dry-run
   ./migrate-retroarch.py --storage-root /var/lib/korri \
     --private-state-root /var/lib/korrid
   ```

   Run from that script directory as an operator with permission to read the
   private state and preserve file ownership. Make the scripts' Nix Python
   dependencies available as prebuilt downloads; do not build on the device.
   Review each dry run before its corresponding write command. The second dry
   run requires the first migration to have completed.
3. Review every `UNMATCHED` and `AMBIGUOUS` line. Those files stay where they
   are; the new runner will not load them. Resolve their game identity before
   playing if they contain progress to keep. Do not delete them.
4. Activate the new product and plugin, restart korrid, and launch the GBA game.
   Confirm its old progress loads. End the session and check save history.
   The first peer contact can require the first-sync review; do not overwrite
   another device's files to avoid that review.

The first script reads the existing `identity/owner.event.json` owner public
key (`pubkey`) and requires an `owned` binding. It trusts this private,
previously daemon-validated event; it neither reads the private key nor repeats
signature verification. Check `korrid identity status` with the old installation
before stopping if the binding's validity is uncertain. The destination uses
exactly the play log's `encodeURIComponent` path codec.

The second script joins `catalog/library.yaml` release `identity.value` to
`device.yaml` file `locations` and matches each content filename's stem. These
are the existing `config/library.rs` and `config/mod.rs` shapes, not a new
migration manifest. Multiple releases of one game can match; a stem shared by
different games is ambiguous and stays untouched. The slice covers `.srm`,
`states/<runnerId>/<stem>.*`, and RetroArch's plain or dated PNG/BMP screenshots.
The raw runner id is preserved, including its nested folders. Provider and
multi-file locations, custom screenshot names and files without a catalog
match are not guessed.

Both commands refuse any nonempty `host-session` journal, including pending
completion or recovery files. Recover with the old korrid; never delete that
journal to force migration. A stopped daemon remains an operator precondition:
checking a journal does not lock out a concurrent launch. Dry runs write
nothing. Moves preserve file metadata, reject symlinks and existing
destinations, and never merge or overwrite files. A rerun skips completed
moves. A failure can leave earlier moves complete; review another dry run
before continuing. Cross-filesystem moves are refused rather than copied.
These are one-off operator actions, not daemon migrations.
