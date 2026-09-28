# Native-input component trial on Mini V2

Status: normalized evdev-to-native-portal navigation passed on the actual Mini V2 in `proc_dbcb`. The original services, policies, signed-plugin state, and topology-aware browser view are independently restored and verified. No image build, image write, kernel/firmware change, or on-device build ran. Physical button/raw-normalization and remote Sunshine acceptance are not claimed.

## Authority and limits

The user answered approval question `f474001e-99b4-4b8c-854d-42f6b53d4338` with: "You can restart any service. Device is plugged in. Go". This approves the component-only test on the spare SD, including necessary service restarts and rollback. It does not authorize kernel, firmware, loader, internal-storage, or SD-image writes. Build only on workstations/build machines. Keep plugin signatures and permissions in force. Never copy a replacement Sunshine executable over an approved signed plugin merely to bypass admission.

Parent session owns all device effects and visible build processes. Subagents prepare diagnostics and advise off-device only.

## Observed baseline

Read through the pinned SSH host key in `/tmp/rpmini-iteration-ssh.conf` on 2026-09-28:

- Device model is Retroid Pocket Mini V2. SSH reports aarch64.
- Root is `/dev/mmcblk0p2`, mounted as NIXOS_RPMINIV2. Boot is `/dev/mmcblk0p1`, label RPMINIV2. Card CID is `1d41445553440000200000365a019700`.
- Current system is `/nix/store/kij0kw2k0pz450ilnkpg27vmcm0dwiaw-nixos-system-rpminiv2-sd-card-26.05.20251221.a653104`.
- Boot ID is `8679b6fb-2dd2-48a8-9805-231a56adfa57`.
- Inputd retains its existing `90-swaymsg-canonical.conf` and `91-volume-socket.conf` persistent overrides. The kiosk retains the per-boot `60-input-view.conf`. These are rollback data, not files to silently delete.
- Old signed Sunshine and its plugin-owned receiver are active. The new core receiver unit is absent. Inputd still creates separate game/portal targets.
- Cache policy is max-jobs=0, builders empty, fallback=false, require-sigs=true.

The first preflight refused to proceed because Wario Land 4 was running and the kiosk was frozen. The parent then sent the existing exact-session stop for observed launch `f816871483dd7485af62171fc38c037e`. Process `proc_553b` completed successfully and verified idle through the existing terminal SessionStatus outcomes. The rollback baseline for component changes is this idle state, not a claim that unsaved live emulator state can be recreated.

## Source correction found before staging

The production Nginx CSP in `clients/portal/nix/nixos-module.nix` permitted HTTP RPC but omitted the native WebSocket endpoint. The updated module regression failed on the original policy in `proc_9e54`. The narrow change permits only HTTP and WS on the same existing loopback korrid port, retaining no-connect policy without a kiosk. The module check passed in `proc_ab64`. Browser/device verification remains required; a Nix assertion is not proof of navigation.

## Trial requirements

Stage prebuilt core binaries and static portal assets only after verifying their runtime dependencies. Preserve the baseline unit/drop-in bytes and restore them on failure. Do not overlap old plugin-owned and new core-owned seat receivers. Preserve the private window.KorriRpc bootstrap. No CDP input delivery or Chromium rebuild. A test-only private-pipe observer can inspect bounded focus/visibility/animation facts without credentials, text, or arbitrary URLs.

The existing iteration runner tests the retired browser Gamepad path. Its replay and verdict cannot be reused as proof of the new native path. The new trial must prove validated normalized evdev input reaches native semantic navigation and real DOM focus, then verify rollback separately. Service readiness or socket existence alone is not a pass.

Remote Sunshine acceptance requires an actually admitted signed replacement or a separately scoped unchanged-producer test. A core physical-input pass must not be reported as full remote-controller acceptance.

## Device attempts and recovery

Core binaries and the native diagnostic package are prebuilt. Payloads contain no replacement Sunshine binary. The diagnostic package built in `proc_89fa`; its parser fixture was corrected to place a restart inside the tested window, without weakening the verdict. Isolated kernel check `proc_d11f` proved that the actual D-pad encoder reaches the exclusively grabbed evdev reader and not the other reader.

