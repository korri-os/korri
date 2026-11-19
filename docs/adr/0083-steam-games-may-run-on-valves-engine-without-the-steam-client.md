---
status: accepted
---

# Steam games may run on Valve's engine without the Steam client

**Context, 2026-11-17 (issue #187).** `@korri:steam` runs every Steam game
through Valve's full Steam client. On the Retroid Pocket Mini V2 that costs
about 28 s from launch to game, 1.4 GB of memory for the client and its web
UI, and Steam dialogs that Korri cannot answer. A spike showed that a small
program can load Valve's own engine library (`steamclient.so`) unchanged, sign
in with Steam's saved login, and serve games with no client and no UI. Steam
Cloud works in both directions, and controllers work through Korri's seat. A
random sample of 40 library games ran. A launch took about 8 s, and the whole
session used about 0.5 GB. A Steam folder that holds only the engine (44 MB,
compared with about 4.7 GB for the client and its package cache) signed in,
installed a game from Steam's servers and played it.

The program depends on undocumented engine internals, found by reading the
binary. Each engine update can break it until those internals are mapped
again. Steam's Subscriber Agreement forbids reverse engineering and
third-party control of Steam software (§2.G, §2.G(ii), §4.B), except where
applicable law allows it. EU law allows it for interoperability. Valve has
long tolerated authenticated tools of this kind, for example SteamKit2,
OpenSteamClient and Aurelia. The risk falls on the person's Steam account.

**Owner decisions, 2026-11-17.**

## Decision

- `@korri:steam` gets a second runner, `@korri:steam/engine`. It plays games
  through Valve's engine with no Steam client. The client runner stays, and
  it is the fallback.
- The engine runner is opt-in. It becomes the default only when it passes as
  many games as the client runner on the same random sample of the person's
  library. A game passes when a screenshot shows gameplay, not only a live
  process.
- The end state is a device with no Steam client installed. The engine also
  installs and updates games.
- The device downloads only the engine package, from Valve's own update
  servers. It checks the package against the version that Korri pins. Korri
  never hosts or redistributes Valve files.
- Korri pins the engine version and moves the pin only after the new engine
  is mapped. The device keeps using the pinned engine while Steam accepts it.
  If Steam refuses it, Korri offers to install Valve's client as a temporary
  fallback, and removes the client again when the pin moves.
- A person signs in to Steam by scanning a QR code and approving it in the
  Steam mobile app. No password is typed on the device. Korri offers the sign-in
  during setup, in settings, and the first time a Steam game is opened.
- `@korri:steam` is also a provider. A person's Steam games are found and
  acquired like any provider's games: the games the account owns, free games
  on the account, and games shared through Valve's Steam Families feature (not Korri family
  members). The library holds
  the games that are installed. Korri does not sell games; a person buys them
  on Steam's website or app.
- A Steam account is linked to one Korri person. On a device that several
  people use (its owner, family members or guests), each person has their own
  Steam folder and sign-in, and the person playing uses their own Steam
  account. Sharing one Steam
  account between Korri people is not decided.
- Each device signs in with its own QR approval and holds its own token.
  Steam lets only the Steam mobile app's authenticator approve a new login,
  and a copied token would make two devices share one revocable session, so
  one Korri device cannot sign in another.
- korrid acquires a Steam game by running the engine's install as a provider
  job that korrid starts, queues, reports and can cancel. The engine moves the
  bytes from Steam's content servers, which the plugin declares. This is an
  exception to ADR 0016, where korrid downloads the bytes itself. The engine's
  traffic is not restricted to those hosts at first. A follow-up spike checks
  whether korrid can download Steam content itself.
- Before a launch, the engine installs any available update, and Korri shows
  its progress. The game starts after that.
- The runner, the shim and the engine host's source are public, in the Steam
  plugin. The map of engine internals for each pinned engine version is built
  privately and ships only with the prebuilt engine host.
- The sign-in token stays in the engine's Steam folder, owned by the user
  that runs games. The engine reads and writes it. korrid never reads it.
- The engine runs for one session and stops when the session ends. Korri does
  not keep it running between games.
- Steam games may start with no network. They sync on the next session that
  has one.
- The first target is the Retroid Pocket Mini V2.

## Consequences

- The engine runner has no Steam overlay, no Steam Input and no Remote Play
  Together. Korri owns controller mapping for these games, including games
  that expect a mouse.
- For games on the engine runner, a Steam Cloud conflict is a split. Korri
  takes save snapshots of Steam saves at session boundaries, so the conflict
  has two snapshots in the save history. A game's Steam saves are exactly the
  files in its Steam Cloud set, as the engine reports them. The engine host
  packs those files and gives korrid the snapshot, so korrid never opens the
  Steam folder. A game with no
  Steam Cloud has no Korri save snapshots. This changes the rule in ADR 0014
  that leaves Steam games to Steam Cloud.
- Downloading the engine from Valve's servers is an exception to ADR 0042's
  rule against outside download addresses. The exception covers Valve's
  update servers and the pinned engine package only.

## Amends ADR 0011

ADR 0011 names one Steam plugin, `@korri:steam`, as the owner of the Steam
manifest discovery. This ADR moves the Steam integration to the single enabled
plugin named `steam`, whatever its publisher namespace. Every id korrid uses for
it is derived from that plugin's id `P` by ADR 0045: the discovery
`P/installed-apps`, the provider `P`, the client runner `P/steam`, the engine
runner `P/engine` and the engine package `P/engine-package`. The plugin declares
each of them by its full id.

- The Steam integration covers the installed-apps discovery reads (both
  manifest folders, the client runner's `steam/` and the engine runner's
  `steam-engine/steam/`), korrid's admission of `P` as the engine-backed
  acquisition provider, the engine host launch for `P/engine`, and the Proton
  lookup for that launch.
- korrid grants it only while exactly one enabled plugin is named `steam`. With
  two or more, korrid grants none of it and logs that Steam features are off.
  The client runners stay ordinary runners.
- Trust comes from two things: the owner's approval when a plugin of a bound
  publisher is installed, and this single-enabled rule. The name `steam` alone
  grants nothing.
