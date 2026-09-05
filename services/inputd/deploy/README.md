# Linux input development and deployment

Routine development does not use a NixOS activation. Run `nix run .#korri-dev` to start isolated korrid and inputd processes. Physical input and all actions are disabled by default. `nix run .#korri-dev -- --physical` opts in to the existing validated normalized InputPlumber target, but actions remain disabled.

A normal NixOS consumer imports only `nixosModules.korri-linux-host` and enables `services.korriLinuxHost`. The consumer supplies its runtime identity, network interfaces, and DRM render device. Korri owns InputPlumber, inputd, korrid, Sunshine, headless Sway, service identities, permissions, ordering, validation actions, and the bundle selector. Lower-level modules remain available for Korri development and unusual platform composition. A device configuration does not copy their policy.

```nix
{
  services.korriLinuxHost = {
    enable = true;
    runtimeUser = "korri";
    runtimeUid = 1000;
    runtimeGroup = "korri";
    runtimeGid = 1000;
    firewallInterfaces = [ "tailscale0" ];
    compositor.renderDevice = "/dev/dri/by-path/pci-0000:01:00.0-render";
  };
}
```

The host module installs stable units and initializes `/nix/var/nix/gcroots/korri-bundle/active` once. The active and previous selectors are Nix garbage-collection roots, so both exact bundles remain available for rollback. Each unit uses `korri-bundle-launch` to validate the selected immutable bundle and execute one fixed component without a shell. `nix run .#korri-bundle-select -- switch BUNDLE SYSTEMCTL` changes only that root-owned selector, restarts only InputPlumber, inputd, and korrid, waits for their health, and restores the prior selector if the candidate fails. It does not run `switch-to-configuration`, reload Home Manager, or restart unrelated user services. During first installation, the device gate removes a strictly validated orphan selector only when the current generation does not provide the selector service and no candidate service is active. Routine host generations that already own the selector keep the selected bundle unchanged.

A complete NixOS activation remains necessary only when the stable host layer changes, such as users, groups, udev rules, Polkit policy, kernel settings, or unit structure. Treat that operation as maintenance work that needs explicit approval.

## Full host maintenance gate

`device-check.sh` is the repository-side gate for a reversible NixOS rollout on one explicit device. It does not contain a default host. It does not discover devices.

The gate is read-only unless `--mode` selects a mutation state. Every invocation requires the candidate generation, expected machine ID, expected hostname, and runtime user. Before identity inspection, it requires the exact candidate helper at `CANDIDATE/sw/bin/korri-device-gate` to be root-owned, executable, non-writable store content whose SHA-256 matches the local gate source.

## Safety contract