The SSH account's private umask made initially copied public payload files unreadable to sandboxed services. Preflight refused before service mutation. Preserving the prepared public modes fixed that staging error. A second preflight found a trial-validator regex that omitted the bare namespace prefix used by the existing polkit denial. That was corrected and regression-tested before another stage.

Attempt `/run/korri-input-trial.9kzSRWEE` stopped at coordinator readiness, before replay. The original services restarted, but independent verification detected that the regenerated legacy browser view omitted a joystick-like node created when signed Sunshine restarted. Parent repaired only that owned view after all original producers were running. A fresh complete verification passed in `proc_639a`.

The observed cutover order stopped korrid before inputd. The still-running inputd status poll could reactivate the old daemon through its socket. Device journal shows korrid restarted 0.192 seconds after stopping. The expected staged executable never produced a journal entry. The runner must stop clients and the private control socket before the daemon, and block kiosk auto-start from its existing parent Wants/PartOf links until the stack and view are ready.

The normalized controller was read without grabbing or writing in `proc_c4b4`: zero held buttons, all eight axes zero. The baseline browser view was refreshed for the restored topology, rather than blindly restoring old input-node numbers.

## Accepted device result

The corrected runner stops clients before the activation socket and daemon. It holds an owned native condition gate on kiosk startup until the correct stack is ready. Rollback starts all original producers, including signed Sunshine, waits for stable topology, regenerates the existing owned view, and only then releases the kiosk. The socket check uses root UID, korrid GID 976, and mode 0660. Actual RPC readiness is required in addition to unit activation.

`proc_fbf2` reached candidate readiness and deliberately interrupted before replay. Original services and live topology passed independent verification. This was a rollback test, not navigation acceptance.

`proc_dbcb` then ran the actual normalized-target trial at `/run/korri-input-trial.Ejeg3PNn`. The parent discovered the tuple through the current unique InputPlumber DBus owner, physical source mapping, TargetDevices, DeviceType, and protected DevicePaths. The replayer independently rechecked mapping and the actual open descriptor. No name-only event-node selection, raw-device injection, grab removal, browser event injection, or direct focus call was used.

| Observation | Actual result |
| --- | --- |
| Replay window | 4.00544 seconds, four Right press/release pulses. |
| Native events in the window | Counter increased from 19 to 35, including report boundaries. |
| DOM focus | Backend node sequence 5, 9, 11, 13. Four controls were observed, with three transitions. |
| Native adapter | One start and one initialization throughout the measured interval. |
| Browser Gamepad API | Zero adapter starts and zero reads. |
| Visibility/focus | True throughout 14 in-window observations. Largest in-window sample gap was 292.169 ms. |
| Screenshot | Actual 21,426-byte JPEG inspected by the parent. Settings is visibly focused. |
| Trial and discovery | Both exited zero. |
| Rollback | A separate fresh verifier exited zero after restoration. |

Private local evidence is `/tmp/korri-native-device-trial/evidence-Ejeg3PNn/`: observer.log, replay.log, screen.jpg, summary.json, restore.log, and verification.log. The screenshot contains actual page output, not a generated mockup. Evidence stays private rather than entering the repository.

The input boundary was the real normalized InputPlumber evdev target. This does not prove a physical button generated that event or that raw normalization works for every controller. Signed Sunshine was stopped for the component trial and restored unchanged. Its remote-controller path was not accepted by this test. Test-only counters add small per-event overhead and sampled observations cannot exclude every concurrent input or sub-sample focus change.

All installed programs used existing runtime libraries. Core candidate binaries, diagnostics, assets, policies and unit overrides were staged in RAM. Existing signature checks and plugin receipts/approval stayed intact. The owned browser view necessarily received current sysfs parent numbers on rollback. Existing swaymsg and volume overrides were preserved. The original stopped Wario session was not relaunched. After another complete baseline verification, `proc_4bd1` removed all four owned inactive RAM payload stages. Private evidence remains on the workstation.
