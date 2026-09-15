# Administrator-approved Linux plugins

The host installs previously unknown plugins without a NixOS update or a device build. It accepts zero or one named native service per plugin. It validates the shipped systemd unit and applies a host-owned hardening drop-in. It accepts no installation scripts or TypeScript service directives.

## Host installation

Import `nixosModules.korri-plugin-host` and enable `services.korri.pluginHost.enable` in the base system. This installs the generic CLI, boot recovery, TUN support, the download-only policy, and the upstream OpenSSH privilege-separation account/PAM prerequisites on SSH-disabled hosts. It does not enable SSH, provision account keys, generate recovery host keys, or replace existing recovery SSH/PAM. It preserves the stock NixOS cache and key. No option names a plugin.

The module composes `nix/device-cache/nixos-module.nix`, which sets `max-jobs = 0` and disables remote builders. The importer realizes an exact output with `nix build --no-link --extra-substituters SOURCE OUTPUT`. Despite the command name, it forces `max-jobs = 0`, empty `builders`, `fallback = false`, and `require-sigs = true`: no local or remote builds are allowed. It validates the exact `/nix/store` output before invoking Nix; derivations, flakes and expressions are rejected. Nix can download dependencies from the host's configured trusted caches, including `cache.nixos.org`, rather than requiring the plugin cache to duplicate them. Recursive signature verification consults the same caches, including for already-present paths without locally registered signatures. Verification and store filesystem synchronization still complete before approval.

