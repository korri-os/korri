---
id: r36tmax-standalone
title: Complete standalone R36T Max NixOS and Korri
status: active
created: 2026-09-14
source: direct
---

# Complete standalone R36T Max NixOS and Korri

The user approved all eight steps and autonomous execution. Preserve the working generic panel driver. Never treat a source audit, build, or emulation result as physical-device acceptance. No internal storage writes belong to this work.

## Execution tracker

| Step | Work | State | Acceptance |
|---|---|---|---|
| 1 | Package the working loader, kernel, DTB, LZO initrd, modules, and safe diagnostics. | Final recovery image built; actual file image verifies labels, FAT/root generation and complete module subtree. | A complete image boots without a kernel override. |
| 2 | Package prior RK915 work and owner-controlled SSH. | Exact-kernel compilation and 159-symbol ABI check pass. MMC/DTS integrated. Diagnostic image has key-only SSH. | Association, transfer, reboot reconnection, and key-only SSH pass. |
| 3 | Promote to nix/devices/r36tmax and signed cache publishing. | Source move committed in 77fee02a0485. Public app checks pass. Signed publication and empty-store module download pass on the host. | Device checks, image build, and signed prebuilt downloads pass. |
| 4 | Integrate the existing Korri brain, portal, and kiosk. | Real configuration checks pass; public and diagnostic images built and file-verified. Memory remains unmeasured. | Real display and measured memory pressure on the 1 GB board. |
| 5 | Compare generic DSI and ST7703. | Exact-source audit and four negative controls pass. No causal display fix established. | Defensible patch followed by physical display and power-cycle proof. |
| 6 | Correct the actual PMIC supply model. | Unsupported switch claims corrected. No guessed descriptors or voltage changes. Actual backlight feed unresolved. | Supplies follow primary hardware evidence, not assumed RK809 descriptors. |
| 7 | Build mainline U-Boot with Rockchip DDR initialization. | Build, FIT-loadables checks and six real-artifact rejection tests pass. Separate image candidate built. | Cold starts, warm restarts, and SD recovery pass. |
| 8 | Produce, deploy, and verify the final image. | Hardware unavailable; no write or deployment attempted. Normal boot-generation installation explicitly blocked. | Fresh-card Korri boot, recovery, and updates pass. |

## Verified starting state

- The working branch starts at `a5955c0be93468383b7e45efa3ce597ce20b66c9`.
- Prior photo evidence shows a NixOS root console with the generic panel driver. This run has not rechecked the handheld.
- Both removable reader slots report zero bytes. The USB inventory shows no handheld. No card write is possible or attempted.
- The `fuji` build machine answers as `aarch64`. It is not the target device.
- The archived `rocknix-loader-full.bin` matches the loader extracted from the `a` release image, not `b`. Its SHA-256 is `0c88ade1572385515331cb6ea1c1ba6368c6ef867ef51ad59a57cafbfcff6c7a`. Reconcile this with the actual card before claiming an exact card copy.

## Current evidence and blockers

- Initial complete recovery artifact: `/nix/store/p9hhh3mz98sy8y45s782hyn17kdx8zby-nixos-r36t-max.img.zst`. Rooted at the existing local state directory's `complete-recovery-image`. It predates radio integration and review fixes; do not present it as final.
- Integrated cross kernel before firewall additions: `/nix/store/k53xa1rplv6i3rcmdw8hxi5vif4gyhym-linux-aarch64-unknown-linux-gnu-6.12.63`. Its compiled DTB proves 25 MHz SDIO, PA2 active-low, PA5 level-high wake, and disabled eMMC.
- New firewall kernel/module build adds NFT_CT, NFT_LOG, NFT_COMPAT, NFT_LIMIT, NFT_REJECT and XT_PKTTYPE. The retained configuration did not satisfy the pinned nft-backed iptables firewall.
- Impure builds no longer inherit the shared Wi-Fi credential copy. Two public configurations were tested with a dummy environment file; neither population command includes it.
- Normal NixOS boot-generation installation now fails explicitly. The preserved loader reads FAT, while the generic installer targets root `/boot`. Until a transactional FAT installer passes real rollback tests, use full SD reimaging. Cache publication does not solve that boot-update contract.
- R36T Max image artifact/release publication is held in CI until the preserved loader's notices and corresponding-source obligations are met. System closure cache publication does not include that loader or private radio firmware. The actual publication completed in 495 seconds, adding 73 paths / about 69 MiB.
- The exact panel comparison proves matching 22 register payloads but different schedules. Generic waits 592/80 ms and emits duplicate tail commands. These facts do not establish which change will make ST7703 draw.
- RK817 selection comes from OF match data, not an ID read. The extra RK809 switch descriptors are not a valid RK817 fix. No electrical backlight-feed evidence is available.
- A worker accidentally printed Brave and Serper API keys into local logs. The known run logs were redacted. Conversation/session copies cannot be retracted; the keys require owner rotation. No log or credential was published.

