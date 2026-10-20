# Enabled Steam plugins discover provider releases from their own manifests

## Decision

This amends ADR 0003's prohibition on discovery in plugin data. The exception
is only provider releases described by the closed first-party manifest table in
`services/korrid/src/plugin_discovery.rs`. Its one entry maps
`@korri:steam/installed-apps` to `steam/steamapps/appmanifest_*.acf`, relative to
each existing account root. This is the Steam home already produced by
`plugins/steam/plugin.ts` in `korri-os/plugins` (`${accountRoot}/steam`), not
a new storage layout.
Paired-source discovery still excludes plugin data. Prepared games still use
ADR 0003's `games` declarations and hashes. Build trees are never scanned.

A plugin exports `discovery` as a list of `{id, title}` data records and puts
its pure discovery function in `handlers["discovery.discover"]`. Admission
rejects an unknown provider, a foreign owner, duplicates, an empty title, or a
missing handler. Both administrator admission and korrid use the same closed
vocabulary. Only an enabled owning plugin runs discovery. This is a deliberate
first-party limit, like `SessionControlEffect`, not a general capability model.
Other plugins cannot discover content in their own data until another real case
justifies extending the contract. A Steam plugin that uses this export needs
the updated administrator and daemon before it can be installed.

korrid enumerates only that manifest directory. It opens directory components
and files with descriptor-relative `O_NOFOLLOW`; symlinks and special files do
not grant access. It reads at most **4,096 manifests per pass**, and at most
**65,536 bytes per manifest**. Files over either cap are skipped with daemon
stderr diagnostics and discovery diagnostics. Directory enumeration is bounded
at 10,000 entries; an over-limit or unreadable directory leaves previous
provider locations unchanged. Names are sorted before the file cap is applied.
A missing directory is an empty installed set. These are implementation bounds,
not persisted configuration. Changing files can produce a non-atomic scan;
the next rescan settles it, as with source files today.

Each synchronous handler call receives `{files: [{relativePath, text}]}` with
one manifest. `relativePath` is relative to the Steam home, as in legacy.
Even worst-case JSON escaping fits the existing 512 KiB callback input bound.
The sandbox has no filesystem callback or I/O. korrid validates the strict
observation shape, numeric AppID matching the manifest filename, the owner's
provider ID, non-empty title, source path, release `steam`, system `steam`, and
current runner `@korri:steam/steam`. Invalid output fails the provider scan and
preserves prior locations; it is not evidence of uninstall.

korrid publishes through the existing reconciliation and atomic document writer.
It writes the existing provider library identity and provider-ref device
location; no new persisted fields, source records, scan-root configuration, or
catalog files are introduced. The existing private `origins` journal owns only
locations this scanner creates. A complete pass withdraws absent apps' owned
provider locations, including when the plugin is disabled, and removes this
scanner's origin claim in the same publication. Games and release history stay
unavailable, as missing file releases do today. Authored locations are not
claimed or removed: a person can author the same location after withdrawal,
and later scans preserve it with Steam enabled or disabled. A reinstall restores
a missing location without making a second game; an existing authored location
stays authored. The existing daemon-start scan and `app.discovery.rescan` run this
same path. No watcher or device work is part of this decision.

## Legacy grounding and deliberate deltas

- `legacy:product/platform/plugin/discovery.ts` defines
  `ReleaseDiscoveryProvider {id, title?, discover}`, the `files`/`readText`
  context, and `provider-ref-release` observations. We retain the provider
  list and observation `kind`, `confidence`, `target`, `release`, and `launch`
  names. Code moves to main's existing handler mechanism, and pre-read text
  replaces `readText`, because plugins must remain effect-free. `source`
  retains only `relativePath`; absolute paths and storage descriptors are not
  needed to identify or launch a provider release. This consumer accepts only
  `high` confidence and has no evidence consumer, so it does not carry optional
  evidence or the other legacy observation kinds.
- `legacy:product/plugins/steam/src/discovery.ts` and `discovery.test.ts` supply
  the manifest recognition, matching AppID, installed StateFlags 4, missing-type
  allowance, and exclusion of every explicit type other than `Game` (including
  tools, Proton, Steam Runtime and Steamworks redistributables). Titles come
  from manifest `name`. Unlike legacy's `Steam App <appid>` fallback, a missing
  or empty name is skipped: this slice requires actual manifest titles.
- `legacy:product/plugins/steam/src/state-materializer.ts::parseVdf` supplies
  the quoted-string, escape, comment, and nested-object VDF parser. The port
  adds a 64-level nesting bound and null-prototype records.
- `legacy:product/platform/library/discovery/release-candidate-scan.ts`
  supplied manifests and readText to providers from configured storage roots;
  the Steam plugin's `src/plugin.ts` declared Steam storage at its state root.
  Its `release-candidate-scan.test.ts` case “honors configured max-depth scan
  hints for manifest-only storages” attaches `@korri:steam/installed-manifests`
  to `steamapps` with `scan.max-depth: "1"`. The closed table keeps that
  manifest-only scope without restoring broad configured-root scans or their
  storage configuration. It uses main's existing Steam home.
- ADR 0002, `config/library.rs`, `config/catalog.rs` and `Location::ProviderRef`
  already identify and locate `@korri:steam/<appid>` provider releases. Unlike
  file locations, provider locations have no discovery timestamps: removing
  the location expresses unavailable history without changing that schema.
  Existing authored game titles remain authored rather than being overwritten.

## Cost and limits

Only the primary Steam home is scanned. External Steam libraries and
`libraryfolders.vdf` are not followed. A skipped oversized or over-count
manifest cannot make an app available and can withdraw its previous owned
location. Each file starts a fresh interpreter, so large libraries cost scan
time. The handler cannot discover addons, file releases, or new storage roots.
This does not provide install progress, automatic filesystem watching, or a
capability model. The core changes and the separately published Steam plugin
must ship together; do not change `korri-plugins` from this worktree.
