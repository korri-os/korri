# korrid acquires declared artifacts

**Owner decision, 2026-10-19.**

Supersedes ADR 0010's exception (docs/adr/0010-plugin-runtimes-download-into-the-shared-cache.md). Plugins no longer download. A plugin declares artifacts; korrid downloads, verifies, resumes and places them, and reports progress to the portal. Plugins keep one native prebuilt program per artifact that unpacks verified files, because unpacking is format-specific (squashfs, zstd overlays, xz with --strip-components, a toolmanifest.vdf edit) and "native programs arrive prebuilt" (AGENTS.md, plugin boundaries).

## Decision
- A plugin exports `artifacts`, keyed like `runners`, each with `id`, `version`, an ordered `files` list of `{ url, sha256, bytes? }`, and an optional `unpack` bare file key.
- A runner lists the artifact ids it needs in `RunnerRecord.artifacts`. korrid resolves them before `app.session.start` is allowed; it never starts a runner whose artifacts are not ready.
- korrid places verified files at `<storage-root>/cache/<plugin>/<contribution>/.downloads/<sha256-hex>` (partial: `<sha256-hex>.part`), runs `unpack` in a work directory under the same contribution directory (`.tmp-*`), and renames it atomically to `<storage-root>/cache/<plugin>/<contribution>/<version>/`. Ready means "the version directory exists". No journal: state is derived from the cache directory, exactly as fex-prepare does today. After a successful rename korrid removes `.downloads`.
- `<plugin>` in the cache path is the plugin's short name (`fex`, `proton`), `<contribution>` is the artifact key (`rootfs`, `cachyos`), as in ADR 0010: `cache/fex/rootfs/archlinux-2026-01-08-mesa-26.1.2/`, `cache/proton/cachyos/11.0-20260601-slr-arm64/`.
- The runner receives ready paths in `PluginLaunchInput.artifacts`, keyed `<pluginId>/<key>` (own plugin: bare key), the same grammar as `PluginLaunchInput.files` (services/korrid/src/plugin.rs ~589-614).
- Progress is polled over local `/rpc`, like `app.session.status`.
- Source acquisition (search, claims, resolve-download, browser hand-off, provider health) is OUT. The `artifacts` shape is the "known URL + hash" half of legacy's `FinalDownload`; a later source layer produces the same shape at runtime.

## Grounding

| Element | Comes from | Delta and why |
|---|---|---|
| `artifacts.<key>` export, `id: "<pluginId>/<key>"` | `runners.<key>` with `id: "@korri:steam/steam"` (plugin.rs); contribution-id grammar in ADR 0010 | None |
| `version` | plugins/fex/rootfs/pins.nix `version`, plugins/proton/cachyos/pin.nix `version`; ADR 0010 cache segment | None |
| `files: [{ url, sha256, bytes? }]` ordered | pins.nix `base` then `overlay[]` (order matters); pin.nix `url`/`sha256` | Base and overlays merge into one ordered list; the unpack program gets them in order. `sha256` is written `sha256:<64 hex>` like `games.releases.hashes` (one hash grammar in korrid; plugin.rs `is_declared_hash`). |
| `bytes?` per file | legacy `size` on AcquireArtifactRequest/DownloadChoice; ADR 0010 measured sizes | number, not string. Used to show the Confirm size before any network call. |
| `unpack?` bare file key | `RunnerRecord.program` bare key validated by plugin_references.rs; the unpack halves of fex-prepare and cachyos-prepare | The prepare program is split; its download half moves to korrid. Invoked `<unpack> <work-dir> <file>...` in declared order; exit 0 ready, 1 failed; stderr diagnostics; no stdout contract (korrid owns the rename). |
| `RunnerRecord.artifacts: string[]` | `runners.<key>.program`/`core` references; `requires` in the package manifest | New field. Explicit beats "every artifact of every required plugin". |
| `PluginLaunchInput.artifacts: Record<string,string>` | `PluginLaunchInput.files` keying | New field, same grammar |
| Cache paths, `.downloads/<sha256>.part`, `.lock` | ADR 0010 verbatim | `.lock` is korrid's per-contribution exclusion (in-process mutex plus the file, so an unpack program and a concurrent korrid never collide) |
| Artifact state `Missing / Downloading / Unpacking / Ready / Failed` | legacy job states `acquiring / staged / imported / failed` | Renamed to what the user sees; `Unpacking` is new |
| Progress `bytesDone, bytesTotal?, bytesPerSecond?` | legacy pico Downloading mockup ("154 / 248 MB", "6.4 MB/s"); nothing on legacy's wire | done/total in bytes plus a rate sampled over the last 5 s; the host derives ETA |
| RPC `app.acquisition.status`, `.start`, `.cancel` | legacy `app.acquisition.acquire`/`acquire-status`; main's `app.session.status`/`reserve`/`cancel` | keyed by artifact id (the id already dedupes, like legacy `activeByClaim`) |
| Download policy | legacy url-policy.ts, safe-fetch.ts; fex-prepare curl flags | http/https only; no embedded credentials; SSRF block list (localhost, 0/8, 10/8, 127/8, 100.64/10, 169.254/16, 172.16/12, 192.168/16, ::1, ::, fe80::/10, fc00::/7, 64:ff9b::/96, ::ffff:0:0/96); manual redirects, max 5, no https->http; resume with `Range`, restart the file on 416 or hash mismatch; stall = under 1 KiB/s for 60 s fails; connect timeout 30 s; 5 retries 2 s apart; NO size cap (ADR 0010); magic-byte sniffing dropped (the pinned SHA-256 subsumes it). |
| `SurfaceStatus.Busy.progress?: { done, total? }` | `Busy.detail` and `SurfaceAction`; legacy DownloadProgress organism | New optional field; the host still writes `detail` prose |
| `SurfaceStatus.Confirm` variant | legacy DownloadConfirm page (A: Download, B: Cancel); `SurfaceAction` | `{ _tag: "Confirm", kicker, detail, actions, gameId? }` |
| `SurfaceGame.download?: { bytes? }` | legacy DownloadConfirm SIZE stat | New optional field so a game can show "needs 1.5 GB download" before launch |