- Default `inspect` mode only reads state. It captures the result in a mode `0700` temporary directory, prints the sanitized result, and removes the temporary directory on exit.
- Before the first SSH call, every mode requires strict canonical Nix generation path syntax for the candidate. Mutation and reconcile modes also require it for rollback. Remote preflight proves that both exact paths exist and contain `switch-to-configuration`.
- The inputd package installs the byte-identical gate as `bin/korri-device-gate`. The NixOS module places it in the candidate system closure. The root-owned systemd attempt holder receives only the immutable candidate `sw/bin` path so the byte-identical `env bash` helper and bounded holder command resolve without an interactive-shell environment. The rollout never uploads or runs a writable script with `sudo`.
- Every SSH command uses a shell-escaped argv plus a local transport deadline and an appropriate remote command deadline. The default 570-second remote mutation bound covers the measured full agenix/Home Manager NixOS transition plus targeted user-unit reload and service activation; the longer local bound lets the remote timeout release the root lock before cleanup. Tests override these bounds to prove timeout rollback. Candidate, rollback, user, and helper paths never enter unquoted remote shell text.
- A mutation also requires a mode `0700` ledger outside the repository, an explicit runtime user, and a confirmation token bound to the captured machine ID, hostname, and exact candidate generation.
- The script prints the required confirmation token when it is absent. Run the same command again with `--confirm TOKEN` only after checking all three displayed values.
- The script arms rollback in two durable phases before every remote mutation. It first stores a cryptographic nonce in `pending-mutation-starting`, creates the matching remote marker and lease, and only then writes marker-required `pending-mutation`. The remote process enforces the command deadline. A longer local SSH deadline covers the remote deadline and lock wait. Failure, timeout, or interruption runs rollback and records `failed-needs-inspection`.
- A root-owned `/run/lock/korri-device-gate.lock` serializes each remote operation. A root-owned mode `0600` `/var/lib/korri-device-gate/attempt` marker and bounded systemd lease serialize the complete activation, verification, and restore or acceptance window. Both bind the private attempt nonce, exact candidate, and exact candidate helper. Every mutation and automated gate validates them.
- Another invocation cannot replace a live marker. Reconcile removes a matching stale marker only after the lease is inactive and while it holds the root operation lock, so it cannot race a live switch. A marker-required in-progress ledger requires the marker to exist and match its exact nonce and candidate. A `*-starting` ledger permits no marker, but still requires its stored nonce and the state-specific rollback or candidate checks before it can return to its resume state. If a starting state has a marker, reconcile applies the same exact-match and active-lease rules.
- Every automated gate and acceptance-fingerprint read after activation has both the remote wall-clock deadline and the longer local SSH deadline while rollback remains armed.
- The gate rejects another mutation from `failed-needs-inspection`. `reconcile` is read-only. It requires the rollback generation and every sanitized baseline predicate to match before it restores the prior ledger state.
- The script never retries a switch, profile change, reboot, or destructive command blindly.
- Every user-scope systemd operation resolves the explicit runtime user's UID, then uses `sudo -n -u USER` with `XDG_RUNTIME_DIR=/run/user/UID` and `DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/UID/bus`. It never infers a user from `SUDO_UID`. Manager or transport errors fail the gate instead of becoming inactive or disabled state. The gate captures `korrid.service`, `sunshine.service`, and `x11-headless.service` separately. It preserves each old user-unit file and restores each active/enabled pair exactly.
- Before each candidate or rollback mutation, the gate queries the exact local korrid control socket when it exists and lists live `korri-game-*.service` units. `running`, `stopping`, any unknown local status, or any activating/active/reloading/deactivating Korri game unit blocks mutation. If the socket is absent or unreachable, cleanup proceeds only when there is no live game unit and the existing `/var/lib/korrid/host-session` state is absent or an empty, non-symlink mode `0700` directory. Any persisted or ambiguous private state fails closed. The gate never stops a game to make rollout proceed.
- Candidate activation first removes a strictly validated orphan bundle selector when the rollback generation does not own the selector service. It refuses symbolic roots, unexpected entries, mutable targets, wrong ownership or modes, and any active candidate service. Candidate rollback applies the same cleanup after the old generation returns.
- Candidate activation stops all three old user units and proves that they are inactive. It then tightens the Sunshine state modes. The gate switches to the candidate and reloads the runtime user's unit definitions without restarting the user manager. It disables the retained user-unit files and proves that they stay inactive. It then starts `korri-compositor.service`, `korrid.service`, and `sunshine.service`. The gate verifies their declared process credentials and rejects stale user-manager groups.
- Inputd disables zbus composite-property caching. It uses only explicit `Properties.Get` calls permitted by the narrow DBus policy and never requires broad `Properties.GetAll` authority.
- Raw-controller rollback predicates use the USB serial, udev path, and stable physical interface path. They do not use volatile kernel `inputN` names. Reconnecting the same controller to the same port is stable, but another controller or port changes the predicate.
- `switch-to-configuration` exit status `4` is accepted only when the requested generation became current and every unhealthy service observed after activation was already unhealthy before activation. Each side uses a bounded 60-second observation that includes failed state, `activating/auto-restart`, and restart-counter growth, so a restart loop cannot hide in an active interval. Any new unhealthy unit, another exit status, or a generation mismatch fails the gate and leaves rollback armed. This permits an explicitly accepted pre-existing unrelated failure without hiding a new Korri or host regression.
- Rollback stops the three candidate system services and restores the exact prior generation. It reloads the rollback user-unit definitions without restarting the user manager. It then restores all six old user-unit state predicates. Enablement restoration is independent. Active rollback units start in this order: X11 headless, Sunshine, then korrid. A failed active-game check leaves the generation and game untouched. Manual inspection is then necessary.
- The script does not run `reboot`. The operator performs each reboot as a separate HITL action, then runs the matching read-only reboot-verification state.

