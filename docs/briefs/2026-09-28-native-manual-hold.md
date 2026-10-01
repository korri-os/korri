# Parent conversation update: native controller manual test

## Read this first

The candidate was last verified running on the Mini V2 for the user's manual test. **Do not restore the old stack, restart services, or remove the RAM stage merely because automated checks finished.** Wait for the user's manual results or explicit direction.

This update records the last verified state from `proc_09a9`. Writing this document did not contact the device again. Recheck identity and current session before any later operation. A marker or an old process result does not establish current readiness after a reboot.

## What happened

The user objected that the automated component trial restored the original stack before manual testing. The earlier trial result remains valid, but it was not manual acceptance. On request to correct this, restage the same prebuilt core candidate and leave it running.

The new explicit `manual` mode starts the candidate through the existing guarded transaction, requires exact readiness and a second actual verification, and then disarms automatic rollback only on success. Failed setup or failed verification still attempts rollback. No replay is injected. The original snapshots and explicit restore command remain available. A user session must end before the existing idle-only restore can run.

This retains temporary selection under `/run`. It is not a permanent installation. Signed Sunshine remains stopped and masked during the native physical-input test; remote streaming is unavailable. Plugin receipts and signatures remain unchanged. Do not run plugin restore/install commands while the temporary candidate owns the input stack.

Tests execute the actual shell finish handler in subprocesses. Manual success calls only verification, never restore. Manual verification failure reports failure after restore/verify. Automatic trials and failed setup retain their original rollback behavior.

Status: candidate is running for the user's manual test. Parent process `proc_09a9` completed startup and a separate actual `manual-check`, both successfully. The stage is `/run/korri-input-trial.URiPGSUr`. It verified candidate executables, inputd readiness, kiosk state, runtime masks and idle RPC. No replay ran and no rollback timer is armed. The workstation record is `/tmp/korri-native-manual/`; its `restore-command` contains the exact explicit recovery command. Leave the candidate running until the user directs otherwise.

An earlier staging-only attempt included a workstation-generated Python bytecode directory and failed the public-mode guard before service changes. The fresh payload excludes bytecode. Thirty-two host tests passed, including three actual shell-finish subprocess cases. Manual button and gameplay results still require the user's report.

## Landed code

Repository: `/home/simonwjackson/code/sandbox/korri`. All three commits below are on local `main`. Their feature worktrees and branches were removed. Unrelated working-tree changes were preserved. No remote push was performed in this work.

| Commit | Result |
| --- | --- |
| `73639cf3` | Connected native evdev capture, authenticated local korrid WebSocket delivery, configurable shared physical/remote seats, and session reservations. |
| `5f2715b7` | Fixed the missing WebSocket CSP allowance and added the reversible image-free Mini V2 trial and diagnostics. |
| `13d8b5fb` | Added explicit manual mode, which retains a successfully verified candidate instead of automatically restoring it. |

The earlier implementation passed full korrid/inputd/browser checks and isolated kernel/security tests. Strict korrid Clippy still has documented baseline findings; do not report every lint gate green.

## Device and retained state

| Item | Last verified value |
| --- | --- |
| Device | Retroid Pocket Mini V2, aarch64. |
| Boot media | Spare SD: root `/dev/mmcblk0p2`, boot `/dev/mmcblk0p1`. |
| SD CID | `1d41445553440000200000365a019700` |
| Boot ID | `8679b6fb-2dd2-48a8-9805-231a56adfa57` |
| Current system | `/nix/store/kij0kw2k0pz450ilnkpg27vmcm0dwiaw-nixos-system-rpminiv2-sd-card-26.05.20251221.a653104` |
| SSH | Alias `rpmini` in `/tmp/rpmini-iteration-ssh.conf`, with pinned host-key checking. |
| Retained RAM stage | `/run/korri-input-trial.URiPGSUr` |
| Private rollback snapshot | `baseline/` inside that stage. Do not delete it. |
| Local records | `/tmp/korri-native-manual/activation.log`, `stage`, and `restore-command`. |

Manual mode uses the prebuilt native core stack and production portal assets. The test-only shell observer remains staged. No synthetic replay ran during manual activation. The old signed Sunshine service and its receiver are stopped and masked, so remote streaming is unavailable. No replacement Sunshine plugin was admitted. Existing receipts, signatures, private bootstrap, swaymsg override, and volume override were preserved.

The user approved service restarts and component testing, not image rewrites or kernel, firmware, loader, or internal-storage changes. No such writes ran. No software was compiled on the handheld. The earlier Wario Land 4 session was closed through the exact-session stop RPC and was not relaunched.

## What is actually proven

The earlier automated device trial (`proc_dbcb`) injected four Right pulses into the independently validated normalized InputPlumber evdev target. In 4.00544 seconds, native-event count increased from 19 to 35 and DOM focus moved through nodes 5, 9, 11, and 13. Browser Gamepad adapter starts and reads stayed zero. The parent inspected the captured JPEG and separately verified rollback.

Evidence: `/tmp/korri-native-device-trial/evidence-Ejeg3PNn/`. This proves the normalized evdev-to-native-portal path. It does not prove real physical button presses, raw normalization, gameplay, or remote-controller acceptance. Manual results are still pending.

The automated trial originally restored too soon for manual testing. That mistake is corrected: `proc_09a9` started the candidate again and independently checked actual executables, inputd Ready, kiosk state, owned overrides, masked old producers, and idle RPC. It exited successfully while leaving the candidate running, with no rollback timer.

## Next action and recovery

Ask for or consume the user's manual test results. Keep the candidate running while they test. Do not confuse successful activation with manual acceptance or permanent deployment.

If the user requests restoration, first establish that no game is active. Use the exact command saved in `/tmp/korri-native-manual/restore-command`, after checking it against the retained stage and device identity. The underlying target command is:

```sh
bash /run/korri-input-trial.URiPGSUr/target.sh restore \
  /run/korri-input-trial.URiPGSUr \
  /nix/store/zc0vzc0vp0amc9iqm3cd542bmim6vh09-python3-3.13.9/bin/python3
```

Run a separate `verify` invocation afterward. Restore refuses an active or uncertain game session. Do not weaken that guard, overlap seat owners, or run plugin install/restore operations while the temporary candidate owns input.

The runner preserves socket activation ordering and gates kiosk autostart. Rollback must start all original input producers, including signed Sunshine, before refreshing the owned topology-dependent browser view. Restoring old sysfs parent numbers verbatim previously failed verification.

Further details: [component trial and evidence](2026-09-28-native-input-component-trial.md), [implementation and limits](2026-09-28-unified-controller-input.md), and [operator runner](../../nix/devices/rpminiv2/native-iteration/README.md).
