# Mini V2 native-input RAM trial

This is an operator tool, not a deployment task. It replaces no signed plugin.
A physical-native pass is **not** remote Sunshine acceptance. The old signed
Sunshine and its receiver stay stopped and runtime-masked during capture.

No device command, build, test or commit ran during this implementation. Parent
owns all staging, builds, execution, review and hardware acceptance. Do not use
`iteration/run.py`: that tool accepts the retired browser Gamepad path.

## Grounding

- Authority, exact spare SD/system/boot: `docs/briefs/2026-09-28-native-input-component-trial.md`.
- Receiver unit/hardening/IDs: `services/inputd/nix/korri-linux-host-core.nix`.
- Seat rules: `services/inputd/nix/input-seat-rules.nix`.
- Polkit grants/denial and profile environment: `services/inputd/nix/korri-input.nix`.
- Existing portal launcher, private bootstrap and same-port HTTP+WS CSP:
  `clients/portal/nix/nixos-module.nix` and `runtime-policy.nix`.
- Idle predicate: `iteration/no-game.jq`. Python accepts precisely its two
  terminal `app.session.status` error codes, never `Ok` or transport failure.
- Refreshed rollback topology: `iteration/keep.sh`.
- Native observer/replayer: `clients/linux/diagnostics/NATIVE-README.md`.

The paths under one temporary stage are operational evidence, not a persisted
configuration schema. No device paths are discovered by controller-name guesses.

## Parent preparation (off-device)

Build/download approved prebuilt artifacts through existing producers. The parent
artifact inventory is `/tmp/korri-native-device-trial/artifacts.json`. Build no
software on the target; do not turn off Nix signature checks. Every ELF interpreter
and resolved RPATH library must already exist on the target. The runner calls each
prebuilt ELF interpreter with `--list` **before** any service mutation. It does not
fetch dependencies. Cache failure stops the trial.

Stage these dereferenced regular files. Directories are root:root 0755; executables
0755; other public inputs 0644. The stage's `baseline`, `control` and `evidence`
directories are root:root 0700. Do not upload credentials into public inputs.

| Staged path | Exact producer/input |
|---|---|
| `target.sh`, `run.py` | This directory |
| `bin/inputplumber` | `inputplumber-korri/bin/inputplumber` |
| `bin/korri-inputd`, `bin/korri-input-seat-receiver` | `korri-inputd/bin/…` |
| `bin/korrid` | `korrid/bin/korrid` |
| `bin/korri-portal-shell`, `bin/korri-replay-native-dpad` | New `korri-portal-native-input-probe/bin/…` |
| `assets/` | New probe's `share/korri-native-input-probe/portal/` (passive counters) |
| `inputs/receiver.service` | Fully generated production core receiver unit, not a hand-written approximation |
| `inputs/seat.rules` | Fully generated `input-seat-rules.nix` output with observed runtime GID |
| `inputs/InputPlumber.policy` | Patched provider's `share/polkit-1/actions/org.shadowblip.InputPlumber.policy` |
| `inputs/metadata.rules` | Exact `korri-input.nix` polkit block with observed `korri-inputd`/`korri` identities; retain denial |
| `inputs/nginx.conf` | Full observed nginx config, with only the same-port HTTP+WS `connect-src` delta |
| `inputs/native-verdict.py` | New probe package's offline parser |

Generate `payload.sha256` on the workstation with one `SHA256␠␠relative/path`
record for every regular payload file, including both scripts and all assets.
Do not include the manifest itself, baseline or evidence. No symlinks, traversal,
unlisted files or group/world-writable inputs are accepted.

The runner preserves actual effective InputPlumber `XDG_DATA_DIRS` and inputd
`KORRI_INPUTD_PROFILE_PATH`; it does not substitute generic provider data. It reads
only named whitelisted environment values, never dumps capabilities/environments.
UIDs/GIDs come from existing accounts. The generated receiver/rules must match.

## Required device inspection by parent

Use the already pinned SSH configuration; this tool does not make SSH calls.
Before invocation, inspect—not invent—the following:

1. Exact old signed receiver unit, complete fragment/drop-ins, installed setup
   artifact and `ExecStopPost`. Confirm cleanup deletes its GID 980 and
   `/run/udev/rules.d/99-z-korri-sunshine-input.rules`. Do not edit the signed setup.
2. Current nginx `-c` config path and asset root; kiosk launcher path; provider's
   effective process argument suffix. Inputd/korrid are required to have no suffix.
3. Exact plugin registry, receipt, approval and installed artifact paths, plus
   bundle/portal selector and profile paths. Pass them as repeated `--guard-path`.
   These are copied privately and byte/link/mode checked, not printed or rewritten.
   Do not pass `/var/lib/korri-plugin-host` wholesale: its live lock must not be
   copied/restored. Pass the observed entries inside it and immutable plugin paths.
4. Installed target Python `/nix/store/…/bin/python3` and ordinary runtime tools:
   bash, coreutils (`timeout`), util-linux (`flock`, `runuser`, `findmnt`), systemd,
   curl, busctl, pkaction and udevadm. No `nix-shell` is used.
5. Baseline service states: all eight touched existing services must be active,
   inputd `Ready`, kiosk unfrozen, core receiver absent, session idle. Baseline
   korrid must not require masked Sunshine/old receiver for isolated recovery.
6. Review service hardening/source-ACL helper compatibility. Existing helpers are
   not replaced. Keep persistent `90-swaymsg-canonical.conf` and
   `91-volume-socket.conf` unchanged. Ordinary service state writes are not
   redirected; inspect settings paths before running a newer brain against data.

No firmware, partitions, images, kernels, receipts, approval or persistent unit
files are written. Runtime service operations can still write their normal logs
and state. This is a RAM-only **component selection**, not a storage sandbox.

## Parent invocation

Call the script with installed bash. `target.sh stage` checks model, spare SD CID,
root/boot partitions and tmpfs, then returns `/run/korri-input-trial.XXXXXXXX`.
Upload the payload into that directory. Run the parent-owned invocation in a
supervised session that survives SSH disconnect; allow **450 seconds** for the
candidate deadline, rollback deadline and independent verification together.

```
bash STAGE/target.sh run STAGE /nix/store/OBSERVED-PYTHON/bin/python3 \
  --old-receiver OBSERVED-SIGNED-RECEIVER.service \
  --asset-root OBSERVED-ASSET-ROOT --nginx-config OBSERVED-CONFIG \
  --kiosk-launcher OBSERVED-LAUNCHER \
  --guard-path OBSERVED-PLUGIN-REGISTRY \
  --guard-path OBSERVED-RECEIPTS --guard-path OBSERVED-APPROVAL \
  --guard-path OBSERVED-INSTALLED-PLUGIN --guard-path OBSERVED-SELECTOR
```

Upper-case operands are inspection requirements, not proposed filesystem paths.
Pass each actual provider argument as `--provider-arg=VALUE`, including `run` if
present; the runner compares them with the running provider before cutover.

The outer shell holds the actual plugin-host `flock` throughout candidate and
rollback. It stops kiosk → Sunshine → korrid → inputd → old receiver → provider.
Only after signed cleanup completes does it install core rules. It saves/moves
existing runtime fragments before creating masks. It parks only the owned
`60-input-view.conf`. It copies complete policy directories and bind-mounts them
read-only inside **polkit only**. It bind-mounts assets/config inside **nginx only**.
The core receiver unit changes only `ExecStart`. The kiosk uses a private copy of
the production launcher with only the shell executable replaced by the observer.
No credential/bootstrap code, Chromium binary, browser input or plugin binary is
replaced.

The runner starts provider → core receiver → korrid → inputd, checks actual PIDs,
root socket authority, masks and idle, then starts nginx/kiosk. When
`control/ready` appears, the parent has 45 seconds to obtain the new protected
physical composite/target/event mapping. Atomically publish **three newline-
terminated lines** to root-only `control/replay-target`:

1. The actual InputPlumber composite object path.
2. Its actual normalized xb360/gamepad target object path.
3. That target's protected `DevicePaths` evdev node.