Not carried: Repair and UpdateAvailable screens (mockup only; a ready directory is immutable and a new version is a new directory); Installed screen (launch continues straight after ready); HTML sniffing; XDG staging root; claim store; provider health; validate-providers.

## Considered Options

- **Keep prepare programs and parse their stderr for progress.** This leaves
  the effect in the plugin, contrary to AGENTS.md. Every plugin reimplements
  resume.
- **Unpack natively in Rust.** This needs squashfs, zstd, xz and tar support,
  plus a hook for the toolmanifest.vdf edit.
- **This decision.** korrid owns download, verification and publication. A
  sandboxed prebuilt plugin program owns format-specific unpacking.

## Consequences

`steam-arm64-seed` still downloads Valve's unpinned Steam client with curl;
that bootstrap is outside the declared artifact mechanism and this decision's
scope.

The first Steam launch shows Confirm with "1.5 GB", then a real progress bar, then unpacking, then starts. fex-prepare, cachyos-prepare, pins.nix, pin.nix, KORRI_FEX_PREPARE and KORRI_PROTON_CACHYOS_PREPARE go. steam-session reads KORRI_FEX_ROOTFS and KORRI_PROTON_CACHYOS. The plugin-host interface changes; `korri-published-plugins-check` reports the break by design and published plugins are republished with core. A partial download survives a korrid restart on disk and resumes. Downloads run on the launching device only; the capability model stays unbuilt.


### One-off deployment cut

End live sessions and stop the deployed korrid before replacing the old prepare
programs. Delete only their reconstructible cache:

```sh
sudo rm -rf -- /var/lib/korri/cache
```

Then activate the new product module and restart korrid. For a configured
storage root other than `/var/lib/korri`, use that root's `cache` directory.
Do not delete `users`, saves, library content, or private daemon state.

This is an operator action, not a daemon migration. Old prepare-owned version
directories would otherwise be mistaken for Ready. The cost is downloading the
pinned runtimes again (about 1.5 GB). The new cache belongs to korrid; the runtime
user receives only read/traverse ACLs. `.downloads` is mode 0700. Unpack units run
as korrid's user, with the contribution directory as their only writable path.

## Wire clarification

`ArtifactsNotReady` is an existing-shape `RpcFailure`: its `code` is
`ArtifactsNotReady` and its `message` is plain copy. It adds no `artifactIds`
field. The portal obtains the artifact IDs from
`app.acquisition.status { gameId, runnerId? }` before starting a session.
An explicit `runnerId` uses the same selected-runner resolution as
`app.session.start`; omission retains the saved/default route rules. This
clarification was approved during implementation on 2026-10-19.

## Amendment

[ADR 0013](0013-declared-games-can-come-from-artifacts.md) lets declared game
releases name artifacts, and changes Cancel: it now deletes the `.part` files,
so a `.part` file with no ready directory always means an interrupted download.

[ADR 0014](0014-korrid-acquires-games-from-provider-plugins.md) amends "Source
acquisition (search, claims, resolve-download …) is OUT" for search provider
plugins; their downloads stay in this cache grammar.
