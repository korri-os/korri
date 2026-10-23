---
status: proposed
---

# A starter pack is a list acquired once, not a plugin that owns its games

**Owner decisions, 2026-10-23** (issue #189). The Starter pack stops being a
plugin that ships its games inside its package and pins exact builds of its
runners. That design made the runners impossible to update while the pack was
installed (#189: FAKE-08's save-state fix could not reach haku). It also tied
every game to the pack, so a person could not keep some games and drop others.

A starter pack is now a one-time offer. It is a list held by a starter-pack
plugin. Each game entry names the provider that serves the game, that
provider's item, and the expected content hash. Each runner entry names a
runner plugin. Running a starter pack acquires each game through the normal
acquire workflow and installs each missing runner as an ordinary plugin. After
it runs, the starter pack keeps no claim on anything it added.

- **One list plugin, unchanged game providers.** The list points at items in
  other providers. Game providers do not know starter packs exist; each one
  answers an ordinary acquire for one item. An entry whose provider is not
  installed and approved shows as unavailable and is never fetched another way.
  The shape of the list and of the handler that offers it is not decided here.
- **Per-item results.** Each game joins the library as soon as its acquire
  finishes. Failures are listed at the end. Running the starter pack again, from
  Settings, adds back only what is missing and asks first; it never runs by
  itself, so removals stick.
- **Offline through the cache.** The content hash comes from the existing
  release identity. A product image can pre-fill the acquisition cache by
  content hash, so a starter pack works with no network at first boot and still
  works online later. Acquire must accept a cached file whose hash matches
  before it asks the provider.
- **One review screen.** Setup shows every game and runner once. Accepting it
  records each runner's own approval. Afterwards each runner updates and is
  removed on its own.
- **Removal in the portal.** Anyone using the device's own portal can remove a
  game or a runner, after a confirmation. A removed game's files leave the
  device and the game stays in library history as unavailable (ADR 0013).
  Removing a runner lists the games it plays; they stay in the library with no
  way to play until a runner returns. Saves are never removed and keep syncing.
- **Every game has a provider.** The current pack's two Solarus quests and one
  ZQuest Classic quest need providers before a starter pack can name them.
- **Existing devices.** A one-off migration copies each old pack game into the
  acquisition cache by content hash, records its provider link where a provider
  exists, and removes the old pack's ties to its runners. Games, history and
  saves stay.

This supersedes ADR 0016 (declared games can come from their own package) for
the Starter pack: its games no longer come from its package, and disabling or
removing anything no longer makes them unavailable.

## Exact plugin dependencies elsewhere

Steam still requires exact builds of FEX and Proton. For that case an update of
the root plugin replaces its exact dependencies in the same approved step, and a
rollback returns the whole graph to its previous builds together.

## Considered options

- **Copy games into each system's source folder.** Games would look hand-copied,
  but it bypasses acquire, needs a write path into person folders that ADR 0013
  and ADR 0016 refuse, and removal would differ from acquired games.
- **A list from a URL or file the person enters.** Most open, but it needs a new
  list file format and its own trust rules. It can come later as another way to
  supply the same list.
- **Packs require runners by ID, not exact build.** Simplest updates, but a pack
  could run with a build nobody tested with it, and it drops the exact-graph
  rule restored in e5ffafd13.