The cost of this design is more operator steps. It also needs a consuming NixOS configuration to build both generation paths before rollout. The gate cannot make an unsupported controller profile valid.

## Private baseline ledger

The baseline contains no configuration file contents and no journal payloads. It records:

- the machine ID and hostname;
- current and default NixOS generation links;
- InputPlumber and inputd summaries;
- system summaries for korrid, Sunshine, X11 headless, and the Korri compositor;
- user summaries for the retained rollback korrid, Sunshine, and X11 headless units;
- sanitized physical-controller candidate identities, temporary-artifact status, and catalog health;
- exact active and enabled state for each old user korrid, Sunshine, and X11 headless unit;
- exact active and enabled state for each same-named system unit;
- normalized and raw topology digests;
- permissions, ACL, and runtime-user readability as one digest;
- moved-source and temporary-artifact topology as one digest;
- InputPlumber active and enabled state;
- Sunshine pairing-state file presence plus sanitized directory/file modes;
- one combined digest for the protected Sunshine configuration tree;
- the exact running `sunshine-korri` store executable and approved patch-set digest.

The observed Sunshine baseline grounds the presence check at the runtime user's `.config/sunshine` tree. The gate resolves the runtime UID and home. It requires the tree, `sunshine.conf`, and `sunshine_state.json` to stay below the real home `.config` directory. It rejects links, special nodes, wrong owners, group/other write access, empty required files, and path escapes. The separate candidate presence check requires the top directory and state file to be private after the bounded mode adjustment. Before candidate Sunshine starts, the gate changes only the top directory and state-file modes to `0700` and `0600`; every rollback restores the exact captured baseline modes before the old user service restarts. A combined SHA-256 digest covers sorted relative names, object metadata, and regular-file content hashes. The gate records only that combined digest. It never prints a file name, file content, per-file hash, certificate, PIN, pairing record, application definition, or private setting. Exact baseline comparison proves that rollback did not lose or replace the private tree.

While the candidate is active, the gate resolves `/proc/$MainPID/exe`, requires it to equal the one immutable `/nix/store/.../bin/sunshine` executable declared by `sunshine.service`, and rejects any package path that is not `sunshine-korri`. It validates the mode-`0444`, root-owned installed provenance file, package identity, executable record, complete eleven-patch record set, and 64-character patch-set digest. Stock Sunshine, a replaced running executable, incomplete provenance, or a mutable provenance file fails before HITL.

The gate never records game paths, catalog titles, game content, private configuration contents, credentials, or environment values.

Delete the private ledger after the final evidence has been transferred to the work ledger in sanitized form. The private ledger is operational evidence, not a repository artifact.

## Inputs

Obtain the expected identity from a separate trusted device inventory. Do not copy it from a failed gate run.

Build the candidate through the consuming NixOS configuration. Import `nixosModules.korri-input` and configure the module through `services.korriLinuxInput`. The flake output name stays `nixosModules.korri-input`; the option namespace does not reuse the legacy `services.korri.input` tree. Enable `services.korriLinuxInput.provider` and `services.korriLinuxInput.inputd` independently for the device capabilities in the candidate.

Keep the active pre-rollout system closure as the rollback generation. Both paths must contain `bin/switch-to-configuration` and remain available until all reboot gates pass.

Mutation modes require the first eight arguments below. `candidate-test`, `persistent-switch`, `candidate-reboot-verify`, and reconcile after candidate verification also require the final two controller arguments:

```text
--host HOST
--expected-machine-id ID
--expected-hostname NAME
--candidate /nix/store/CANDIDATE
--rollback-generation /nix/store/ROLLBACK
--runtime-user USER
--ledger PRIVATE_DIRECTORY
--confirm TOKEN
--expected-controller-id BUS:VENDOR:PRODUCT:VERSION
--production-profile korri-60-xbox_one_gamepad.yaml
```

No path above is a project schema. `CANDIDATE`, `ROLLBACK`, `USER`, and the consumer configuration remain device-owned inputs. The repository modules do not define Zao's consumer path or identity values.

## Runtime identity cutovers

The gate requires one concrete runtime user to own both the rollback and candidate user services. Do not use it to rename an account, change its UID or GID, or move its home in the same activation. The gate would inspect or restore the wrong user manager after either generation changed the identity.

