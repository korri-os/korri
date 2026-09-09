# Runtime installation of Linux plugins

Status: the original native lifecycle is implemented and locally verified. The evidence below records that slice, not a live publication result. HTTPS repository publication preparation now follows [PUBLICATION.md](../../services/korrid/plugin-host/PUBLICATION.md). No live destination is configured.

## Approved requirements

A device with generic host support must install a plugin that its system generation never named. Install, enable, update, disable, and remove require neither native compilation on the device nor a NixOS update. Plugins are independently redistributable. Build machines produce native closures; devices download prebuilt content. Prebuilt native payloads are permitted. TypeScript/JavaScript declarations remain interpreted and effect-free.

The first real payload is Tailscale. The owner approved an administrator CLI for the first permission flow. Portal controls and Android integration are out of scope. There are no compatibility requirements for bundle paths or commands. Do not remove unrelated core deployment code merely because compatibility is unnecessary.

## Source grounding

- `services/korrid/src/script.rs` and `services/korrid/SCRIPTING.md` own the existing declaration evaluator. External source requires bounded execution, source size, and memory.
- `services/korrid/src/plugin.rs` and `plugins/retroarch/plugin.ts` establish `namespace`, `name`, `title`, and `contributes`.
- `legacy:product/platform/plugin/index.ts` establishes `contributes.daemons`, but its `PluginDaemonFactory.create` executes callbacks. This case requires data instead of executable callbacks. Preserve established plugin identity. Introduce only daemon data required by the actual systemd consumer.
- Tailscale's upstream `cmd/tailscaled/tailscaled.service` supplies the real command, notification readiness, stop cleanup, runtime directory, and private state directory. The pinned nixpkgs package supplies binaries and their tool dependencies. Do not copy its unrestricted unit directly into the system manager.
- `services/korrid/src/host/systemd_unit.rs` already uses runtime systemd units. Its game-launch authorization is not permission to install privileged plugins.

Persisted receipt structure and paths must come from implemented producers/consumers, with rationale documented here. Do not create a capability catalog, registry service, compatibility layer, or speculative package format.

## Host boundary

One generic host installation provides the CLI, boot recovery, immutable Nix import, and a narrow privileged systemd adapter. No configuration names individual plugins. Policy is an allowlisted subset of the actual Linux service contract, not permission for a plugin to supply arbitrary unit directives or installation scripts.

Permission approval is separate from Nix cache signature verification. Approval binds to the exact immutable package and complete requested execution policy. The first implementation requests approval for every new package, including updates. This costs another confirmation but avoids inventing publisher-level trust or automatic grant inheritance.

The CLI presents the policy before acceptance. An unapproved package cannot start. The owner must authorize requested host-network administration explicitly. This permission can change host routes and firewall rules; it is not confined to one named interface. It must not be described as ordinary network access.

The host stages imports without running payload code. Missing prebuilt outputs fail closed. Installation does not imply enablement. Enabled plugins resume after reboot. Disabled plugins remain off during updates. Tailscale authentication state survives disable and package update. Removal must state separately whether it retains or deletes private state; default to retaining data rather than silently deleting credentials.

Keep declaration evaluation bounded and separate from privileged effects. Pin approved store closures against garbage collection. Serialize mutations. Persist the intended state before starting effects, and define recovery after each interruption. A failed update must not report success or silently forget failed cleanup. Never restart unrelated services.

## Execution and verification

1. Write public CLI/VM acceptance for an empty generic host. Prove missing functionality before implementation.
2. Implement bounded declaration loading, policy validation, immutable package identity, and approval. Test malformed source, path traversal, unsupported permissions, and exact-package approval.
3. Implement lifecycle and boot recovery against real files and real systemd units. Test failed updates, disabled updates, interrupted work, and removal.
4. Package the host and Tailscale outside the device. Keep build-side delivery separate from the runtime lifecycle.
5. Boot a VM with no Tailscale package or named configuration. Import prebuilt closures, exercise the complete CLI lifecycle, and assert the system generation does not change. Exercise a second plugin identity and prove isolation.
6. Run static checks, Rust tests, package checks, VM tests, and security review before landing.

The VM must prove TUN creation, daemon readiness, and cleanup. A private tailnet login needs an owner-controlled credential and separate operational approval. Do not claim remote streaming or authenticated tailnet connectivity from an offline daemon test.

## Implementation choice and costs

The [parallel installation brief](../briefs/2026-09-08-linux-plugin-installation-brief.md) proved standard portable services with a trusted profile. That mechanism remains valid. This implementation instead uses a bounded systemd adapter. It accepts only the daemon fields needed by Tailscale and runs the payload as a dynamic unprivileged user. The administrator grants the exact package's network capabilities, not arbitrary unit directives.

The new evidence is the maintained cold-host VM: this restricted service carries authenticated IP traffic, survives update and reboot, and removes its network rules. Native dependencies remain ordinary Nix store objects shared between plugin packages, rather than copies inside separate portable images.

The cost is maintaining the unit adapter and its interruption tests. Its permission set is intentionally limited to the actual case. Host DNS administration is not implemented; login uses `--accept-dns=false`. This does not promise browser-style isolation. Host-network administration can still redirect traffic or interrupt access to the device.

The existing `korri-tailscale` native export is now a small declaration package linked to the same unchanged nixpkgs binaries. Its version check still exercises both actual executables. The host reuses the separately landed download-only device policy. The current owner-triggered workflow explicitly checks both Linux architectures and the host's Rust tests, with x86_64 VM acceptance as a separate gate. It replaces the former Garnix operational wiring; its presence is not evidence of a live CI run.

## Verified evidence

- The plugin-host Rust suite passes 17 tests, including the shared interpreter tests. `nix run .#korri-plugin-check` passes formatting, Clippy, package tests, and the authenticated VM lifecycle.
- `nix run .#korrid-test -- --lib` passes 492 tests with 2 ignored. The existing dependency warning in proseql is unchanged.
- The x86_64 package, Tailscale executable, and device-cache checks pass.
- The VM imports from a signed cache into a device with no Tailscale package or named configuration. It rejects an untrusted signature and an incorrect approval.
- The real daemon reaches `NeedsLogin`, then joins a disposable Headscale network over TLS. Both Tailscale protocol ping and normal IP ping succeed before and after a forced reboot.
- Updating Tailscale leaves the second plugin's PID unchanged. A failed candidate restores the previous selection. Disabled updates remain disabled.
- Forced power loss and interruption before the first service start recover correctly. Failed native cleanup retains the receipt and package roots instead of reporting successful removal.
- Disable restores the original IPv4 and IPv6 policy rules. Ordinary removal retains private data; explicit purge removes it. The system generation remains unchanged throughout.

The aarch64 host and Tailscale package expressions evaluate successfully. Their ARM binaries have not been executed in this session. The diff received a direct security and correctness review; an independent subagent review was unavailable after provider errors.

No physical device, owner credential, or public tailnet changed. Local verification does not prove live publication. The current workflow only prepares artifacts or an explicitly approved draft; immutable-release settings and separate HTTPS catalog deployment remain operator prerequisites. Application-data rollback across incompatible upstream versions remains outside package-selection rollback.