Core exports `packages.<system>.korri-plugin-host`, including the generic Rust host and publisher. It also exports the optional first-party `korri-plugin-ssh` output; see [`plugins/ssh/README.md`](../../../plugins/ssh/README.md) for TCP 2222 coexistence, root authority, upstream-module extraction, and verification. Its root CLI path does not supply the still-missing local owner enrollment/approval consumer. Production Tailscale packaging and publication belong to [korri-os/plugins](https://github.com/korri-os/plugins). Core retains only a test fixture under `tests/fixtures/tailscale`, copied from that real package and declaration with the same `@korri:tailscale` identity and unchanged nixpkgs binaries. The fixture is not a public package or a distributable plugin. Live publication and the official HTTPS destination still need owner approval. General core binary caching is separate from plugin cache publication.

## Build-side authoring API

`lib.<system>.mkPlugin { publisher; source; plugin; }` is exported by the plugin-host Nix composition. `source` is `plugin.ts`; `plugin` is a `plugin.nix` path or a function receiving `{ pkgs; }`. `publisher.namespace` comes from trusted publication composition, not TypeScript. The lower-level `builder.nix { pkgs; }` API permits a caller-owned package set. Rendering always imports that same `pkgs.path` and passes those `pkgs` to NixOS `eval-config.nix`.

The concrete example is `tests/fixtures/tailscale/plugin.nix`. Its `packages`, named `files`, and standard NixOS `services` produce the generated manifest. `files` are checked with `test -e` during the build. Service values can be standard NixOS service attributes or references to actual native `.service` files (paths, strings, or derivation outputs). Attribute values are rendered by NixOS, not a Korri template. Native file references pass through unchanged. Both reach the same runtime admission and approval boundary. The build context has no global locale, timezone or command-search environment from the CI system. Explicit service `path` is refused; use an immutable `ExecStart` path. Explicit environment directives still render and the host refuses them by name.

The manifest fields are grounded in the approved authoring brief and this builder: `publisher`, `packages`, `files`, `services`, `requires`, and `ports`. `services` maps names to rendered immutable unit paths. `ports` uses NixOS's `allowedUDPPorts` and `allowedTCPPorts` lists. Tailscale requests UDP 41641. Empty contributions need no payload. Each `requires` entry is an exact plugin output in the package's Nix closure. It must also be installed with its own approval before the dependent can be installed. Install dependencies first, then enable them before enabling the dependent. Closure download does not grant installation or activation authority. The host does not recursively approve dependencies. The current one-service limit is retained.

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

Review `warning`, `provenance`, `declaration`, `native_unit`, `ports`, and `unit_configuration` before copying the approval digest. Inspection is required again when the release, archive bytes, package, source or effective execution policy changes. There is no automatic update or blanket approval. Switching a raw-cache installation to a repository also uses `repository switch`; raw `update` cannot change a repository installation's origin.

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

Install leaves the plugin disabled. Approval binds to the exact store path, `plugin.ts` bytes (including any `launch` function), manifest bytes, explicit input provenance, declaration, base policy, complete rendered unit, and any required packages in its closure (reported under `brings`). Installing an approved package commits receipts for its entire closure atomically, and enabling the dependent activates its required dependencies. Repository approval never includes the disposable staging/cache path. Two sources serving identical bytes have different approvals. There is no blanket `--yes` grant.

The device binds each publisher namespace to one full Nix public key and one exact cache URL through `services.korri.pluginHost.publishers`. Each binding has `publicKey` and `cacheUrl`; the module writes `/etc/korri-plugin-host/publishers.json` and adds those keys to Nix's trust list. A non-NixOS administrator supplies that root-owned file and configures the same Nix keys. Missing or malformed configuration fails closed. An empty map permits no external plugin.

The immutable package contains `plugin.ts` and a generated `manifest.json`. Its only identity field is `publisher.namespace`, grounded in the approved authoring brief. The shared builder generates the manifest from trusted publisher composition and native package artifacts; `tests/fixtures/tailscale/package.nix` is the build-side fixture. No manifest key is authoritative. The host verifies the actual selected NAR contents and checks an Ed25519 signature over Nix's fingerprint (path, NAR hash, NAR size, sorted references) with the bound full key. A matching signature label, another globally trusted signer, or content-addressed identity alone is insufficient. Raw-cache inspection also requires the bound cache URL.

When a local store image lacks signatures, the host fetches metadata only from the bound cache, checks it against the local NAR fingerprint, and retains verified signatures in Nix's own metadata. Later offline activation checks that full key again. This can require one metadata request during initial inspection. Removing a binding or replacing its key prevents new starts of old packages, including rollback and boot recovery. It does not revoke the exact installed receipt's authority to stop and clean up. Republish selected builds and cut over bindings operationally; there is no runtime migration.

Namespace verification does not authorize permissions. The administrator still approves the exact package. This is a clean cut: old completion-value plugins and packages without a manifest cannot be installed. Production publication in `korri-os/plugins` must adopt the manifest and named exports separately; this core slice changes its test fixture, not that repository.

## Exact publishing-commit lookup

```sh
sudo korri-plugin install "$CACHE_URL" @korri:tailscale --release "$COMMIT"
```

`COMMIT` must be the full lowercase 40-character publishing commit. `CACHE_URL` must be the publisher-bound, exact `https://github.com/OWNER/REPO/releases/download/CACHE_TAG/` metadata cache URL (the final slash is optional only if the binding omits it). This command resolves and **inspects only**. It prints the exact package, permissions and approval digest, then the existing `install CACHE PACKAGE APPROVAL` command. Review the report before running that command. Installation still leaves the plugin disabled; enable it separately. There is no implicit approval, prompt default, mutable pointer or new catalog.

The lookup is grounded in the approved authoring brief's Versions section and the actual `korri-os/plugins` `PUBLICATION.md` / `nix/github-cache.py` producer (`batch_files`, `build`, `publish`):

| Evidence | Consumer check |
|---|---|
| Batch `build-<first 12 commit characters>` | Derive its location in the same GitHub repository, separate from the metadata cache tag. Never infer trust from that name. |
| Batch `revision.txt` | Fetch at most 41 bytes. Require the exact requested full commit followed by one newline, before requesting paths. A shared 12-character prefix is insufficient. |
| Batch `paths-x86_64-linux.txt` or `paths-aarch64-linux.txt` | Fetch only the running host's architecture, at most 64 KiB. Require nonempty, unique, newline-terminated exact store outputs. Reject derivations, expressions, malformed paths and duplicate lines. |
| Each listed candidate | Use the existing download-only Nix importer, full namespace-bound signature verification, declaration and native artifact validation. Select by the verified declaration ID, never by the store-path name. |

Lookup refuses missing IDs, multiple outputs for the requested ID, and any invalid, unavailable or untrusted candidate, even after finding a match. It re-inspects the selected output to retain its download GC root after scanning other candidates. HTTPS transfers use the existing certificate, redirect, size and timeout policy. Cache and build failures remain errors; lookup never evaluates a flake, instantiates a derivation, compiles, or changes Nix trust.

The cost is downloading and inspecting every listed output, not just the requested plugin. A broken unrelated output blocks that batch. The selected output is inspected again before its report and again during exact-path installation. Custom batch tags and other cache hosts still use raw exact-path inspection; there is no custom-tag lookup option. Evidence is unsigned: the full revision check detects mismatches, but is not cryptographic proof that a package was built from that commit. Nix signatures authenticate package bytes and publisher authority, not the tag or path list. No installed release record is added: the existing receipt and approval keep the exact store path and raw cache provenance, not this lookup commit. Publication immutability and availability remain publisher operations.

Focused local checks (no VM or full plugin check):

```sh
nix develop .#plugin-host --command cargo test --locked --manifest-path services/korrid/plugin-host/Cargo.toml --test release --test repository
KORRI_PUBLISH_NIX="$(command -v nix)" nix develop .#plugin-host --command cargo test --locked --manifest-path services/korrid/plugin-host/Cargo.toml --test repository release_evidence_selects_only_a_unique_bound_signed_package_with_real_nix -- --ignored --exact
```

The second check runs on a build machine. It creates unique test outputs with `nix store add-path`, exports them with real Nix signatures, serves evidence and cache files over local HTTPS, deletes one test output, and downloads its NAR through the real importer. It checks identity, ambiguity, wrong bound keys, malformed declarations, a missing output, an instantiated-but-never-built derivation, and damaged NAR bytes. It does not change host publisher configuration or activate a plugin. Live GitHub lookup, privileged CLI lifecycle, physical ARM installation and cold-store multi-cache closure acceptance are separate gates.

## Lifecycle commands

| Command | Effect |
|---|---|
| `inspect CACHE PACKAGE` | Import and verify the signed closure, then show the bounded declaration and effective policy. Run no payload. |
| `install CACHE ID --release COMMIT` | Resolve the exact publishing batch and show inspection plus the exact-path approval command. Do not install or enable. |
| `install CACHE PACKAGE APPROVAL` | Record an approved package, initially disabled. |
| `update ID CACHE PACKAGE APPROVAL` | Update an existing raw-cache installation of that ID. Preserve enabled state. Restore the prior selection after a failed start when cleanup succeeds. |
| `enable ID` | Enable the approved package and start its daemon, if any. Require enabled exact dependencies. |
| `disable ID` | Stop and clean up the daemon. Retain private data. |
| `remove ID` | Stop, clean up, and release the package and approval. Retain private data. |
| `remove ID --purge` | Also ask systemd to delete the plugin's private state. |
| `status ID` | Show the committed package, provenance, approval, and desired state. This is not a live health report. |
| `unit ID` | Print the managed systemd unit name for status and journal commands. |
| `enabled-packages` | Return approved reports for enabled committed selections under the host lock. Recheck publisher signatures, exact dependencies and active roots; refuse pending transitions. |
| `restore ID` | Swap current and previous exact approvals without importing a package. Preserve current enabled/disabled intent. Refuse a revoked candidate or any broken installed dependency/reference. |
| `restore-all` | Reconcile interrupted operations and restore enabled daemons. Leave matching healthy daemons running. |

The generic boot service runs `restore-all`. Bare `restore` is not an alias; it requires an ID. Every command opens the same exclusive host lock, including source writes. Local `status`, `enable`, `disable`, `remove`, `restore ID`, and `restore-all` use the receipt and immutable store; they do not need a configured or reachable repository. Pending, active and previous package symlinks are Nix GC roots. The host commits a new package or enable selection only after its service operation succeeds. If the device loses power before that commit, boot restores the previous selection only if its publisher is still authorized. Disable and removal record their intent before cleanup, so an error or crash cannot silently re-enable them. They replace a pending enabled operation without first restarting the previous selection. A prior removal keeps its original purge choice.

Deactivation still checks the receipt's exact package, source bytes, manifest, provenance, identity and effective policy against its approval. It skips only current publisher authorization, not approval equality. The existing managed unit may run its already approved `ExecStopPost`, including cleanup from an approved pending update; there is no general cleanup-command bypass. Purging an inactive plugin renders that exact approved unit for systemd's state deletion without starting its daemon. Missing or malformed publisher configuration still prevents opening the host; remove a namespace from the valid binding map to revoke it.

Recovery stops a revoked enabled unit before refusing its restart, even if it was healthy. It retains the receipt and GC roots on refusal. This is not a live revocation watcher: a running daemon is stopped by disable, remove or restore-all, not by the configuration edit alone. If disable or removal reached disk before a crash, recovery only completes cleanup and optional purge; it never starts that daemon.

If cleanup fails, disable, removal and automatic rollback stop. The receipt and package remain available for inspection. Read the unit journal. Do not delete the roots to force success. A failed native cleanup can need separate administrator repair; the CLI does not pretend to reverse arbitrary native effects.

## Installed game admission

Zero-service packages can carry the shipped `systems`, `launchers`, `runtimes`, `discovery`, `sessionControls`, `transports`, `providers`, `android`, and `config` data exports. The installer retains these records without translating them. Record interpretation belongs to the game registry. Inspection never calls `launch`; approval binds its exact source bytes. Reports expose the manifest's existing `packages`, `files`, and `requires` fields.

`Host::enabled_packages()` and the administrator-only `enabled-packages` command expose a consistent admission snapshot. They perform no receipt repair, package realization or callback effects. If Nix lacks a saved signature, verification can fetch signing metadata from the bound cache and retain it, as activation does. They reject changed approvals, revoked signing keys, unfinished selections, inconsistent active roots, and missing or disabled exact dependencies. A cached signature still requires the full bound key. Verification holds the host lock and can be slow for large installed sets; a concurrent install makes the query fail as busy.

Lifecycle changes validate the complete post-operation selection, including cycle rejection. Update or removal cannot replace a build named by any installed dependent, including a disabled one. Disable is refused while an enabled dependent needs that package. Disable dependents first; remove dependents before replacing their exact dependency builds. The cost is explicit dependency-order administration. This checks manifest `requires`, launcher-instance-to-kind references, and runtime-to-launcher references. Native file keys must belong to the declaring package.

Recovery loads the rooted dependency graph and orders it once without recursion. It recovers each required selection before its dependents, regardless of directory enumeration order. A failed dependency recovery stops its enabled dependents and retains their pins. Cycles and their dependents cannot start. Required receipts are read by the immutable package's identity and exact selected path; an unrelated corrupt receipt is reported separately without stopping a healthy dependency chain. This isolation applies to recovery, not the all-or-error registry snapshot or lifecycle selection validation. Graph memory grows with the number of packages and requirement edges; publisher verification still holds the host lock.

Korrid now consumes the read-only projection described under **Installed Linux game registry**. RetroArch and mGBA ship real `plugin.nix` payloads; callbacks execute inside the existing runtime-user game unit. Configuration cascade, typed-setting source checks, route chooser, deployment, and user-data migration remain separate. The existing VM game fixtures still prove admission only; the final integrated game-launch acceptance must use the real payloads.

For the remaining configuration cutover, the task's requirement to preserve grounded legacy schemas takes precedence over the brief's illustrative `extraConfig` spelling. `legacy:product/plugins/retroarch/src/launch-spec.ts` consumes `LaunchOverrides.config` through `renderRetroArchOverrideConfigLines`. That producer uses `overrides.config.prepend` and `.append`, refuses `.replace`, and validates configuration keys and plaintext credential exclusions. `legacy:product/plugins/retroarch/src/policy.ts` contains the nested typed policy. The launch treaty retains that raw configuration shape. It does not introduce a replacement persisted configuration schema.

## Declaration and storage grounding

`src/declaration.rs` consumes named exports `name`, optional `title` and `description`, `services`, and the existing data exports listed above. Service names reference the generated manifest. Missing or repeated names fail. No `daemons` export or compatibility reader remains. The publisher namespace comes from verified packaging, not source. ID segments follow the grammar in `services/korrid/src/plugin.rs`, with a 64-byte bound.

`src/native_unit.rs` parses systemd logical lines, comments, quotes, C escapes and list resets. Unsupported grammar fails closed. The allowlist is `[Unit] Description` and `[Service] Type`, `ExecStart`, `ExecStartPre`, `ExecStopPost`, `User`, `CapabilityBoundingSet`, `DeviceAllow`, and `LoadCredential`. `User` accepts only literal `root` (or an empty reset). Every pre-start command has the same immutable-closure and argument validation as the main command. `Type` is `notify` or `exec`. Capabilities are limited to `CAP_NET_ADMIN` and `CAP_NET_RAW`; the only device request is `/dev/net/tun rw`. Unknown directives are refused by name, not dropped. Execution prefixes, non-store programs, systemd specifiers and arbitrary environment expansion are refused. Native `${STATE_DIRECTORY}` and `${RUNTIME_DIRECTORY}` expansion stays owned by systemd.

Both selected paths and resolved symlink targets must belong to the package's actual Nix closure. The host checks that commands resolve to regular executables. Being present elsewhere in `/nix/store` grants no authority. The unit source is bounded and read as a regular file. Its exact bytes and the effective hardening enter the approval digest with the manifest, which also binds the requested ports.

`src/unit.rs` copies the validated native source unchanged into `/run/systemd/system`. It writes `zzzz-korri-policy.conf` in the unit's host-owned `.service.d` directory. `unit_configuration` shows the source followed by that policy. The drop-in supplies `DynamicUser`, private state, read-only system access, device restrictions, timeouts, cleanup supervision and restart policy. It resets list directives before applying approved capabilities, devices and credentials. Plugins cannot ship host drop-ins. This remains the default for every plugin, including one named SSH. Network administration still grants substantial control of routes and firewall rules; it is not confined to a named interface.

An explicit native `User=root` requests **device-wide root authority**. The report names that authority before approval. This is not a constrained capability profile: the daemon and all its lifecycle commands can change accounts, files, credentials, devices and network policy. The host retains private state/runtime directories, timeouts, restart and control-group stop policy, but does not apply the dynamic-user sandbox, capability drop, read-only system, hidden homes or privilege-escalation restrictions. Administrative OpenSSH needs account switching, writable account homes, PAM and PTYs. The native request, not a plugin ID or publisher, selects this policy. Every other plugin needs its own exact-build approval to request it. The root policy version appears in the effective unit and approval digest. Existing unprivileged policy bytes and approvals are unchanged.

Disable stops the managed service and removes its declared firewall rules. It cannot undo arbitrary root effects or promise to revoke processes that a privileged daemon or PAM moved to other systemd scopes. This is part of approving device-wide authority, not an authorization bypass.

The only accepted credential request is native `LoadCredential=authkey:tailscale-authkey`. Systemd searches its inherited credentials and standard credstore locations. Systemd explicitly treats a missing named credential as non-fatal; inspection reports this. Korri reads no secret, creates no credential and supplies no fallback. Even a delivered credential does not log Tailscale in. The Tailscale fixture requests no credential and starts in `NeedsLogin`. An operator runs `tailscale up` against the reported socket. The VM's separate credential fixture checks both absence and native delivery, without claiming login.

`src/firewall.rs` owns the `korri-plugins` filter chain for IPv4 and IPv6. Each operation uses immutable injected `iptables` and `ip6tables` tools with the xtables lock. Rules carry the existing hashed unit name as their comment. Reconciliation inserts one INPUT jump at position 1 and replaces only that plugin's port rules. Disable, remove, failed enable, rollback and crash recovery remove the candidate rules. Healthy restore reapplies rules without restarting the daemon. Boot recovery runs after `firewall.service`.

The NixOS package uses the observed `nf_tables` iptables backend. A firewall reload preserves this chain. Tailscale can later insert `ts-input` ahead of it when the operator logs in; the Korri jump still precedes `nixos-fw`. Restore puts Korri first again. This is not a watcher or a constraint on an approved `CAP_NET_ADMIN` program. Other distributions must provision matching IPv4/IPv6 helpers for their active firewall backend; automatic Ubuntu backend detection is not implemented or tested.

The host's `StateDirectory` convention grounds `/var/lib/korri-plugin-host`. `src/selection.rs` stores one `selection.json` receipt per hashed ID. Its fields come from the imported package, approval report, and requested lifecycle state. `Disabled`, `Enabled`, and `Removed` are desired states. The latter retains the explicit purge choice during recovery. The native systemd unit name also determines private state and runtime directories. `src/package.rs` is the sole name producer. The report exposes the resulting paths.

The GC roots use Nix's existing `/nix/var/nix/gcroots` convention. `active` pins the committed package. `previous` pins the one retained prior selection. `pending` pins an uncommitted candidate. Runtime units live in systemd's `/run/systemd/system` directory and disappear on reboot.

### Current and previous transaction

The approved brief's gap 7 grounds one `previous` slot in the existing `selection.json`. It is either explicit `null` or the existing `package`, `provenance`, and `approval` fields from the displaced selection. Identity and desired state remain on the current receipt. Prior selections have no desired state or nested history. Missing `previous` fails closed; existing receipts need an explicit one-off operational cutover before deployment. The host performs no migration.

`src/selection.rs` owns this bounded history and its durable roots. A changed update or source switch replaces previous with current. Re-selecting the same package, provenance and approval keeps previous unchanged. Enable and disable also retain previous. `restore ID` swaps the exact package, provenance and approval together, using current desired state. It rechecks both approvals and the candidate's current publisher authority. It never calls the importer or selects another installed dependency version. Signature verification may still consult the bound cache if Nix has lost signing metadata; rollback never downloads package contents.

The host roots pending before service effects. Until success, both committed selections and their approvals remain unchanged. One atomic receipt write commits current and previous together. The host then roots previous before replacing active, and finally removes pending. Recovery uses only that receipt, including after a crash between these steps. Failed starts roll back only after native cleanup succeeds. Failed cleanup retains the receipt and all transaction roots for repair. Removal releases both selections only after cleanup. Successful recovery removes interrupted root temporaries as well.

The retention bound is two committed selections, plus one candidate during an update or failed transition. This is not a promise that Nix retains only two physical builds: exact closures and other installed packages can keep older outputs alive. A third successful update releases the displaced previous root. Restoring a dependency remains refused while any installed dependent requires a different exact build, even if the dependent is disabled. There is no automatic multi-plugin downgrade or arbitrary-version substitution.

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

## Installed Linux game registry

After a lifecycle commit, the host publishes an atomic, root-owned snapshot at
`/run/korri-plugin-host/enabled-packages.json`. It projects the existing report
fields `id`, `package`, `files`, and `requires`; it adds no plugin manifest
fields. The snapshot contains only approved, enabled, committed selections.
The host removes it before any selection mutation or recovery. A failed
transition cannot leave stale launch authority published. Publisher-configuration
changes restart the NixOS restore service before korrid.

`services/korrid/src/plugin_installation.rs` is shared by the host and reader.
Korrid reads a bounded regular file, refuses symlinks and writable parents, and
requires root ownership. The directory is mode 0755 and each published file is
0644. Receipts, approvals, source administration, and the host lock remain
private. There is no privileged reader RPC or WebView access to `korri-plugin`.
The settings backend refuses native install/enable changes from browser callers.

Linux discovery reads these selections, not Android's bundled policy. Several
claims for one system make one catalog candidate; a disagreement about the
system remains explicit. `resolve_linux_route` accepts a full runtime ID and
resolves its launcher instance and kind. Without a choice it accepts only one
compatible runtime. The launcher program comes from its instance's package;
the callback and auxiliary files come from its kind's package. Lifecycle
validation rejects removal or renaming of referenced launcher/kind exports,
even when a plugin omitted the corresponding manifest dependency.

Build-side payload checks (no VM):

```sh
nix build --no-link .#korri-plugin-mgba
KORRI_TEST_GAME_PACKAGE="$(nix build --no-link --print-out-paths .#korri-plugin-mgba)" \
  nix develop .#korrid --command cargo test \
  --manifest-path services/korrid/Cargo.toml --test installed_game_launch \
  built_plugin_nix_payload -- --ignored
```

Install and enable the required RetroArch selection before mGBA. Each build
still needs its own publisher-bound signature and explicit approval. An empty
but initialized host publishes `[]`; a missing snapshot is an authority error,
not a request to use bundled games. `korri-plugin restore-all` rebuilds the view.
The runtime user needs the existing account-directory and ROM access described
in `nix/rg353m/gba-library-access.nix`. This cut does not migrate user files.

The focused package test resolves the real built dependency, discovers a ROM,
checks callback config/argv, and executes that package's `retroarch --version`.
It does not prove a systemd game launch. Final install/enable, runtime-user
sandbox, and gameplay checks belong to the integrated VM/device acceptance.

## Verification

```sh
nix build .#checks.x86_64-linux.korri-runtime-plugin-host
nix develop .#plugin-host --command cargo test --manifest-path services/korrid/plugin-host/Cargo.toml
nix develop .#plugin-host --command cargo clippy --manifest-path services/korrid/plugin-host/Cargo.toml --all-targets -- -D warnings
```

The VM starts without the Tailscale package. A second fixture VM serves signed raw caches, real HTTPS catalogs and complete content-addressed archives produced by the prebuilt publisher. The cold client never publishes or builds. The test disables automatic test-script closure injection, so the client must actually import the package. It proves signature refusal, explicit approval, installation, service readiness, a TUN interface, independent plugin updates, failed-start rollback, power-loss recovery, data retention, purge, cleanup-failure refusal, and an unchanged NixOS generation. No compiler is on the client command path. The extended gates also cover two sources sharing an ID and identical bytes, same-source update, cross-source approval rejection, changed catalogs, failed/interrupted source-switch rollback, source removal with offline lifecycle, and owned stale-staging cleanup. These new VM gates must be run before claiming end-to-end verification.

The current/previous verification record is in `LIFECYCLE-VALIDATION.md`. The current/previous VM gates are added but unrun in this slice. They cover offline swaps with exact source approvals, enabled and disabled intent, exact dependency refusal for enabled and disabled dependents, failed explicit restore, preservation across failed updates and reboot, and prior-root cleanup on removal. Run them only as part of the final integrated gate. Focused receipt/filesystem tests cover the atomic commit and each root repair boundary without a VM.

The revocation VM gates remove a publisher binding and rotate its full key. Both deny enable, permit disable and removal with real Tailscale cleanup, retain data on disable, and purge it on removal. They reject a changed receipt approval, cover pending-operation and persisted-disable recovery boundaries, stop a revoked healthy daemon during restore, refuse revoked restarts, and retain receipts and roots after native cleanup failure.

The split-cache VM gate removes a dependency's NAR and narinfo from the plugin cache. An independently signed, configured upstream supplies it through the real `inspect` and `install` commands. Missing and untrusted upstream cases fail without running an available build canary, even with permissive ambient Nix settings. A later inspection without the dependency's trusted key also fails after the output is already present. A separately seeded unsigned local dependency requires a trusted upstream signature before inspection succeeds. The gate checks unchanged raw-cache provenance, explicit approval, activation and removal. It uses local fixture caches, not a live `cache.nixos.org` connection.

The Nix devshell and package check both supply `KORRI_TEST_CURL`. The real local TLS tests fail if it is absent; none silently return. One explicitly ignored test is a subprocess entrypoint invoked by the ambient-credentials test. Regular package and HTTPS tests remain sandboxed.

`korri-publisher-check [STORE_PACKAGE]` runs the ignored real publisher integration tests on a build machine with a writable Nix store. The optional argument is an already built absolute `/nix/store` package containing the actual `@korri:tailscale` declaration and binaries for the app's system. With no argument, it uses the core test fixture. For example, a consumer can invoke `korri.apps.<system>.korri-publisher-check.program` from its pinned core input with its own package path. The app uses core's immutable host source (including the shared `src/script.rs`), locked Rust/C toolchain and Nix, not the caller's Git root or flake. It copies that source into a temporary writable workspace, sets `CARGO_TARGET_DIR` there, and removes the workspace on exit or handled interruption. Cargo uses `Cargo.lock`; uncached dependencies can require downloads. The cost is a fresh Cargo target and full closure conversion per run. It must never run on a download-only device.

The plugins repository can also import `services/korrid/plugin-host/vm-test.nix` from pinned core. Its existing arguments remain `pkgs`, `hostModule`, `hostPackage`, and `tailscalePackage`. Supply the actual Tailscale package; the update fixture derives from that package's `plugin.ts` and generated manifest. Core's check supplies its test-only package instead. Both run the full cold-host, HTTPS, Headscale, lifecycle and failure tests.

The VM uses real Tailscale 1.90.9 from the pinned nixpkgs. It first reaches `NeedsLogin`, then joins a disposable local Headscale network over TLS. Real IP traffic survives package update and a forced reboot. No owner's credentials or public tailnet are used. The test opts out of host DNS changes with `--accept-dns=false`. It does not prove host DNS integration, physical-device behavior, or remote streaming. The second release changes the plugin package, not Tailscale's upstream binary version.

The crash test exposed imported store data that was not durable when the receipt committed. The importer now synchronizes the store filesystem before approval. This can delay installation while other store writes finish. Another regression covers interruption after writing a unit but before starting it. Restore distinguishes unexecuted cleanup from cleanup that ran and failed.
