# Korri

Product terms shared across Korri devices. These definitions distinguish the product promise from work still being tested.

## Language

**Supported image**:
A Korri installation image verified to meet all agreed product requirements that apply to its device. It can have explicit limitations in hardware features that the product does not require.

**Development image**:
A Korri installation image that has not qualified as a supported image. It can boot and provide some features without meeting all applicable product requirements.

**Required plugin**:
A plugin needed for a Korri release to provide an agreed required behavior on a device. Inclusion in a default bundle alone does not make it required.

**Person key**:
The Nostr key that identifies one person to Korri. korrid never holds its private half.
_Avoid_: account, user key, login

**Person name**:
The name Korri shows for a person key: the name the person typed during setup, or else the name in their Nostr profile. When neither exists, Korri shows the device's label in place of a person.
_Avoid_: username, display name, nickname

**Device owner**:
The person key whose signed statement binds one device key. It is the person for every local call on that device.
_Avoid_: active user, current user

**Local signer**:
A separate service on a device that holds a person key and signs for korrid on request.
_Avoid_: keystore, wallet

**Automatic identity**:
The person key a local signer creates on first boot without sign-in. It is a real person key, not a temporary account.
_Avoid_: guest account, default user

**Identity switch**:
The explicit, consented replacement of a device owner by a different person key. It is the only way device authority moves.
_Avoid_: login, account transfer, sign in

**Live session**:
The one exact game launch a device holds as active. Every session control names it and is refused for any other launch. Its launch modifiers belong to it: they freeze, resume and end with the game.
_Avoid_: current game, running process

**Controller seat**:
A stable Korri input position to which a controller is bound. A seat is distinct from a physical controller and from a player inside a game. People see it as **seat**.
_Avoid_: physical controller, game player

**Pending launch**:
A launch Korri accepted to start that is not yet a live session. A device can hold several, and each one is named by its exact launch.
_Avoid_: queued game, loading game

**Cancel**:
Stopping a launch before its game takes the screen. A pending launch never starts, and a live session that has not taken the screen yet is ended.
_Avoid_: abort, kill, quit

**Launch modifier**:
A plugin that changes how another plugin's game launch runs, without being the game's runner. Gamescope is one.
_Avoid_: wrapper, launch companion

**Seat reservation**:
A controller's claim to its seat during a live session, including while it is disconnected or the person leaves the session.
_Avoid_: connected controller

**Leave**:
Moving away from a live session without ending it. The session is frozen and Korri owns input until the user resumes it.
_Avoid_: pause, minimise, background

**Resume**:
Coming back to a live session. Korri thaws it, raises its window, and gives it live input.
_Avoid_: return, unpause

**End**:
Destroying a live session on purpose. Unsaved progress is lost.
_Avoid_: quit, kill, close

**Gameplay overlay**:
The screen Korri shows over a live session. It offers resume, end, the full Korri screen, and the actions the active plugin supports.
_Avoid_: pause menu, system panel

**Shared play**:
More than one device playing one game together. Stream play and netplay are its two kinds.
_Avoid_: co-op, multiplayer, join

**Stream play**:
Shared play in which one device runs the game and holds the live session, and other devices play on its stream. Their controllers take free controller seats in that session. They can play and watch, but only the device that holds the session can end it or change it.
_Avoid_: remote play, guest stream, player two

**Netplay**:
Shared play in which each device runs its own copy of the game, and the copies are kept in step.
_Avoid_: online play, linked play

**Signal**:
A message between devices that is lost when no device is listening. A device that missed one catches up by asking. An invite is a signal.
_Avoid_: notification, event, ephemeral message

**Record**:
Data that devices keep until every device of the federation has it, even after the device that made it goes offline. Devices that are online together copy it to each other. Settings, the person name and saves are records.
_Avoid_: durable message, sync item, state

**Nearby link**:
A private Wi-Fi network that one device starts so that a device next to it can reach it with no router and no internet.
_Avoid_: hotspot, local link, direct link, ad hoc network

