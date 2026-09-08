# Administrator-approved Linux plugins

The host installs previously unknown plugins without a NixOS update or a device build. It accepts one declared daemon per plugin. It does not accept raw unit files, installation scripts, or new Linux permission kinds.

## Host installation

Import `nixosModules.korri-plugin-host` and enable `services.korri.pluginHost.enable` in the base system. This installs the generic CLI, boot recovery, TUN support, and the Garnix cache key. No option names a plugin.

The module composes `nix/device-cache/nixos-module.nix`, which sets `max-jobs = 0` and disables remote builders. The importer also forbids fallback builds. It accepts only exact store output paths and uses `nix copy`, never a flake installable or `nix build`.

Build outputs are `packages.<system>.korri-plugin-host` and `packages.<system>.korri-tailscale`. Include both Linux architectures in Garnix's explicit build list. Garnix's GitHub app and cache publication are separate operational setup. They were not configured by this slice.

## Install and approve

Use the exact package output published by the build for this device architecture. The cache must contain the entire signed closure.

```sh
sudo korri-plugin inspect https://cache.garnix.io "$PACKAGE"
```

Read `warning`, `declaration`, and `unit_configuration` in the report. Copy its `approval` value only after reviewing that access.

```sh
sudo korri-plugin install https://cache.garnix.io "$PACKAGE" "$APPROVAL"
sudo korri-plugin enable @korri:tailscale
```

Install leaves the plugin disabled. The approval binds to the exact store path, declaration, and complete rendered unit. A different package or host execution policy needs another approval. There is no blanket `--yes` grant.

A Nix signature proves cache integrity. It does not prove the publisher's identity or authorize permissions. Plugin IDs are self-declared. The administrator approves the exact package, not an automatically trusted publisher.

## Lifecycle commands

| Command | Effect |
|---|---|
| `inspect CACHE PACKAGE` | Import and verify the signed closure, then show the bounded declaration and effective policy. Run no payload. |
| `install CACHE PACKAGE APPROVAL` | Record an approved package, initially disabled. |
| `update ID CACHE PACKAGE APPROVAL` | Update only that ID. Preserve enabled state. Restore the prior selection after a failed start when cleanup succeeds. |
| `enable ID` | Start the approved daemon. |
| `disable ID` | Stop and clean up the daemon. Retain private data. |
| `remove ID` | Stop, clean up, and release the package and approval. Retain private data. |
| `remove ID --purge` | Also ask systemd to delete the plugin's private state. |
| `status ID` | Show the committed package, approval, and desired state. This is not a live health report. |
| `unit ID` | Print the managed systemd unit name for status and journal commands. |
| `restore` | Reconcile interrupted operations and restore enabled daemons. Leave matching healthy daemons running. |

The generic boot service runs `restore`. Each mutation holds one exclusive host lock. Pending and active package symlinks are Nix GC roots. The host commits a selection only after its service operation succeeds. If the device loses power before that commit, boot restores the previous selection. Removal records its intent before cleanup, so recovery cannot silently re-enable it.

If cleanup fails, removal and automatic rollback stop. The receipt and package remain available for inspection. Read the unit journal. Do not delete the roots to force success. A failed native cleanup can need separate administrator repair; the CLI does not pretend to reverse arbitrary native effects.

## Declaration and storage grounding

`src/declaration.rs` preserves Korri's `namespace`, `name`, optional `title` and `description`, and `contributes`. ID segments follow the grammar in `services/korrid/src/plugin.rs`, with a 64-byte bound.

Legacy's `contributes.daemons` contains executable factories. This host replaces factories with the systemd fields exercised by Tailscale: `Type`, `ExecStart`, optional `ExecStopPost`, and `CapabilityBoundingSet`. Only `notify` and `exec` service types are supported. The only nonempty capabilities are `CAP_NET_ADMIN` and `CAP_NET_RAW`. Unknown fields, explicit nulls, duplicate capabilities, and executable path traversal fail.

Commands name `bin/<program>` inside the immutable package. Their arguments can use systemd's `STATE_DIRECTORY` and `RUNTIME_DIRECTORY` names. The host substitutes its own directories. It never passes untrusted unit directives, environment variables, specifiers, or a shell command string to systemd.

`src/unit.rs` supplies `DynamicUser`, private state, read-only system access, device restrictions, and restart supervision. The payload never runs as root. Host-network administration still grants substantial control of routes and firewall rules. It is not confined to a named interface.

The host's `StateDirectory` convention grounds `/var/lib/korri-plugin-host`. `src/host.rs` writes one `selection.json` receipt per hashed ID. Its fields come from the imported package, approval report, and requested lifecycle state. `Disabled`, `Enabled`, and `Removed` are desired states. The latter retains the explicit purge choice during recovery. The native systemd unit name also determines private state and runtime directories. `src/package.rs` is the sole name producer. The report exposes the resulting paths.

The GC roots use Nix's existing `/nix/var/nix/gcroots` convention. `active` pins the committed package. `pending` pins an uncommitted candidate. Runtime units live in systemd's `/run/systemd/system` directory and disappear on reboot.

The CLI uses korrid's actual evaluator from `services/korrid/src/script.rs`. That interpreter now limits source size, execution time, memory, and stack. Declarations must be deterministic. A changed evaluation cannot match a stored approval.

## Verification

```sh
nix build .#checks.x86_64-linux.korri-runtime-plugin-host
nix develop .#plugin-host --command cargo test --manifest-path services/korrid/plugin-host/Cargo.toml
nix develop .#plugin-host --command cargo clippy --manifest-path services/korrid/plugin-host/Cargo.toml --all-targets -- -D warnings
```

The VM starts without the Tailscale package. Its signed cache is a second VM. The test disables automatic test-script closure injection, so the client must actually import the package. It proves signature refusal, explicit approval, installation, service readiness, a TUN interface, independent plugin updates, failed-start rollback, power-loss recovery, data retention, purge, cleanup-failure refusal, and an unchanged NixOS generation. No compiler is on the client command path.

The VM uses real Tailscale 1.90.9 from the pinned nixpkgs. It first reaches `NeedsLogin`, then joins a disposable local Headscale network over TLS. Real IP traffic survives package update and a forced reboot. No owner's credentials or public tailnet are used. The test opts out of host DNS changes with `--accept-dns=false`. It does not prove host DNS integration, physical-device behavior, or remote streaming. The second release changes the plugin package, not Tailscale's upstream binary version.

The crash test exposed imported store data that was not durable when the receipt committed. The importer now synchronizes the store filesystem before approval. This can delay installation while other store writes finish. Another regression covers interruption after writing a unit but before starting it. Restore distinguishes unexecuted cleanup from cleanup that ran and failed.