The RG353M therefore keeps its deployed `gameplay:games` identity, UID/GID `1001`, and `/home/gameplay` while the public module contract changes to `runtime*`. Its separate cutover to `korri:korri`, UID/GID `1000`, and `/home/korri` needs a device-backed operation with these gates:

1. Read the real passwd and group records. Find every file owned by UID or GID `1001`. Confirm the Sunshine tree and its modes from the device itself.
2. Preserve the complete `/home/gameplay/.config/sunshine` tree. Prepare the new home with numeric ownership `1000:1000` without removing or changing the rollback copy.
3. Prove a rollout path that addresses the rollback identity and candidate identity separately. The current gate does not do this. Stop if this proof is absent.
4. Activate and verify the candidate. On failure, restore the old generation, account identity, ownership, home, and user-unit state before retry.
5. Remove the old account and home only after the candidate survives reboot verification and the private Sunshine-state digest is unchanged.

The device was not reachable when this contract rename was prepared. The account record, external ownership, and migration commands remain unverified until a direct read-only inspection is possible.

## State sequence

Use one private ledger for the complete sequence. The gate rejects out-of-order states.

| State | Mode | Device mutation | Required result |
|---|---|---:|---|
| Baseline | `inspect` | No | Identity and sanitized device posture captured. |
| Failed mutation | `reconcile` | No | A fresh inspection proves the rollback generation and all sanitized predicates exactly match the baseline before retry. |
| Temporary candidate | `candidate-test` | Temporary | Candidate activates with NixOS `test`, automated and HITL gates pass, then rollback restores the prior generation and all three old user units. |
| Automatic rollback | `inject-health-failure` | Temporary | The provider stops once, inputd publishes `Recovering` or `Missing`, and the prior generation restores automatically. |
| Explicit rollback | `rollback` | Persistent rollback | Candidate activates temporarily, then the system profile and runtime restore to the rollback generation. |
| Rebooted rollback | `rollback-reboot-verify` | No | Boot ID changed, rollback generation booted, and automated regression gates pass. |
| Persistent candidate | `persistent-switch` | Persistent | Candidate becomes the system profile, all three old user units stay disabled but retained, the runtime user manager has permitted credentials, and automated gates pass. |
| Rebooted candidate | `candidate-reboot-verify` | No | Boot ID changed, candidate generation booted, and automated plus HITL gates pass. |

`rollback-reboot-verify` and `candidate-reboot-verify` do not reboot the machine. Reboot only after the preceding mode exits successfully.

Candidate, persistent, and candidate-reboot modes have no default controller. The operator copies one sanitized identity from `inspect`, checks it against the physical controller, and passes its exact lowercase `BUS:VENDOR:PRODUCT:VERSION`. Pre-activation validates exactly one live non-virtual node with that identity, matching device major/minor, and `ID_INPUT_JOYSTICK=1`; it does not require the rollback provider to expose the candidate profile. While the candidate is active, its InputPlumber composite must list that exact `/dev/input/eventN` in `SourceDevicePaths`, and its `ProfilePath` must be the production package's `korri-60-xbox_one_gamepad.yaml`. Candidate-test records this proof. Persistent switch requires that prior evidence and rechecks the live physical identity before activation; the acceptance fingerprint rechecks full profile selection after activation.

The Korri host keeps source devnodes in `/dev/input` so InputPlumber 0.75.2 receives upstream add and remove events. InputPlumber removes `uaccess` and sets source mode `000`. The gate proves that the runtime user cannot open the raw node. The selected composite persists its one normalized target across source reconnects. Inputd stops only empty non-authoritative composites, which removes stale secondary targets without replacing the primary target. The rollback ACL digest ignores a redundant POSIX ACL mask when no named user or group ACL entry exists. It still records the mode, owner, group, runtime-user access, named entries, and effective permissions.

A synthetic controller can be used only during separate temporary preflight work. Use the exact name `Korri U7 Synthetic Controller` and a private path matching `korri-u7-device-gate.*` only for that bounded preflight. Remove both before invoking a mutation mode. The gate refuses either artifact before mutation. Synthetic, virtual, stale, unsupported, generic-joystick, and identity-mismatched evidence never satisfies persistence or reboot acceptance.