**Invite**:
What an idle device shows when another device holds a live session that it can take part in through shared play. It names the person who is playing and the game, not the kind of shared play; the receiving device chooses its route. It lasts until the session ends, the device is no longer idle, or the person dismisses it.
_Avoid_: notification, broadcast, request

**Product module**:
The one shared composition of every required Korri part. Every device installs it whole.
_Avoid_: base, shared config, host module

**Hardware fact**:
A value only a device can supply, such as its display, kernel, or input map. It changes how the product runs, never what the product contains.
_Avoid_: device override, device setting

**Recorded limit**:
An explicit statement that an optional hardware feature is absent or unverified on one device. It is the only allowed omission from the product.
_Avoid_: disabled feature, workaround

**Product check**:
The single evaluation-time check that every device configuration contains every required part. Passing it allows a build, not a support claim.
_Avoid_: module check, lint

**Delivery**:
A published installation image and its complete signed-cache closure from the same commit. A supported image needs both.
_Avoid_: release, artifact

**Sleep state**:
One kind of device sleep a device model can provide: light sleep, deep sleep, or hibernation. A device declares every state it has, and declares none when it has no sleep.
_Avoid_: sleep tier, suspend support, power mode

**Light sleep**:
The screen is off, the live session is frozen, and the device stays awake. A fixed delay then shuts the device down cleanly.
_Avoid_: standby, fake suspend, idle

**Deep sleep**:
Suspend to RAM. The frozen live session is still there on wake, with no time limit.
_Avoid_: S3, mem, real suspend

### Displays

**Display**:
A physical output that shows pictures, such as a built-in panel, a connected TV or a monitor. Every display belongs to exactly one device, and a device can have any number of them.
_Avoid_: screen, panel, output, monitor

**Display arrangement**:
Where a device's displays sit relative to each other, such as one above the other or side by side. Built-in displays have a fixed arrangement that the device declares; a person arranges the displays they connect.
_Avoid_: layout, display setup, topology

**Main display**:
The display that shows content with a single pane, the portal and Korri's own notices when nothing says otherwise. A device declares it; a device with one display has it as its main display.
_Avoid_: primary screen, default output

**Pane**:
One image that a game or surface produces, such as the upper or lower picture of a Nintendo DS game. A game has one pane unless its system has more; each pane is shown on one display.
_Avoid_: view, screen, picture, feed

### Content and discovery

**System**:
A kind of playable content, declared by plugins, with rules for what one game of that kind looks like. Korri knows no system of its own.
_Avoid_: platform, console, core

**System group**:
A named set of related systems that one library folder can hold together, such as Game Boy and Game Boy Color. Only plugins declare system groups.
_Avoid_: family, platform, collection

**Library folder**:
A folder on one device that discovery scans, paired with exactly one system or one system group.
_Avoid_: source, game folder, entry point, ROM folder

**Layout**:
A pattern that says where one game sits inside a library folder and which values its path names.
_Avoid_: shape, glob, folder structure

**Game**:
The thing a person plays, with every release of it on every device.
_Avoid_: title, entry, ROM

**Release**:
One version of a game's content. A file release is identified by its content bytes, so new bytes make a new release; a multi-disc game's discs form one release. A folder release is identified by its library folder and path on one device, and stays one release until a person says otherwise. A replaced release stays in the game's history as unavailable.
_Avoid_: ROM, build, file

**Game identity**:
What makes copies on different devices the same game: a provider reference, a hash list some plugin may contribute, or a link.
_Avoid_: game ID, fold key

**Link**:
A statement, signed by one person, that two games are the same game. It holds for every person and device in the federation; only its author can undo it.
_Avoid_: merge, match, personal link

