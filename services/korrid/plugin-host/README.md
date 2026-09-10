# Administrator-approved Linux plugins

The host installs previously unknown plugins without a NixOS update or a device build. It accepts one declared daemon per plugin. It does not accept raw unit files, installation scripts, or new Linux permission kinds.

## Host installation

Import `nixosModules.korri-plugin-host` and enable `services.korri.pluginHost.enable` in the base system. This installs the generic CLI, boot recovery, TUN support, and the download-only policy. It preserves the stock NixOS cache and key. No option names a plugin.

The module composes `nix/device-cache/nixos-module.nix`, which sets `max-jobs = 0` and disables remote builders. The importer realizes an exact output with `nix build --no-link --extra-substituters SOURCE OUTPUT`. Despite the command name, it forces `max-jobs = 0`, empty `builders`, `fallback = false`, and `require-sigs = true`: no local or remote builds are allowed. It validates the exact `/nix/store` output before invoking Nix; derivations, flakes and expressions are rejected. Nix can download dependencies from the host's configured trusted caches, including `cache.nixos.org`, rather than requiring the plugin cache to duplicate them. Recursive signature verification consults the same caches, including for already-present paths without locally registered signatures. Verification and store filesystem synchronization still complete before approval.

Core exports `packages.<system>.korri-plugin-host`, including the generic Rust host and publisher. Production Tailscale packaging and publication belong to [korri-os/plugins](https://github.com/korri-os/plugins). Core retains only a test fixture under `tests/fixtures/tailscale`, copied from that real package and declaration with the same `@korri:tailscale` identity and unchanged nixpkgs binaries. The fixture is not a public package or a distributable plugin. Live publication and the official HTTPS destination still need owner approval. General core binary caching is separate from plugin cache publication.

## HTTPS repositories

Set `services.korri.pluginHost.officialCatalogUrl` to the owner's real HTTPS catalog URL. The module writes `/etc/korri-plugin-host/official-catalog-url`. There is no default address. `repository list` reports missing official configuration explicitly; raw-cache commands and local recovery do not read this configuration. Downloaded metadata cannot claim official status. The official source can only be changed through trusted host configuration.

Adding a source trusts that HTTPS host. Sources have no priority; identical plugin IDs in different catalogs remain separate choices. Every release selection uses the running host's Nix platform (`x86_64-linux` or `aarch64-linux`). Release labels are explicit, not sorted versions.

| Command | Effect |
|---|---|
| `repository add URL` | Validate an HTTPS catalog, then durably add its normalized URL. Repeated addition is idempotent. |
| `repository remove URL` | Remove a user-added source. Retain installed software, provenance and private data. |
| `repository list` | List the configured official source distinctly from user-added sources. |
| `repository catalog URL` | Fetch a configured source's strict catalog without importing or evaluating plugin source. |
| `repository inspect URL ID RELEASE` | Verify and import the selected release, then show its provenance, declaration, effective policy and approval digest. |
| `repository install URL ID RELEASE APPROVAL` | Refetch and verify that exact selection; commit an approved installation, initially disabled. |
| `repository update ID RELEASE APPROVAL` | Resolve only the source stored in the installed receipt. Preserve desired state. Never try another source. |
| `repository switch ID URL RELEASE APPROVAL` | Explicitly choose another configured source after inspecting it. Preserve desired state; reject approval from the old source even for identical package bytes. |

Review `warning`, `provenance`, `declaration`, and `unit_configuration` before copying the approval digest. Inspection is required again when the release, archive bytes, package, source or effective execution policy changes. There is no automatic update or blanket approval. Switching a raw-cache installation to a repository also uses `repository switch`; raw `update` cannot change a repository installation's origin.

Acquisition validates certificates and follows at most three HTTPS redirects. HTTP downgrades, URL credentials, curlrc and netrc credentials are disabled. Catalog transfers allow 30 seconds and 4 MiB; archive transfers allow 180 seconds and less than 2 GiB. Connection setup allows 10 seconds within the total deadline. The host measures the actual archive SHA256 **before** extraction. It checks confined extraction, typed Nix closure metadata, compressed file sizes/hashes and actual bounded xz/NAR hashes through `archive::preflight_cache`. Only this private local file cache reaches the existing Nix importer. Recursive `--sigs-needed 1` verification remains active. No downloaded Nix options, builds, flake evaluation or signature bypass are accepted.

A repository archive must now also identify an output signed by the namespace's bound key. Unsigned content-addressed archives alone are refused. If the archive omits that signature, the bound cache must supply matching signed metadata; the catalog URL is not signing authority.

The cost is a complete per-release archive and temporary disk use: up to 2 GiB downloaded plus 3 GiB extracted compressed cache, in addition to imported store paths. NAR validation permits at most 3 GiB of actual decompressed bytes. Downloads and validation hold the exclusive host lock, so concurrent commands fail as busy. A lost connection requires a new download; there is no partial resume.

## Raw-cache installation and approval

The existing explicit raw-cache route is separate from repository selection. Use the exact output published for this device architecture. The plugin cache can host only custom outputs; the rest of the closure must already exist locally or be available from configured trusted substituters. Every path requires a trusted signature for ordinary input-addressed paths or a verified content-addressed identity. A missing or untrusted dependency fails installation without a build. Adding the plugin cache does not add signing keys or change source identity or approval. The cost is dependence on configured cache availability: Nix can fail an import when it cannot initialize a cache, even if another cache has the needed output. HTTPS repository archives still require their complete content-addressed closure during preflight; this importer change does not change the archive or publisher contract.

```sh
sudo korri-plugin inspect "$CACHE_URL" "$PACKAGE"
```

Read `warning`, `declaration`, and `unit_configuration` in the report. Copy its `approval` value only after reviewing that access.

```sh
sudo korri-plugin install "$CACHE_URL" "$PACKAGE" "$APPROVAL"
sudo korri-plugin enable @korri:tailscale
```

Install leaves the plugin disabled. Approval binds to the exact store path, `plugin.ts` bytes (including any `launch` function), manifest bytes, explicit input provenance, declaration, base policy, and complete rendered unit. Repository approval never includes the disposable staging/cache path. Two sources serving identical bytes have different approvals. There is no blanket `--yes` grant.

The device binds each publisher namespace to one full Nix public key and one exact cache URL through `services.korri.pluginHost.publishers`. Each binding has `publicKey` and `cacheUrl`; the module writes `/etc/korri-plugin-host/publishers.json` and adds those keys to Nix's trust list. A non-NixOS administrator supplies that root-owned file and configures the same Nix keys. Missing or malformed configuration fails closed. An empty map permits no external plugin.

The immutable package contains `plugin.ts` and a generated `manifest.json`. Its only identity field is `publisher.namespace`, grounded in the approved authoring brief. The package producer supplies that value from trusted publisher composition; `tests/fixtures/tailscale/package.nix` is the build-side fixture. No manifest key is authoritative. The host verifies the actual selected NAR contents and checks an Ed25519 signature over Nix's fingerprint (path, NAR hash, NAR size, sorted references) with the bound full key. A matching signature label, another globally trusted signer, or content-addressed identity alone is insufficient. Raw-cache inspection also requires the bound cache URL.

When a local store image lacks signatures, the host fetches metadata only from the bound cache, checks it against the local NAR fingerprint, and retains verified signatures in Nix's own metadata. Later offline activation checks that full key again. This can require one metadata request during initial inspection. Removing a binding or replacing its key prevents new starts of old packages, including rollback and boot recovery. It does not revoke the exact installed receipt's authority to stop and clean up. Republish selected builds and cut over bindings operationally; there is no runtime migration.

Namespace verification does not authorize permissions. The administrator still approves the exact package. This is a clean cut: old completion-value plugins and packages without a manifest cannot be installed. Production publication in `korri-os/plugins` must adopt the manifest and named exports separately; this core slice changes its test fixture, not that repository.

## Lifecycle commands

| Command | Effect |
|---|---|
| `inspect CACHE PACKAGE` | Import and verify the signed closure, then show the bounded declaration and effective policy. Run no payload. |
| `install CACHE PACKAGE APPROVAL` | Record an approved package, initially disabled. |
| `update ID CACHE PACKAGE APPROVAL` | Update an existing raw-cache installation of that ID. Preserve enabled state. Restore the prior selection after a failed start when cleanup succeeds. |
| `enable ID` | Start the approved daemon. |
| `disable ID` | Stop and clean up the daemon. Retain private data. |
| `remove ID` | Stop, clean up, and release the package and approval. Retain private data. |
| `remove ID --purge` | Also ask systemd to delete the plugin's private state. |
| `status ID` | Show the committed package, provenance, approval, and desired state. This is not a live health report. |
| `unit ID` | Print the managed systemd unit name for status and journal commands. |
| `restore` | Reconcile interrupted operations and restore enabled daemons. Leave matching healthy daemons running. |

The generic boot service runs `restore`. Every command opens the same exclusive host lock, including source writes. Local `status`, `enable`, `disable`, `remove`, and `restore` use the receipt and immutable store; they do not need a configured or reachable repository. Pending and active package symlinks are Nix GC roots. The host commits a new package or enable selection only after its service operation succeeds. If the device loses power before that commit, boot restores the previous selection only if its publisher is still authorized. Disable and removal record their intent before cleanup, so an error or crash cannot silently re-enable them. They replace a pending enabled operation without first restarting the previous selection. A prior removal keeps its original purge choice.

Deactivation still checks the receipt's exact package, source bytes, manifest, provenance, identity and effective policy against its approval. It skips only current publisher authorization, not approval equality. The existing managed unit may run its already approved `ExecStopPost`, including cleanup from an approved pending update; there is no general cleanup-command bypass. Purging an inactive plugin renders that exact approved unit for systemd's state deletion without starting its daemon. Missing or malformed publisher configuration still prevents opening the host; remove a namespace from the valid binding map to revoke it.

Recovery stops a revoked enabled unit before refusing its restart, even if it was healthy. It retains the receipt and GC roots on refusal. This is not a live revocation watcher: a running daemon is stopped by disable, remove or restore, not by the configuration edit alone. If disable or removal reached disk before a crash, recovery only completes cleanup and optional purge; it never starts that daemon.

If cleanup fails, disable, removal and automatic rollback stop. The receipt and package remain available for inspection. Read the unit journal. Do not delete the roots to force success. A failed native cleanup can need separate administrator repair; the CLI does not pretend to reverse arbitrary native effects.

## Declaration and storage grounding

`src/declaration.rs` consumes named exports `name`, optional `title` and `description`, and `daemons`. The publisher namespace comes from verified packaging, not source. ID segments follow the grammar in `services/korrid/src/plugin.rs`, with a 64-byte bound.

For this module/identity slice, `daemons` retains the internal systemd fields exercised by Tailscale: `Type`, `ExecStart`, optional `ExecStopPost`, and `CapabilityBoundingSet`. Only `notify` and `exec` service types are supported. The only nonempty capabilities are `CAP_NET_ADMIN` and `CAP_NET_RAW`. Unknown fields, explicit nulls, duplicate capabilities, and executable path traversal fail.

Commands name `bin/<program>` inside the immutable package. Their arguments can use systemd's `STATE_DIRECTORY` and `RUNTIME_DIRECTORY` names. The host substitutes its own directories. It never passes untrusted unit directives, environment variables, specifiers, or a shell command string to systemd.

`src/unit.rs` supplies `DynamicUser`, private state, read-only system access, device restrictions, and restart supervision. The payload never runs as root. Host-network administration still grants substantial control of routes and firewall rules. It is not confined to a named interface.

The host's `StateDirectory` convention grounds `/var/lib/korri-plugin-host`. `src/host.rs` writes one `selection.json` receipt per hashed ID. Its fields come from the imported package, approval report, and requested lifecycle state. `Disabled`, `Enabled`, and `Removed` are desired states. The latter retains the explicit purge choice during recovery. The native systemd unit name also determines private state and runtime directories. `src/package.rs` is the sole name producer. The report exposes the resulting paths.

The GC roots use Nix's existing `/nix/var/nix/gcroots` convention. `active` pins the committed package. `pending` pins an uncommitted candidate. Runtime units live in systemd's `/run/systemd/system` directory and disappear on reboot.

The CLI uses korrid's actual evaluator from `services/korrid/src/script.rs`. That interpreter now limits source size, execution time, memory, and stack. Declarations must be deterministic. A changed evaluation cannot match a stored approval.

## Catalog, source-state and receipt contract grounding

`src/catalog.rs` is the shared strict Rust/Serde contract for publisher and reader. `Catalog` contains `records`; unknown fields and repeated `(plugin_id, release_version, platform)` records fail. Optional title/description may be absent but not null. No official flag, permissions, approval, channels or version ordering are accepted.

| Field | Existing producer grounding |
|---|---|
| `plugin_id` | `Declaration::id()` combines the manifest publisher namespace with the named `name` export. The imported declaration must match the selected ID. |
| `title`, `description` | The existing optional fields in `plugin.ts`; the publisher loads the real declaration through `package::load_declaration`. |
| `release_version` | The explicit publisher CLI release label, independent of upstream binary version. |
| `platform` | The Nix build system string supplied to the publisher. The consumer selects its compiled Nix system, not one supplied by the catalog caller. |
| `store_path` | The exact output returned by Nix `store make-content-addressed`. |
| `archive_url` | The publisher's explicit HTTPS asset destination. It is only a download location, never a Nix cache URI/options string. |
| `archive_sha256` | SHA256 measured over the publisher's actual tar bytes. The consumer measures the downloaded file independently. |

`src/provenance.rs` adds the required `provenance` sum type to the existing report and receipt. Its `kind` is exactly `RawCache` or `Repository`. `RawCache.cache_url` is the existing raw CLI's CACHE argument. `Repository.source_url` is the explicitly configured, normalized catalog URL; `plugin_id`, `release_version`, `platform`, and `archive_sha256` come from the selected catalog record above. The surrounding receipt retains its existing `id`, `package`, `approval`, and `desired` fields. Package and provenance commit in the same atomic `selection.json` write; failed updates and interrupted source switches restore both together. No catalog reachability or source registration is needed to validate the committed approval locally.

This is a clean receipt cut. A receipt without `provenance` fails deserialization and remains unchanged. There are no source guesses, compatibility reads or runtime migrations. Existing development receipts need an explicit one-off operational decision before deployment; this change performs none.

`src/storage.rs` owns host-root policy and the lock. Under the existing state root, `sources/list.json` contains only `sources`: the normalized URLs produced by successful `repository add` commands, in insertion order with no duplicates. Both reads and writes cap the serialized JSON at the existing 128 KiB reader bound. Stored malformed, noncanonical or duplicate URLs fail without replacing the list.

The same storage owner creates private `staging/download-<8 alphanumeric characters>` directories. The suffix comes from tempfile's random directory producer, not metadata. Success and ordinary failures remove their own staging directory; acquisition and restore remove owned stale directories under the host lock. Cleanup rejects links or unrecognized entries rather than deleting arbitrary paths. Restore recognizes only the locked `sources` and `staging` directories in addition to its existing receipt directories and lock. Unexpected entries still fail, while errors from one receipt or staging entry do not prevent recovery of other receipts.

## Verification

```sh
nix build .#checks.x86_64-linux.korri-runtime-plugin-host
nix develop .#plugin-host --command cargo test --manifest-path services/korrid/plugin-host/Cargo.toml
nix develop .#plugin-host --command cargo clippy --manifest-path services/korrid/plugin-host/Cargo.toml --all-targets -- -D warnings
```

The VM starts without the Tailscale package. A second fixture VM serves signed raw caches, real HTTPS catalogs and complete content-addressed archives produced by the prebuilt publisher. The cold client never publishes or builds. The test disables automatic test-script closure injection, so the client must actually import the package. It proves signature refusal, explicit approval, installation, service readiness, a TUN interface, independent plugin updates, failed-start rollback, power-loss recovery, data retention, purge, cleanup-failure refusal, and an unchanged NixOS generation. No compiler is on the client command path. The extended gates also cover two sources sharing an ID and identical bytes, same-source update, cross-source approval rejection, changed catalogs, failed/interrupted source-switch rollback, source removal with offline lifecycle, and owned stale-staging cleanup. These new VM gates must be run before claiming end-to-end verification.

The revocation VM gates remove a publisher binding and rotate its full key. Both deny enable, permit disable and removal with real Tailscale cleanup, retain data on disable, and purge it on removal. They reject a changed receipt approval, cover pending-operation and persisted-disable recovery boundaries, stop a revoked healthy daemon during restore, refuse revoked restarts, and retain receipts and roots after native cleanup failure.

The split-cache VM gate removes a dependency's NAR and narinfo from the plugin cache. An independently signed, configured upstream supplies it through the real `inspect` and `install` commands. Missing and untrusted upstream cases fail without running an available build canary, even with permissive ambient Nix settings. A later inspection without the dependency's trusted key also fails after the output is already present. A separately seeded unsigned local dependency requires a trusted upstream signature before inspection succeeds. The gate checks unchanged raw-cache provenance, explicit approval, activation and removal. It uses local fixture caches, not a live `cache.nixos.org` connection.

The Nix devshell and package check both supply `KORRI_TEST_CURL`. The real local TLS tests fail if it is absent; none silently return. One explicitly ignored test is a subprocess entrypoint invoked by the ambient-credentials test. Regular package and HTTPS tests remain sandboxed.

`korri-publisher-check [STORE_PACKAGE]` runs the ignored real publisher integration tests on a build machine with a writable Nix store. The optional argument is an already built absolute `/nix/store` package containing the actual `@korri:tailscale` declaration and binaries for the app's system. With no argument, it uses the core test fixture. For example, a consumer can invoke `korri.apps.<system>.korri-publisher-check.program` from its pinned core input with its own package path. The app uses core's immutable host source (including the shared `src/script.rs`), locked Rust/C toolchain and Nix, not the caller's Git root or flake. It copies that source into a temporary writable workspace, sets `CARGO_TARGET_DIR` there, and removes the workspace on exit or handled interruption. Cargo uses `Cargo.lock`; uncached dependencies can require downloads. The cost is a fresh Cargo target and full closure conversion per run. It must never run on a download-only device.

The plugins repository can also import `services/korrid/plugin-host/vm-test.nix` from pinned core. Its existing arguments remain `pkgs`, `hostModule`, `hostPackage`, and `tailscalePackage`. Supply the actual Tailscale package; the update fixture reads that package's `plugin.ts` and binaries. Core's check supplies its test-only package instead. Both run the full cold-host, HTTPS, Headscale, lifecycle and failure tests.

The VM uses real Tailscale 1.90.9 from the pinned nixpkgs. It first reaches `NeedsLogin`, then joins a disposable local Headscale network over TLS. Real IP traffic survives package update and a forced reboot. No owner's credentials or public tailnet are used. The test opts out of host DNS changes with `--accept-dns=false`. It does not prove host DNS integration, physical-device behavior, or remote streaming. The second release changes the plugin package, not Tailscale's upstream binary version.

The crash test exposed imported store data that was not durable when the receipt committed. The importer now synchronizes the store filesystem before approval. This can delay installation while other store writes finish. Another regression covers interruption after writing a unit but before starting it. Restore distinguishes unexecuted cleanup from cleanup that ran and failed.
