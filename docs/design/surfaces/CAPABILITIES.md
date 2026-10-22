# Surface capabilities: the parity bar

Every Korri surface must have an answer for every item below. The bar is the
treaty, `contracts/surface/korri-surface.ts`: every field of `SurfaceModel`
and every method of `SurfaceHost`. Pico (`surfaces/pico`) is the proof that
the bar is reachable, not the template. Parity is by capability, never by
page: two surfaces answer the same item with different screens, different
words and different objects, and both are at parity.

Each direction's `DIRECTION.md` carries a `## Capability map` section with one
line per item, in this form:

```
C7  Busy: a black STARTING… parallelogram drops over the band; CANCEL beside it
C18 No answer yet
```

Rules for an answer:

- Name the surface's own page, object or control in the surface's own
  words. "Like Pico" is not an answer.
- An answer may be undrawn. The mockups draw one screen; the map says where
  the rest would live.
- `No answer yet` is allowed and honest. A direction with gaps is not done; a
  direction that answers everything differently from Pico is doing its job.
- When the treaty grows, every map owes a new line.

## Catalog (`SurfaceModel.catalog`, `SurfaceGame`)

- **C1 Catalog states.** Loading, Error with a message, Empty, Ready.
- **C2 Browse.** Title, `section` grouping (games sharing a section arrive
  consecutively), `subtitle`, cover art of any shape, and a game with no art.
- **C3 Game facts.** `lastPlayedAt`, `playCount`, `totalPlaytimeSeconds`,
  `resumable`.