## Final host artifacts

The source promotion is `77fee02a0485`; FIT and CI coverage follow-up is
`71b70b76568f`. Final review found all eight earlier source/tooling blockers
addressed. The last two coverage notes were then addressed by the six-test
U-Boot rebuild and the selected-device image CI gate.

All four compressed streams were tested with `zstd -t` and hashed. Each store
output is rooted under the existing local state directory using its package
name. The three preserved-loader images passed the updated actual-image
verifier before compression. The mainline image's builder compares raw loader
bytes; separate root/extlinux inspection supplies its additional file checks.
These checks are not physical boot acceptance.

| Package | Store output | Compressed bytes | SHA-256 |
|---|---|---:|---|
| r36tmax-sd-image | /nix/store/1d1hi4ig2cb4kkf9fm38grrvq9prwf1q-nixos-r36t-max.img.zst | 1036983831 | 6a5efc95d4264197137e6459d5bd6738e62c60ea6096b862954a8cb3bbef0262 |
| r36tmax-recovery-sd-image | /nix/store/4sb8rahqpqh4ra06ivqpqkapy2q4627p-nixos-r36t-max.img.zst | 473758300 | 6fd5f5e368ad6f5b5f700b68b939b98ee9f00e71c2c90f85bab179004ccafd8d |
| r36tmax-diagnostic-sd-image | /nix/store/rxk27gbqqswd6waph4p9fsiqa6lc3b5y-nixos-r36t-max.img.zst | 1036319358 | 6a35972939e46d42fbee6e7b4b286af4d15839b21b31c72550e797a316b85a35 |
| r36tmax-mainline-sd-image | /nix/store/1iylihw7bs88dh6ldk2sx596q4f8cnvb-nixos-r36t-max-mainline.img.zst | 452219102 | e36a676b681bb68f370983eec550b63b4bdb703fb7be263bc5eea9c26179140c |

Files live under each output's `sd-image/` directory. The first three are
`nixos-r36t-max.img.zst`; the fourth is `nixos-r36t-max-mainline.img.zst`.
The public/diagnostic raw images are about 5.34 GB. Disk size does not measure
runtime RAM use.

Native kernel: `/nix/store/sxc0f9nz7c2a2mhm6rwpnif8y7pcrwpg-linux-6.12.63`.
Updated U-Boot: `/nix/store/qp0mff0a9czxj7w2m2adzpfl492rs6wr-uboot-odroid-go2_defconfig-2025.10`.

## Signed-cache proof

The public native system closure and all kernel outputs were published to
`https://github.com/korri-os/nix-cache/releases/download/cache/`, batch
`batch-2026-09-14`. The actual cold test created a new local store, required
trusted signatures, disabled local/remote builds and fallback, then downloaded:

`/nix/store/9i6mdx10p18c68k47r9fx9j98wqi6bn0-rk915-6.12.63-unstable-2025-07-08`

The module SHA-256 matched
`9fe0ae286fdfd7e480b27d05da2c73b9646075aef3a226ec5bebea4a5f0359b9`.
This is real remote-cache delivery proof on the build host, not ARM execution
or a physical-device update. The initial cache-miss probes returned 404;
the publisher completed successfully and the later signed download passed.

## Boundaries