## HITL stages

The candidate remains active while a human verifies behavior that repository code cannot infer from static service state. `candidate-test`, `persistent-switch`, and candidate reboot verification prompt for all seven stage tokens. Each `/dev/tty` read and the complete HITL stage have bounded timeouts. A timeout exits through armed cleanup and rollback. Before activation, the gate generates a cryptographic per-attempt nonce and durably stores it in a marker-not-yet-required starting state. After the root-owned remote marker and lease start successfully, the gate durably transitions to the marker-required in-progress state. Candidate reboot verification never writes an in-progress state without that nonce. Each displayed token binds the machine ID, hostname, exact candidate, nonce, boot ID, ledger state, and gate. The ledger records every consumed gate and rejects reuse. Production and tests use the same controlling-terminal path. No environment variable or command argument can supply or bypass HITL tokens.

Immediately before acceptance, the gate re-reads the exact InputPlumber 0.75.2 `xb360` identity and capability fingerprint and the exact running `sunshine-korri` executable and patch-set provenance. When the immutable Sunshine argv selects `encoder=nvenc`, it also requires strict encoder mode and proves that the latest connected H.264 stream created `h264_nvenc` without creating `h264_vaapi` after that session began. It requires the virtual sysfs provenance, empty `phys` and `uniq`, exact keys and absolute axes, force-feedback support, joystick class, InputPlumber executable/version, event node, sysfs path, inode, and matching sysfs/device major-minor to remain unchanged. Persistent and reboot acceptance also re-read the CLI-bound physical identity and live production-profile selection.

Do not enter a token until the named stage passes.

The validation `workspace-next` action runs immutable `swaymsg` argv without a shell. It focuses and restores fullscreen on the active game workspace. Inputd keeps its normal action limits and cgroup isolation.

| HITL stage | Operator action and required observation |
|---|---|
| `normalized-gameplay` | Start a fresh game process. Confirm exactly one Xbox 360 target works and no raw physical controller opens under the runtime identity. |
| `health-recovery-ambiguity` | Disconnect and reconnect the source. Confirm `Missing`/`Recovering`, then `Ready`. Introduce one bounded second normalized test target. Confirm `Ambiguous`, no action, held-state clearing, recovery, and full cleanup. |
| `dbus-spoof-and-exclusive-grab` | Use a bounded exclusive evdev grab. Confirm the raw observer blocks while the authenticated DBus shortcut still fires. Emit the same signal from an unrelated system-bus client and confirm inputd rejects it. |
| `exact-stop-and-races` | Record one active launch ID. Complete the exact destructive hold and confirm only that launch cgroup stops. Repeat replacement, child-exit/restart, and provider/korrid restart races. Confirm stale holds never stop a replacement and korrid restart does not kill the active launch. |
| `direct-action-isolation` | Keep exactly one game active. Trigger the configured action and confirm the focus or fullscreen change. Inspect the action child and confirm its identity, cgroup, environment, descriptors, capabilities, limits, and cleanup. The gate proves that the game can use Xwayland but cannot see native Wayland, Sway IPC, or the compositor process. |
| `sunshine-video-controller-recovery` | Confirm video and existing pairing presence. For explicit NVENC, select H.264 and confirm the active session creates `h264_nvenc` without VAAPI fallback. Test local and remote controller input through disconnect/reconnect and one system Sunshine restart. Apply live bitrate and FPS changes, restore both launch values, downshift resolution, restore launch resolution, and verify disabled or unsupported requests fail without reconnecting or losing video. Confirm one normalized target, the same attested `sunshine-korri` executable after restart, and no virtual-device feedback loop after repeated reconnects. |
| `catalog-and-session` | Confirm the local catalog is healthy without recording titles or paths. Start and stop one session and confirm session recovery remains healthy. |

The automated portion requires active InputPlumber, inputd, korrid, the Korri compositor, and Sunshine. It requires inputd status `Ready`. It verifies the compositor output, stable Wayland alias, Xwayland `:0`, and the NVIDIA render device. It verifies the Sunshine Wayland environment. Sunshine uses a private PID namespace and cannot reach Sway IPC directly or through procfs. The gate also checks private state, pairing, package provenance, normalized input, DBus, cgroup delegation, and catalog health.

