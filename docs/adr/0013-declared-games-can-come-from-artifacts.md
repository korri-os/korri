---
status: proposed
---

# Declared games can come from artifacts

**Owner decisions, 2026-10-20.** A declared game's release can come from
artifacts ([ADR 0012](0012-korrid-acquires-declared-artifacts.md)), not only
from a file found in a source or prepared under an account root
([ADR 0003](0003-plugins-declare-prepared-games.md)). Without this, korrid can
download a game but the game never reaches the library, so nothing asks for
the download. The first case is Mega Man Rock N Roll: one public ZIP at a
fixed URL.

- **Same mechanism as runtimes.** A game's content is an ADR 0012 artifact:
  fixed URLs and SHA-256 values in plugin source. Search, claims and
  resolve-download stay out, as ADR 0012 says.
- **The files stay in the cache.** The unpacked content stays in the
  artifact's ready directory under `<storage-root>/cache/`. korrid does not
  copy it into a source. Legacy did copy it
  (`legacy:product/platform/acquisition/artifact-placement.ts`); this decision
  departs from that so korrid never writes into folders a person owns.
- **The game is in the library before the download.** It shows that it needs
  a download. The first launch uses the existing Confirm, progress and
  unpacking flow that runner artifacts already use.
- **No plugin-shape rule.** A plugin may declare any mix of games, runners and
  artifacts. The mechanism must not assume one plugin per game.

**Owner decisions, 2026-10-20, second round.**

- **More than one.** One artifact can hold several ordered files (ADR 0012).
  One release can need several artifacts, the same way a runner's list does.
  Each version of a game is its own release, with its own artifacts. This
  follows the glossary: a release is one version of a game's content.
- **Identity stays the content bytes.** The release still lists hashes, and
  they are hashes of its main content file (Mega Man Rock N Roll: `data.pck`,
  not the ZIP). A copy that discovery finds in a source joins the same game,
  as prepared games do under ADR 0003. Discovery rejects archive suffixes, so
  a ZIP hash would never join anything.
- **Other plugins' artifacts.** A declared game may name another plugin's
  artifacts if that plugin is enabled. This is the rule runners already use.
- **The runner gets `contentPath`.** The release names one file inside a
  ready artifact directory. korrid passes that path as `contentPath`. A runner
  then plays a downloaded copy and a copy from a source in the same way.
- **No saved location.** Before and after the download, the library holds
  only the game and its release. The route comes from the plugin's
  declaration, and readiness comes from the cache directory, as ADR 0012
  says. The URL lives only in plugin source. Legacy's `Url` location kind is
  not used, so a saved URL cannot drift from the plugin's pin. Cost: route
  resolution gains one case that has no saved location.

**Owner decisions, 2026-10-20, third round.**

- **Default version: declaration order.** The plugin lists its preferred
  version first, and korrid offers releases in that order. This order must be
  specified and tested, because it comes from map order in plugin source.
- **A dropped version stays in history.** When a plugin update stops
  declaring a version, its release stays in the game's history as
  unavailable. Its cache directory stays until a person removes it. korrid
  adds no cache cleanup.
- **One Confirm.** The first launch asks once, with the total size of the
  runner's artifacts and the release's artifacts. The launch waits until all
  of them are ready. `app.acquisition.status` must report the release's
  artifacts as well as the runner's.
- **Download without launching.** A person can download a game from the game
  without starting it, for example before a flight.

**Owner decisions, 2026-10-20, fourth round.**

- **`file` takes one of two forms.** A declared release's `file` is either a
  path relative to an account root, as today, or
  `{ artifact: "<artifactId>", path: "<path in its ready directory>" }`. The
  release also gets `artifacts: [ids]`, which is the field name runners
  already use, and `file.artifact` must be one of those IDs. `file` keeps
  one meaning: the release's content file. The form of the value says which
  folder the path starts from, so no value is parsed. In Rust this is an enum,
  so code that reads `file` does not compile until it handles both forms.
  This rule is the reason: discovery hashes `file` under every account root
  on every pass (`discovery/identify/mod.rs`), and an artifact path read as
  an account-root path would fail without any error. Rejected: a
  `<artifactId>/<path>` prefix string (it must be parsed, and a missed check
  fails silently), and a new `content` field (`systems` already uses
  `content` for content rules).
- **The Download action fetches the whole default route.** That means the
  release's artifacts and the runner's artifacts, so the game then plays
  offline.
- **Downloads continue in the background.** A download that the Download
  action starts continues when the person leaves the game. The game shows its
  progress and a Cancel action when the person comes back. The launch gate
  keeps its current behavior.
- **Remove download.** A person can remove a downloaded game to free space.
  korrid never removes an artifact that something else still needs. Saves in
  the account root are never removed.

**Owner decisions, 2026-10-20, fifth round.**

- **Remove download removes only the release's own artifacts.** It never
  removes a runner's artifacts, such as the FEX rootfs that Steam also uses.
  A large runtime stays on disk after the last game that used it is removed.
- **No removal of a game in use.** korrid refuses to remove a game while it
  runs. While the game downloads, the person gets Cancel instead.
- **Background downloads continue after a korrid restart.**
- **A download blocks deep sleep.** The screen can turn off (light sleep),
  but the device does not suspend to RAM while a download runs. korrid has no
  sleep inhibitor today, so one must be added.
- **One download at a time,** in the order the person asked.

**Owner decisions, 2026-10-20, sixth round.**

- **Cancel deletes partial files.** This amends ADR 0012, where Cancel kept
  the `.part` files. A `.part` file with no ready directory now always means
  "interrupted", so after a restart korrid continues those downloads, oldest
  `.part` file first. No new saved state is needed, which keeps ADR 0012's
  rule that state comes from the cache directory. Cost: a cancelled download
  starts again from zero.
- **A launch goes first.** A download that a launch is waiting for runs
  before the queue. The queued download pauses and later continues with a
  `Range` request.
- **Shared artifacts stay.** Remove download keeps any artifact that another
  release in this device's library still names.

## Considered Options

- **Wait for the source layer.** Rejected: it blocks every pinned download on
  claims, search and provider health, which no current case needs.
- **Copy into a source, then discover (legacy).** Rejected: korrid would write
  into person-owned folders and need a rule for choosing the source. ZIPs must
  be unpacked first, because discovery rejects archive suffixes.

## Consequences

Game content is treated as reconstructible. Deleting the cache (the ADR 0012
operator step does this) and losing the author's URL loses the game on that
device. Saves stay in the account root and are not affected.

A declared game whose release comes from artifacts needs no system. Its
runner can accept it by content hash through `runners.<key>.releases`, as
SCRIPTING.md already allows for a release without a system. So Mega Man Rock
N Roll needs no new system ID.