Buttons, sticks, sound, and rumble are not included. Keep release SSH policy separate from owner-approved diagnostics. Do not put Wi-Fi credentials or private keys in source, build outputs, or published images. Do not enable builds or disable signature checks on the device. No new product data schema or UI layout is required.


## Physical portal restart diagnosis and temporary fix

The diagnostic card booted and reached pinned SSH at `192.168.1.195`.
Its saved boot ID is `d34efbf7-915d-4b39-9795-1bb2083b13d5`. Wi-Fi, the
Panfrost render node and the display compositor are working in this boot.

The selected private kiosk repeatedly failed to open the retired
`/run/korrid-browser/brain.json`. Current korrid does not produce that file.
The launcher then killed Chromium and systemd restarted it. The initial
snapshot had 39 kiosk restarts, zero compositor/brain restarts and zero OOM
kills. This was an integration error, not an observed memory kill.

The fix uses the existing `clients/portal/nix/nixos-module.nix` credential
contract and native `window.KorriRpc` shell. It removes the incompatible
private kiosk/static server from this device, retains Pico, and adds no
compatibility file or sandbox exception. The old-consumer assertion failed
before the fix; device/base/shared-portal checks pass afterward. Review found
no source blocker.

The signed diagnostic closure
`/nix/store/86nzj98zyqx0w2cz6ikw4xzgb5gwjnkn-nixos-system-r36tmax-sd-card-26.05.20251221.a653104`
was copied privately and activated with `switch-to-configuration test`.
Missing per-path signatures from the SD image were imported as native Nix
metadata from the build host. Recursive signature verification then passed;
no trust key, signature requirement or device build setting was weakened.

The first temporary activation stayed up, but the acceptance script incorrectly
used Android-only `system.settings.snapshot`. The safety timer restored the
old looping configuration. The second activation used supported `system.health`
and completed activation plus acceptance in one remote operation. Authenticated
health returned Ok; an unauthenticated browser request returned 401. The old
kiosk/static server/socket stopped. The new browser reported zero restarts,
and the rollback timer was cancelled. Direct hashes of the first 144 MiB and
the permanent system profile were unchanged. This fix is temporary: reboot
still selects the old image.

At acceptance, MemAvailable was 464160 kB, no swap was configured, and oom_kill
was zero. These are startup observations, not a game workload or leak test.
Actual screenshot `portal-fixed-live.png` in the local diagnostic logs shows
Pico rendering a distinct `HostLibraryInvalid` error, not a browser crash loop.
The catalog RPC reports `plugin evaluation failed: No such file or directory`.
Its real missing input is `/run/korri-plugin-host/enabled-packages.json`;
the plugin-host unit and administrator state directory are absent. Do not
fabricate that report or add a missing-file fallback. The existing administrator
producer must initialize it through its normal policy.

The unrelated networkd wait-online unit still fails and makes test activation
return 4 despite the new services starting. Its follow-up is
`01M2GJVDYB8HZRC44JZV1VHNVF`. Other retired kiosk consumers are tracked under
`01M2GJV1N2FBKP3D0SFQPMY5N5`. Neither follow-up was folded into this fix.

## Autonomous hardware survey after pausing application work

The owner redirected this session to safe autonomous hardware checks, with
physical actions deferred until requested. Full evidence and limits are in
[nix/devices/r36tmax/HARDWARE-SURVEY-2026-09-14.md](../../../../nix/devices/r36tmax/HARDWARE-SURVEY-2026-09-14.md).

- No runtime service, boot, voltage, governor, charging, mixer or input setting
  was changed. No target build, stress workload or storage write test ran.
- The existing workload reports SoC/GPU temperatures around 83–87°C and active
  thermal cooling states. Browser accounting consumed 17.25 CPU-seconds in
  10.21 wall-seconds. Additional load is withheld pending a cooler baseline;
  the browser is still running, not silently stopped.
- A five-minute, 600-probe Wi-Fi test lost 38 replies and had a 5.52-second
  maximum RTT. A subsequent concurrent one-minute handheld/router control had
  no loss. This is an unexplained intermittent failure, not accepted reliability.