- **C33 Download before launch.** `download` on a game: launching it first
  needs something fetched to this device; `bytes` is the size, formatted by
  the surface, and may be absent. While it is got, `queued` (waiting behind
  another download) and `progress` (`SurfaceDownloadState`, the same state a
  store entry's Get uses).
- **C4 Launch.** One place, or a choice among `launchLocations` that the
  person must make (never a silent default); `launchGame(gameId,
  launchLocationId?)`.
- **C5 Per-game actions.** `gameActions(gameId)` and `runGameAction`;
  `enabled`, `description`, and `destructive` actions confirmed before firing.
- **C6 Find by text.** A way to reach one game among many by typing part of
  its title (surface-local filtering over `games`).

## Status (`SurfaceModel.status`)

- **C7 Busy.** `kicker`, optional `detail`, host-supplied `actions` such as a
  real cancel, and the game it belongs to.
- **C8 Running.** `kicker` and the game, shown while a launch is live.
- **C9 Problem.** `kicker`, `reason`, `retry()` when `canRetry`, `dismiss()`,
  stated against `gameTitle` and not whatever is on screen.
- **C10 Reload.** `reload()` when the model needs a fresh start.
- **C31 Confirm.** Korri asks before it does something, such as a large
  download: `kicker` is the question, `detail` names what and how much; the
  surface renders exactly the host's `actions` and reports the chosen one
  with `runAction`, never inferring from a label which one agrees.
- **C32 Progress.** `Busy.progress`: a real bar from `done` and `total`;
  with `done` and no `total`, how far it has got without an end; without
  `progress`, only that work is under way.

## Saves (`SurfaceGame.saves`, `ChooseSave`, `SurfaceModel.saveHistory`)

- **C34 Save marks.** `saves` on a game: Synced, NotChecked, NotSynced,
  SyncedBySteam, or Split with its `choose` action; absent until Korri knows.
- **C35 Choose a save.** The `ChooseSave` status: `kicker`, `detail`, at
  least two `sides` (`device`, `endedAt`, `durationSeconds`, optional
  `imageUrl`), each side's `actionId` and the host's `actions` run with
  `runAction`; the surface never picks a side itself.
- **C36 Save history.** `saveHistory`: Loading, Error with a message, Ready
  with entries newest first (`device`, `endedAt`, `durationSeconds`,
  `interrupted`, `current`, and `restore` on every entry but the current
  one); Ready with no entries is an empty history; the host's close action.

## Runner choice (`SurfaceModel.runnerChoice`)

- **C11 Runner choice.** States Loading, Ready, Stale, Conflict, Error, Busy,
  Warnings; `routes` with `runnerBuild`, `program`, per-route `warnings` and
  `actions`; the `saved` choice; the content's own `warnings` and `actions`.

## Launch settings (`SurfaceModel.launchSettings`)

- **C37 Launch settings editor.** Closed, Loading, Ready, and Conflict and
  Error with a `message`; `gameTitle`; `layers` most general first, each
  with its groups; rows through `changeSetting`, actions through
  `runAction`; `note`; a write in progress is `settingsStatus` Saving.
- **C38 Profiles.** `profiles` and `selectedProfileId`; pick and New profile
  in the Game layer, Name and Delete in each Profile layer. Back closes the
  surface's own questions first, then runs the host's close action.

## Settings (`SurfaceModel.settings`, `settingsStatus`, `actions`)

- **C12 Groups and items.** `SurfaceSettingGroup.title`, item `label`,
  `value`, `description`; `source` (where the value in force comes from,
  shown beside it, never parsed) and `scope` (`device`, shown as
  device-only, or `shared`).
- **C13 Interactions.** `choice` (pick among `choices`, marked and stepped
  from `selected`, never by matching a label), `text`
  (`placeholder`, `maxLength`), `sensitiveText` (masked, with `clearLabel`),
  `action` (with optional `confirmation` title, message and confirm label;
  `destructive`); `changeSetting` and `runAction`.
- **C14 Saving and Problem.** `Saving` on one setting; `Problem` with a
  message and `dismissSettingsProblem()`.
- **C15 Device actions.** `SurfaceModel.actions` run with `runAction`.

## Identity (`SurfaceModel.identityManagement`)

- **C16 Export backup.** Ask for a password, `exportIdentityBackup`, then
  show `BackupReady.encryptedSecret` so a person can carry it away (a QR
  code and the text).
- **C17 Switch from backup.** Enter an encrypted secret and its password,
  choose the `disposition` (transfer or delete), confirm trust loss,
  `switchIdentityFromBackup`.
- **C18 Switch by bunker.** Enter a NIP-46 bunker URI, disposition, trust-loss
  confirmation, `switchIdentityToNip46`.
- **C19 Retired keys.** List `retiredPublicKeys`; `deleteRetiredIdentity`
  only after `backupConfirmed`.
- **C20 Identity status.** Working with an `operation`, Problem with a
  message, Switched with the new `ownerPublicKey`; `dismissIdentityStatus()`.

## Network (`SurfaceModel.network`)

- **C21 Connection.** Offline, Wired, or Wifi with `ssid` and
  `strengthPercent`.
- **C22 Wi-Fi list.** `NoAdapter`; `scanning`; `networks` strongest first
  with `strengthPercent`, `secured`, `known`; `attempt` Connecting or Failed
  with a message and `passwordRejected` (offer to type it again); `scanWifi()`.
- **C23 Join.** `connectWifi(networkId, password?)` for a listed network;
  `connectHiddenWifi(ssid, password?)` for a hidden one.

## Gameplay overlay (`SurfaceModel.presentation`)

- **C24 Overlay.** `title`, flat `controls` and `groups`; interactions
  `command`, `toggle` (`trueLabel`/`falseLabel`), `choice`, `range` (value
  shown while it moves, `min`/`max`/`step`); `enabled` with
  `disabledReason`; `destructive`; `dismissOnSuccess`;
  `invokeGameplayControl` and `dismissGameplayOverlay()`.

## Onboarding (`SurfaceModel.onboarding`)

- **C25 First boot.** Controllers with `heard`; the surface owns the steps
  (controllers, Wi-Fi, device name, identity) and ends with
  `finishOnboarding()`. When `starterPack` is present: Offered with the
  host's `actions`, Installing, Installed, Failed with a message and
  actions.

## Store (`SurfaceModel.store`)

- **C39 Providers.** `openStore()`; Loading, Problem with a message and the
  host's Retry, Ready with `providers` (empty when no provider is enabled).
- **C40 Search.** `searchStore(providerId, query)`; Idle, Searching, Problem
  with a message and Retry, Ready with `entries` (title, `system` when the
  provider says); no results; no provider art, so the surface draws its own
  stand-in.
- **C41 Entry and Get.** `openStoreEntry(entryId)`; details Loading, Problem,
  Ready with `description`; `get` as Offer (with `note`), Unavailable with
  its `reason`, Confirm, Working (`queued`, `progress`, Cancel),
  InLibrary (`gameId` for `launchGame`), Problem with Retry. A download
  keeps going while another entry is open.

## Chrome and input

- **C26 Clock and build.** `clockLabel` and `buildLabel`.
- **C27 Idle.** What the surface does when nobody presses anything (attract,
  screensaver, or nothing, stated).
- **C28 Input actions.** What `options`, `menu` and `system` do in this
  surface, beyond direction, confirm and back. Everything must be reachable
  with d-pad, confirm and back alone.
- **C29 Text entry.** How a person types with a d-pad: a Wi-Fi password, a
  hidden SSID, a device name, a bunker URI, a 40-character backup secret.
- **C30 Focus and motion.** Focus visible at all times; motion answers input;
  `prefers-reduced-motion` honoured.