**Declared game**:
A game a plugin states directly, naming its releases' hashes and any files it prepared or artifacts that hold them, instead of one discovery found in a library folder. A hash list is a set of declared games. A declared game whose release comes from artifacts is in the library before its artifacts are on the device.
_Avoid_: prepared game, plugin game, hash database

**Starter pack**:
A one-time offer that adds a set of games and the runners that play them, during setup or later. Its games are acquired from providers like any download, and work offline when the device already holds their content. After it runs, each game and each plugin stands alone: its games belong to the device's library as if the person had added them, the person can remove any game or runner, and the starter pack keeps no claim on what it added. Removing a game or a runner never removes saves. A removed game stays in library history as unavailable. The person can run a starter pack again; it adds back only what is missing, after asking.
_Avoid_: bundle, game pack, pack plugin

**Artifact**:
A download a plugin declares by fixed address and hash. Korri fetches it, checks it and unpacks it on the device that needs it, and can fetch it again if it is lost. It can hold a runtime or a game's content.
_Avoid_: payload, package, download (alone), delivery

**Variant**:
A game that is a port, edition or standalone mod of another game.
_Avoid_: fork, remake

**Add-on**:
Content with its own identity that is applied to a release of another game when it starts, such as a mod, a patch or a texture pack. A plugin declares it or a person adds it, and a person ties it to its base game; discovery never finds one.
_Avoid_: DLC, extension, mod (alone)

**Launch settings**:
The values that shape how a game runs, such as its runner, the runner's settings, CPU limits and hooks. Any value can be set on the device, a system, a game, a release, a profile or one launch, and each of these can limit it to one runner or runner family. For each value, the most specific place that sets it wins. They hold for one session and end with it.
_Avoid_: config, policy, cascade

**Runner scope**:
Which runners a launch setting applies to: any runner, one runner family, or one runner.
_Avoid_: runner layer, runner override

**Setting section**:
A named group of a plugin's settings, such as `video` or `input`. A shared section means the same thing in every plugin; a plugin has its own section only for something no other plugin has.
_Avoid_: category, group, namespace

**Setting path**:
The dotted name of one setting value inside its sections, such as `video.fps`. The most specific place that sets a path wins for that path alone.
_Avoid_: key (alone), option, flag

**Hook**:
A command that runs on the device just before a game starts or just after its session ends, as the game's user and inside the game's sandbox.
_Avoid_: script, trigger, callback

**Routine**:
A named list of hooks that any place in the launch settings can use.
_Avoid_: hook set, hook profile, preset

**Profile**:
A person's named variation of launch settings for one game, chosen when the game starts and remembered for its next launch. It changes only the values it names and keeps every other value from the device, system, runner, game and release. It is not content.
_Avoid_: preset, mode, config

**Attention list**:
The files on one device that discovery could not identify or could not decide between, shown to everyone on that device to resolve.
_Avoid_: errors, unmatched, inbox

### Saves

**Save history**:
The record of everything one person's play has saved for one game, on every device and every release. It is a chain of save snapshots, and it can hold splits.
_Avoid_: cloud save, save folder, save slot

**Save snapshot**:
One game's complete saves for one person, captured as one unit after a game exits. It names the snapshot its session started from and the release it ran. It holds in-game saves, savestates and screenshots.
_Avoid_: backup, version, save file

**Save location**:
A place on disk where one launch of a game writes that game's saves. The runner plugin declares it; korrid reads and writes it.
_Avoid_: save path, save dir, save folder

**Interrupted snapshot**:
A save snapshot made from what a game wrote before a crash or power loss, not after a clean exit. Korri keeps it and marks it.
_Avoid_: recovered save, partial save

**Split**:
Two save snapshots that started from the same snapshot and hold different saves or savestates. The person picks one to continue from; the other stays in the save history. Screenshots alone never make a split.
_Avoid_: conflict, divergence, fork

**In-use warning**:
A notice before launch that the same person's game is pending or live on another device, or has newer saves there that this device has not received. It never blocks a launch.
_Avoid_: lock, lease, checkout