The replayer independently validates membership, provenance, descriptor, opened
node identity and strict neutrality before each Right pulse. The runner checks
inputd authorization and action-user denial. Keep real controls, keyboard and
mouse untouched. No confirm, OS key, game launch, raw/seat fallback or CDP input.
See the probe handoff for the kernel-grab delivery limitation. A failed replay is
not a reason to loosen the grab or use a browser Gamepad injection.

For the first rollback exercise add `--check-rollback`. It sends the candidate
Python process SIGTERM after readiness, before replay. A nonzero candidate exit is
expected; require independent rollback verification. This is not a navigation pass.

## Leave running for a manual test

Use `target.sh manual STAGE PYTHON` with the same observed arguments as `run`.
This mode performs no replay. It checks actual candidate readiness twice, then
leaves services running without an automatic rollback timer. Setup or verification
failure still triggers the original rollback path. Use `manual-check` for a fresh
idle-readiness check, and `restore` explicitly after the user finishes and exits
any game. Do not restore merely because the automated setup command has ended.

Keep the private baseline and RAM payload until manual testing ends. Do not run
plugin install or restore operations while the candidate owns the input stack.
Signed Sunshine is stopped during this test, so remote streaming is unavailable.
The candidate is temporary, not a permanent installation.

## Rollback and verdict

EXIT, HUP, INT and TERM invoke rollback; the candidate has a 180-second deadline.
Rollback has its own 180-second deadline; fresh verification gets another 45
seconds. A deadline cannot prove recovery when a service/kernel is unresponsive.
Never report success from a timeout, `Ready`, a socket, or `restore.log` alone.

Rollback first requires exact idle RPC. If the candidate coordinator is down,
it recovers **only** the baseline coordinator with kiosk/Sunshine stopped, then
requires exact idle before changing input components. An active coordinator with
unavailable or nonterminal RPC fails closed. Parent must resolve that safety gate;
the runner never interprets a failed RPC as idle or stops an observed live game.

Rollback removes only fingerprint-matching owned runtime files. It restores the
original fragments/policy/provider, then starts the original signed receiver so
its own setup recreates its group/rules. It starts baseline korrid/inputd and
recomputes the owned browser view from **current** restored sysfs parents. It does
not restore stale `TemporaryFileSystem` paths. Sunshine/kiosk restart only after
idle and inputd readiness. Fresh verification checks effective drop-ins, binaries,
states, policy bytes, protected file hashes, group, environment and live topology.
The original view bytes remain in private baseline evidence for comparison.

Independent parent invocation (also acquires the plugin lock):

```
bash STAGE/target.sh verify STAGE /nix/store/OBSERVED-PYTHON/bin/python3
```

If needed, use `restore` in the same form. Never run a second candidate over a
failed baseline. No automatic cleanup deletes the baseline on failure. A partial
failure may leave an uncaptured private diagnostic screenshot in the old kiosk
runtime directory; inspect it before another trial rather than deleting unknown
files. Captured screenshots are fingerprinted and removed during rollback.

Copy only `evidence/` off-device; keep it private. Keep baseline on-device private
unless a separately approved inspection needs it. Evaluate with the exact parser
from the same native probe package:

```
python3 run.py verdict LOCAL-EVIDENCE --native-verdict NATIVE-PROBE/native-verdict.py
```

Acceptance needs completed native replay, bounded real native-only adapter
counters, actual DOM focus movement, a private screenshot and a separate fresh
rollback check. Review the screenshot itself. Evidence files are not cryptographic
attestations; parent must execute the independent check, not merely copy a stale
marker. Native-only counters and focus evidence do not prove remote streaming.

Prepared host checks (not run by this worker):

```
python3 nix/devices/rpminiv2/native-iteration/run.test.py
bash -n nix/devices/rpminiv2/native-iteration/target.sh
```

Cost: component outages, multiple restarts, strict metadata/neutrality gates, and
private evidence that needs manual handling. The runner deliberately rejects
unknown baseline shapes rather than becoming a general deployment framework.