For korrid, the compositor, Sunshine, and inputd, the gate requires an explicit unit user and a Nix-store unit file. It requires one live main PID with matching UID and GID values. The gate checks process groups for these services, the runtime user manager, and each live game unit. It rejects `input` and `uinput` access everywhere. Only inputd can use `korri-control`. Only Sunshine can use `korri-sunshine-uinput`. Required `video` and `render` groups remain allowed. Each game unit hides the compositor control directory and runtime-user directory. It receives a read-only bind for Xwayland socket `X0` only. The gate rejects all other visible X11 sockets. Each game unit also uses a private PID namespace, so procfs cannot reveal Sway's root directory.

The raw-readable scan skips only the event node whose complete normalized fingerprint still equals the verified fingerprint. A physical joystick with the same `Microsoft X-Box 360 pad` name remains raw and causes failure when the runtime user can read it. The two inputd delegation properties use the values returned by `systemctl show`; `Delegate=pids` is not a valid substitute for the runtime boolean property.

## Failure handling

If a mutating state fails, allow the cleanup trap to run. Do not rerun the failed mutation. Run `reconcile` with the same explicit target, generation, runtime-user, and ledger arguments. It compares current and default generation links, all three old user-unit state pairs, and all four system-unit state pairs. It also compares topology, input access, temporary artifacts, InputPlumber, pairing, private state, and catalog health with the private baseline. The rebooted rollback uses the same complete comparison. Reconcile does not mutate the device.

SIGKILL can bypass local cleanup and leave `pending-mutation`, `rollback-reboot-verifying`, `candidate-reboot-verifying`, or their `*-starting` states. Marker-required states require the exact nonce/candidate marker. Starting states can reconcile without a marker; an existing marker must match exactly. Any active matching lease blocks reconcile. Pending mutation requires the rollback generation and complete baseline. Rollback reboot verification also reruns rollback gates under a fresh bounded lease. Candidate reboot verification requires the candidate generation, exact controller/profile, complete automated gates, and an unchanged acceptance fingerprint under a fresh bounded lease. Only successful checks restore the ledger's explicit resume state. A failed check records `failed-needs-inspection`.

A gate, HITL token, SSH, or fingerprint failure during `candidate-reboot-verifying` records `failed-needs-inspection` with `resume_state=candidate-await-reboot`. Reconcile requires the candidate generation plus the same exact physical identity and production profile. It then restores `candidate-await-reboot`, so a transient failure can retry verification without repeating the persistent switch.

If cleanup reports that rollback failed, stop. Use the exact rollback generation printed in the private ledger. Do not guess another generation and do not remove any old user unit.

A missing rollback closure, host identity mismatch, missing exact physical-controller proof for persistent states, or any `Korri U7 Synthetic Controller` / `korri-u7-device-gate.*` residue is a hard stop.

## Repository verification

Run without SSH or device mutation:

```sh
./services/inputd/deploy/test-device-bitmap.sh
./services/inputd/deploy/test-device-check.sh
shellcheck services/inputd/deploy/device-check.sh services/inputd/deploy/test-device-check.sh services/inputd/deploy/test-device-bitmap.sh
```

The shell test replaces SSH with a modeled command endpoint and exercises the production process-group policy directly. It covers all 64 active/enabled combinations for the three old user units, dependency-ordered active restoration, independent enablement restoration, error-aware user-manager queries, explicit runtime-user manager addressing from root and a different SSH user, process primary- and supplementary-group rejection for services, games, and the runtime user manager, no-follow pairing-state proof, stale user-manager credentials, active-game refusal through exact status and live units, candidate system-service failure, complete replacement shutdown and old-unit restoration, pairing presence, two-phase SIGKILL windows, systemd delegation semantics, physical identity and live production-profile selection, topology count, normalized InputPlumber provenance/capabilities, ACL exposure, baseline drift, target replacement, remote timeout, and interruption. Successful HITL tests run the production `/dev/tty` prompt path through a pseudo-terminal. The tests also prove malicious generation paths stop before SSH, remote argv stays intact, rollback is armed before mutation, a remote mutation that survives transport finishes before lock-serialized rollback, candidate reboot token/SSH failures resume through reconcile, accepted persistent state resumes without repeating mutation, and the nonce is absent from terminal output.
