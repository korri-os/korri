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
| Session start and exit hooks | `HostSessionControl::prepare_inner` and the completion path in `services/korrid/src/host/session_state.rs` | None |
| Peers of the same person | owner roster on the relay (`services/korrid/src/relay.rs`), peer RPC (`services/korrid/src/peer_rpc.rs`) | Peers exchange no data today; this adds the first replication |

Research behind the choice, with sources and an audit:
`~/.cache/research/deterministic-save-sync-20261020/` (outside the repo).

## Considered Options

- **Wrap a file syncer (Syncthing, Unison, rclone bisync, git-annex).** Each one
  either picks a winner, renames files so the game cannot find them, or skips
  the conflict without a choice. None knows what a session started from.
- **Locks or leases (Perforce exclusive checkout, Git LFS locks, WebDAV
  LOCK).** A lock needs a network, so an offline device could not play.
  Kept only as the advisory in-use warning.
- **Last write wins by timestamp (iroh-docs, a CRDT register).** Drops one run
  without a warning.
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

## Open

Not decided yet, and not to be assumed by an implementation:

- Which files on disk belong to one game's save snapshot, and who declares them.
- Per-person save folders: `account_root` is `users/default` for everyone today
  (`services/korrid/src/launcher/linux_plugin.rs:110`).
- Games whose saves land outside `account_root`, and Steam games.
- How a split choice reaches the person's other devices.
- The data model, storage format and wire messages.