- Root filesystem expansion is verified against the card's reported capacity.
  Panfrost clients, Hantro VPU nodes, RK817 ALSA and headphone detection exist;
  those are not physical audio/input or video-decode acceptance.
- The actual built kernel contains RK817 charging support. The live PMIC has
  no charger DT child; its charger platform device is unbound. Exact 6.12.63
  probe code explains the resulting absence of power supplies. An empty child
  cannot provide telemetry; do not guess a battery profile to force a probe.
  Early voltage-calibration reads are distinct from later gauge-state writes.
- Game controls and rumble have no board consumers. A button-press session is
  premature until source-grounded input integration exists.
- Final read-only check retained the same boot and browser invocation, zero
  browser restarts and zero OOM kills. The UI's catalog issue remains paused.

Next owner input is needed for a temporary browser-pause/cooling comparison
and physical placement/power context. Suspend, reboot, voltage changes,
full-card integrity, battery calibration, audible playback and cable changes
remain unperformed. The original eight-step acceptance is still incomplete.

## Owner-approved cooldown and dock follow-up

The owner approved pausing Chromium and changed the power connection from the
charger to a laptop dock. The same boot and runtime system remained active.
Chromium is now intentionally stopped; compositor, korrid and SSH remain up.
No reboot, suspend, boot default, manual voltage, charger, governor, mixer or
input configuration change was made. Details and exact logs are in Stage 2 of
the hardware survey linked above.

- SoC/GPU readings fell from 85.384/84.230°C before the stop to about 64–66°C
  within two minutes, and later about 56°C. Both thermal cooling states cleared.
- A guarded 30-second single-core workload passed at a sampled peak of 62.5°C.
  This does not accept sustained all-core or GPU stress.
- A 32 MiB SD scratch copy and direct-read SHA-256 comparison passed. Scratch
  cleanup was checked. No raw block-device write or full-capacity test ran.
- USB NCM, the observed host address/prefix and pinned SSH are verified. ACM console
  output is verified without sending commands or retaining raw console text.
- Verified 8 MiB payload transfers passed both ways over Wi-Fi and USB. A later
  concurrent ten-minute run returned all 600 probes on Wi-Fi, USB and router.
  The earlier loss remains unexplained; changed conditions prevent causal claims.
- A Wayland EGL context reports Mali-G31/Panfrost and GLES 3.1 Mesa 25.3.2.
  Hantro enumerates JPEG-only output on its encoder, plus parsed H.264/MPEG-2
  and VP8 input on its decoder. No actual codec job was tested.
- The final check found zero OOM kills, about 668 MiB available memory, no
  matching kernel errors and the expected intentionally paused browser.
- ALSA `Headphones Jack` reads off and both PCM nodes are closed. The owner has
  no wired headphones available, so plug/unplug testing is deferred. No sound
  or capture was initiated.

The browser-load issue is parked as `01M2GS38QWXK9HM7ENCMAW6GXG` while application
work remains paused. Battery properties, game controls, rumble and the remaining
physical/power-cycle acceptance are still open. The original eight-step work
is not complete.


## Owner-observed backlight check

With Chromium paused, the owner confirmed the gray-screen sequence at
brightness 833 → 416 → 208 → 833 out of 1666, followed by removal of the gray
layer. Driver readbacks and normal cleanup agreed. An independent restoration
timer was armed and then cancelled after successful restoration; it did not fire.
This accepts dimming/restoration at those levels only. No panel-power, mode,
boot, voltage or governor change occurred. Full range, blank/wake and suspend
remain untested. See Stage 3 of the hardware survey for evidence.


## Remaining-hardware execution started

The owner approved starting controls/rumble, battery/audio, codec jobs and
coordinated recovery work. Host source audits are underway; battery values
remain gated on board-specific evidence. A first bounded JPEG encode failed
with a VPU IOMMU read fault at 0x80 and watchdog timeout. Cleanup then disabled
IRQ 25 (`ff442800.iommu`). No JPEG was produced. The device remains reachable
and cool, with Chromium paused, but no more encoder/decoder jobs may run in
this boot before coordinated recovery. No reboot, system suspend, charging
change or speculative register access occurred. See Stage 4 of the survey.
