# H700 video-engine worksheet

The user approved three checks on the RG35XX Pro. Software streaming is excluded. This worksheet records hardware decode and encoder feasibility, not a working streaming host.

| Step | State | Required evidence |
|---|---|---|
| 1. Add the H616 VE series. | Built and checked; not landed. | All 18 delta settings survived configuration. Cedrus and three board DTBs passed inspection. Runtime display faults block normal deployment. |
| 2. Boot and decode H.264. | Decode verified; display-DMA issue open. | 120 hardware requests, 120 VE interrupts, and actual decoded bytes identical to the reference. |
| 3. Compare encoding registers. | Checked. Verdict unresolved. | The H616 manual omits the encoding register interface. Bootlin's interface is now identified. |

## Baseline

At 2026-10-02 01:30:16 UTC, the Pro answered over USB at `10.42.6.1`, SSH port 2222. It ran system `/nix/store/brww8qrskjz3bmzjnfpxckyzd5k3nshr-nixos-system-rg35xxpro-sd-card-26.05.20251221.a653104` and kernel `/nix/store/b8kilc1kcy6ycdmvbmm2052c755l6cba-linux-aarch64-unknown-linux-gnu-7.2/Image`.

InputPlumber, inputd, korrid, compositor, and kiosk were active. No game or failed unit was active. Sunshine was installed but inactive. No video/media device existed. The original tree probe did not follow the `/proc/device-tree` symlink, so it cannot establish absence of a VE node. Device compilation stayed disabled, and signatures stayed required. The baseline system and selected Korri bundle are the rollback references.

Raw baseline: `/tmp/h700-ve/baseline.log`. Local scripts: `/tmp/h700-ve/run-baseline.py` and `device-baseline.sh`. They copy a read-only probe, then execute it with the installed shell.

## 1. H616 video engine

Source: Chen-Yu Tsai's [v3 seven-patch series](https://ratatoskr.run/lkml/2026/07/17247254/t), 2026-07-12. The series adds H616 binding and driver support, the VE node, and related H6 SRAM/IOMMU corrections. It reuses the H6 decoder variant. It adds no encoding.

Korri carries the seven source patches as `nix/devices/h700/kernel/patches/0224` through `0230`. `Link` headers identify each source message. The patch extractor retains the published diffs and review tags. The ROCKNIX base configuration stays unchanged. The Korri delta enables `VIDEO_SUNXI=y` and `VIDEO_SUNXI_CEDRUS=m`, as required by the actual kernel Kconfig.

The module check first failed at `kernel.config.isYes "VIDEO_SUNXI"` before enabling the driver. Command: `nix build --no-link ./.worktree/h700-video-engine#checks.x86_64-linux.h700`.

All seven patches applied to the unmodified Linux 7.2 files with zero fuzz. The cross build on Zao uses exact commit `9f8591ee207183ab454ce605ff2cb60fdc173484`. The cross build finished with exit 0. Checks `h700`, `h700-inputplumber`, and `korri-base` passed. `/tmp/h700-ve/verify-kernel.sh` checked all 18 delta settings in the generated `.config`. It checked the VE MMIO address, clocks, both IOMMU ports, and both SRAM references in the Pro, SP, and SP v2-panel DTBs. The compiled `sunxi-cedrus` module advertises the H616 compatible and aarch64 Linux 7.2 vermagic. These are artifact checks, not live decode evidence.

## 2. Hardware decode

The Pro booted the candidate at 2026-10-02 02:15:03 UTC. Its new boot ID was `9b21b731-fb83-4dc9-8d56-822b1d761024`. The resolved `/run/booted-system/kernel` Image matched the candidate, and the live H616 VE compatible matched. Cedrus bound to `1c0e000.video-codec` and registered `/dev/video0` and `/dev/media0`. A device node alone was not treated as decode success. The test selects GStreamer 1.26.5's `v4l2slh264dec` explicitly. Its plugin uses the Linux stateless decoder API. The pipeline contains no automatic decoder selection or software fallback.

Zao generated an offline H.264 baseline fixture with FFmpeg 8.0.1, then decoded its NV12 reference. This is test-fixture preparation on a build machine, not software streaming. The Pro receives only the fixture and prebuilt tools. The fixture contains 120 frames at 640x480, 60 fps. The reference has 55,296,000 bytes.

| Artifact | SHA-256 |
|---|---|
| `fixture.h264` | `95a8aa7e418bf6fbe9c754db2762ad24da247ed4a14610d830f8e44952e2d5d2` |
| `reference.nv12` | `0c891fbb2e0740e006286f92c89aae79c19785f9e43439893ff5338c9ede6b53` |

At 02:25:07 UTC, the explicit decoder completed all 120 frames. The actual 55,296,000 decoded bytes had the reference SHA-256. The local runner copied those bytes from the Pro and compared them directly with `reference.nv12`; they were identical. The trace contained 120 successful `MEDIA_REQUEST_IOC_QUEUE` calls, and the VE interrupt count rose from 0 to 120. This proves hardware H.264 decode for this fixture. It does not prove sustained 60 fps, other profiles, or encoding.

Nothing compiled on the Pro. No card image or firmware partition was written. The selected bundle stayed `/nix/store/i6r8grbkrcfdiv483n5r4p1sn6k57xl9-korri-bundle-0.0.0`. Activation stopped the live mGBA game and recorded its new 32,768-byte SRAM save and 44,574-byte automatic state at 02:12:38 UTC. Scripts and logs are in `/tmp/h700-ve/`; copied actual pixels and ioctl traces are in `after-decode/h700-ve/`.

The first decoder probe failed before decoding because the Nix wrapper sets `GST_PLUGIN_SYSTEM_PATH_1_0`, which took precedence over the unversioned path in the probe. Direct plugin loading and the corrected versioned path both exposed the hardware decoder. The successful pipeline explicitly included the core, base, good, and bad plugin directories.

Sunshine restarted during activation and boot. Its startup logs showed failed attempts to open `libx264`, not a working encoder. The test stopped both `korri-sunshine.service` and its certificate-control socket. The plugin remains installed. No software decoder or streaming fallback was used by the test.

### Display-DMA issue — high impact

The candidate also logged master-0 IOMMU read faults at physical display-buffer addresses. They began at boot uptime 10.154 s, just after the VE joined the IOMMU group at 10.098 s. The live device tree declared IOMMU membership only for the VE; neither display mixer belonged to it. Linux's `sun50i_iommu_enable()` enables translation globally with bypass set to zero. The ROCKNIX display mixer selects its own device as DRM's DMA device, but that device has no IOMMU declaration.

This is consistent with the display still using physical DMA addresses after the VE enables shared translation. That cause is inferred, not yet verified by a corrected-kernel test. The five product services were active with no failed units, but that does not establish correct physical display output. Recent three-second probes showed no new faults after the display's idle interval; the last captured fault was at uptime 326.791 s. The kernel is not accepted for normal use on that evidence. The safety rollback completed. The baseline kernel booted with ID `38d21e8d-0e5d-4c45-a8c6-8cb5cb43912e`; all five product services were active, the selected bundle was unchanged, and the kernel log contained zero IOMMU page faults. Sunshine and its control socket stayed stopped.

Source review approved patch `0231-h616-display-iommu.patch`: associate both display mixers with IOMMU port 0. The H616 manual's Table 3-8 on page 207 identifies port 0 as DE, port 1 as DI, and ports 2/3 as VE_R/VE. `sun8i_mixer_bind()` selects the first bound mixer as DRM's DMA device; GEM allocation and PRIME import use that device. The planes driver only owns MMIO and plane sharing. Both mixers must therefore join through port 0, not through the planes node or port 1. The IOMMU driver's `generic_single_device_group` shares one domain across them and the VE.

This keeps the direct kernel-DT route and its no-overlay check. The two-line association changes display allocation/import to translated DMA.

#### Corrected candidate — decode verified, display blocker remains

Commit `290d88ae` cross-built successfully on Zao in 500 seconds. All 18 configuration settings and all three DTBs passed inspection, including port 0 on both mixers. Fuji built and signed the Pro system in 46 seconds. The corrected system is `/nix/store/3maz0c9dh414q58w68af4d5zp0d1mgja-nixos-system-rg35xxpro-sd-card-26.05.20251221.a653104`; its Image is `/nix/store/aah8nm53kr0zm28jy1qxhdnml1jmxspl-linux-aarch64-unknown-linux-gnu-7.2/Image`.

It booted at 03:20:33 UTC with ID `bc4f98d1-6371-49c0-9dc3-bf76b91a537b`. The original physical-address fault storm stopped. A subsequent test decoded the fixture and sent its frames to the existing Wayland compositor at the same time. Local NV12-to-BGRx conversion prepared the preview; this was neither software decoding nor streaming.

The preview harness first failed because the sink's early fullscreen request had no window size. Its default window then displayed frames but timing/QoS dropped 16 decoded output frames. Turning off decoder and sink QoS produced all 55,296,000 bytes, which were copied from the Pro and compared byte-for-byte with the reference: identical.

The same boot contained **18 master-0 read faults at translated addresses `0xFFC49000` and `0xFFC00000`**, at uptime 365.313–365.578 s. The retained successful concurrent run started with 18 faults and ended with 18. It added none. The earlier worksheet wrongly attributed that burst to this run. Panel preparation messages occur inside the earlier burst, but its initiating display transition was not captured. This is not the original unassociated-mixer physical-address fault. The safety gate rejected the corrected kernel despite correct decoded pixels. Idle output and active services do not close this blocker.

The second safety rollback completed. Boot ID `c6e1be8e-bb70-49f1-845e-63a4e0cc3da1` ran the baseline `b8kilc1kcy6ycdmvbmm2052c755l6cba` Image. All five product services were active, the selected bundle was unchanged, and the fresh kernel log contained zero IOMMU faults. The rollback stopped the user's new live game and recorded its 32,768-byte SRAM save and 45,687-byte automatic state at 03:30:46 UTC. Activation succeeded; an operator helper then tried to stop already-unloaded Sunshine units and failed before reboot. A verified reboot-only continuation completed the rollback. Neither candidate is accepted for normal use, and the kernel work has not landed on `main`. The user explicitly approved extending the scope to fix display-DMA before landing. Encoder implementation remains excluded. Raw evidence remains in `/tmp/h700-ve/candidate-kernel-journal.log`, `hardware-decode.log`, `after-decode/h700-ve/`, and `decode-history/`.

#### Approved display isolation

The user approved fixing display-DMA before landing. The corrected candidate was reactivated with trusted closure signatures refreshed directly in the Pro's store. Signature verification and the no-device-build policy stayed enabled. Boot ID `590d966b-9fb5-4c98-a618-ee9eb0b83f0d` ran the exact `aah8nm53kr0zm28jy1qxhdnml1jmxspl` Image.

At 04:03 UTC, raw display, decoder-only file output, and decoder plus preview each completed with zero fault delta. Both decoder outputs had all 55,296,000 bytes and matched the reference. The subsequent native compositor snapshot showed the panel powered off. These clean preview controls therefore do not establish active scanout safety. The test script also initially misreported raw-display success as failure because `journalctl --grep` returns 1 for no matching entries. The corrected script does not treat that empty result as a fault.

The display-only off/on test uses the exact native `swaymsg output DSI-1 power off/on` operation from `korri-display-idle`. No game or decoder runs. The first test added 35 faults across two wakes: stopping an already-idled `swayidle` woke the panel before the measured explicit cycle. The refined test stops idle first, establishes a powered-on precondition, and verifies on/off/on using actual compositor output records.

The refined test at 04:07:49 UTC added 18 master-0 faults. Before off, plane 41 held framebuffer 73, a local 640x480 XR24 GEM allocation at DMA address `0xFFC00000`. The off snapshot had no framebuffer 73 and no active plane. CRTC enable occurred at uptime 848.882608 s. Faults at that old address began at 848.884762 s and ended at 849.120028 s, across panel preparation. Only at 849.124402 s did the driver program the replacement address `0xFFA00000`. This reproduces the blocker without decoder activity and strongly supports stale scanout during re-enable. CPU page-table contents were not captured. No kernel fix is yet verified.

Evidence: `/tmp/h700-ve/display-cycle-1790914067285944288/` contains the full bounded native DRM log, before/off/after KMS state, framebuffer records, compositor output records, and test result. The feedback-loop command is `/tmp/h700-ve/run-display-cycle.py`. It fails on a nonzero fault delta and restores display power, idle service, and the original DRM debug value.

Source review approved a bounded two-file candidate, carried as `0232-h616-display-commit-order.patch`. It selects the standard atomic tail only when every bound engine has the actual H616 DE33 compatible. Both mixers use `allwinner,sun50i-h616-de33-mixer-0`; no mixer-1 compatible is invented. Other engines retain the RPM tail. DE33 now programs disabled/new plane state before TCON enable. Modeset events defer their vblank reference until CRTC enable; a tail-only switch would introduce a failed vblank acquisition and an unmatched later put. Normal active flips keep their existing path. The patch applies without fuzz, and kernel `checkpatch.pl --no-tree --no-signoff` reports zero errors and warnings. Runtime correctness is not yet verified. The cost is changed modeset-event timing and the need to retest boot handoff, wake, and concurrent active scanout. This does not fix a separate cache defect or heterogeneous-engine pipeline.

After diagnosis, baseline activation succeeded and selected the baseline Image and boot entries. Reboot was scheduled, but SSH command sessions stopped opening. The device still answered ping, presented the pinned host key, and accepted public-key authentication. Bounded command probes then timed out. The user restarted the Pro. Boot ID `7588572f-1bb3-4d42-97ed-bb211239e5c9` then ran the exact baseline Image, with five active product services and zero faults. No further device write preceded that verified recovery.

#### Commit-order candidate: improved, still rejected

Commit `f6c1aaee779c064b4011676e673fbb50937995ee` cross-built on Zao in 443 seconds. All three module checks passed. All 18 delta settings, three DTBs, and the Cedrus module passed artifact inspection. Fuji built and signed the Pro system in 29 seconds. A separate implementation review found no source defect in patch `0232`, including its modeset-event reference balance.

The system `/nix/store/4lq7ds29nd37hshvn9j7sbmk32w40398-nixos-system-rg35xxpro-sd-card-26.05.20251221.a653104` was verified at 04:34:57 UTC with boot ID `f368c52d-f709-4127-a23e-efc5e7e595da`. Its exact Image was `/nix/store/ivfnv286cd368q27fbj0rhx8qbaiyr1m-linux-aarch64-unknown-linux-gnu-7.2/Image`. The selected bundle and root filesystem stayed unchanged. Sunshine stayed disabled. Closure signature verification passed before activation.

The first controlled wake still added **one** master-0 fault, at `0xFFAC7000`. Before off, plane 41 held a local framebuffer at `0xFFA00000`, size 1,245,184 bytes. The off snapshot removed it. The corrected driver programmed `0xFFC00000` at uptime 94.432531 s, before CRTC enable at 94.434004 s. The remaining old-range fault occurred at 94.434169 s. Thus the required ordering is now observed, but one old-range read remains. CPU DTE/PTE contents and raw fault class were not captured. The next cycle added no faults, leaving the boot count at one. A zero-delta later cycle does not make the kernel acceptable.

Evidence: `/tmp/h700-ve/display-cycle-1790915757029298231/` records the failing first cycle. `/tmp/h700-ve/display-cycle-1790915918896885452/` records the later zero-delta cycle. The revised decode/isolation scripts now wake the panel through the existing native operation and check actual compositor power state before and after playback. No corrected-kernel decode result is claimed yet. The remaining display fault still blocks landing.

Baseline activation after this test passed, but remote reboot verification timed out again. After the user restarted the Pro, boot ID `396ef413-8c26-4806-951e-34aaf42608c4` passed exact-baseline Image, five-service, unchanged-bundle, and zero-fault checks. Persistent journals then showed an intervening boot, `264dd56d-ea70-48ac-974a-8e08284522df`, starting at 04:46:35 UTC. Its administrative SSH service reported listening at uptime 27.470806 s. Thus a reboot did occur; the failure was remote access after reboot. That intervening boot's exact Image and full health were not verified. This access failure also prevents unattended diagnostic comparisons until it is accounted for.

#### Temporary first-fault diagnostic artifact

Patch `0233` is a diagnostic experiment, not a proposed production change. It preserves `0232` and the existing DMA policy and recovery. It captures the first DE fault's raw interrupt class, hardware error data, CPU DTE/PTE pairs, and table base before recovery. It emits its preallocated record after the existing reset. H616 manual page 260 grounds the added read-only L1/L2 error-data offsets, `0x170` and `0x174`. A root-only read-only debugfs file supplies raw error snapshots before and after off and immediately before wake. Existing DRM debug controls bound GEM backing-memory release and layer-address readback logs to the test interval.

A separate source review found no blocker for the bounded test. Patch application passed with zero fuzz. Checkpatch reported zero errors and warnings. These checks do not prove compilation or runtime safety. Keep the VE/mixer domain attached throughout capture. Do not use the diagnostic file during suspend, domain replacement, device removal, or failed attachment. CPU table snapshots are not atomic with map/unmap. Readbacks prove MMIO visibility, not hardware latching or DMA drain. Added reads and logs can change timing. Remove the entire diagnostic patch before landing.

The next comparison must reproduce the first wake from a fresh boot. Later zero-delta cycles follow IOMMU fault recovery and cannot substitute for independent boots. Capture the instrumented unchanged-prefetch case first. Only then compare DE/master-0 prefetch off, leaving translation, permissions, other masters, and release policy unchanged. Disappearance would establish prefetch sensitivity, not safe buffer retirement.

The unchanged-prefetch diagnostic artifact at commit `b67bc28930593df98a9441476775502bef529817` built on Zao in 498 seconds. Artifact inspection passed all 18 settings, three DTBs, and Cedrus checks. Fuji built its signed system in 33 seconds. System `/nix/store/qiwfcsd4qqk0rpz66nf1brbxl6mvy2v2-nixos-system-rg35xxpro-sd-card-26.05.20251221.a653104` booted with ID `bb993e8d-4a75-4863-9d7f-d44ac1b308fa` and exact Image `/nix/store/brlq39irdznlmxy0far5y9qaqm5jrmsh-linux-aarch64-unknown-linux-gnu-7.2/Image`. The bundle stayed unchanged; closure signatures passed; Sunshine stayed inactive. No decoder ran.

This artifact faulted once during boot at uptime 13.630135 s, address `0xFFCD7000`. Its captured interrupt status was `0x20000`, with L2 blame `0x1`, CPU PTE and paired PTE zero, and hardware L2 error data zero. Software and hardware table bases both were `0x442C8000`. Translation stayed enabled, bypass stayed zero, and prefetch was `0x3F`. This is an invalid L2 translation in the attached domain, not evidence of a mismatched table base or a valid new mapping rejected from stale negative cache. The boot record does not identify the DE request's issue time or prove whether prediction caused it.

The measured off/on then added another fault: boot count 1 to 2. GEM backing-memory release for `0xFFA00000`, size 1,245,184 bytes, returned at 46.071062 s. The new `0xFFC00000` address read back from the live layer register at 46.095543 s, with both address and attribute reads successful. CRTC enable entry was at 46.097322 s. The old-range fault, `0xFFA23000`, was at 46.097560 s. Raw interrupt status was zero before off, after off, and before the native power-on command. Hardware changed its saved L2 error address after wake. These observations weaken a pre-existing latched-error explanation but do not expose a normal request queue.

Evidence is `/tmp/h700-ve/diagnostic-display-cycle-1790919860929177170/`, including the boot journal, lifecycle journal, four IOMMU register snapshots, framebuffer records, and native on/off/on confirmation. The one-shot CPU record belongs to the boot fault, not the later wake fault. The kiosk was still activating during the measured cycle; all five product services were active at the later health check. Future comparisons wait for all five to be active. No corrected-kernel decode or zero-fault acceptance is claimed.

The panel was verified off at 06:02:20 UTC while the next diagnostic comparison was prepared. Stopping compositor/kiosk together canceled the compositor stop, so both remain running. No service-supervision cause is claimed. The existing native panel operation and stopped idle service supply the temporary test state instead. This is not a production DMA workaround.

Patch `0234` is the single-variable comparison. It omits only master-0 prefetch on the actual H616 IOMMU compatible, preserving H6 behavior and every other master. The expected register readback is `0x3E`. It does not alter translation, bypass, interrupt handling, permissions, buffer release, or recovery. Its result cannot establish safe retirement by itself.

Before the corrected boot, the existing `korri-plugin disable @korri:sunshine` lifecycle operation changed its desired state to `Disabled`. The installed package `/nix/store/33w3ig2ilv4fhblc4gi4aa72l4y8xh06-korri-plugin` and approval stayed unchanged. This prevents an automatic software-encoder probe at future boots; it does not remove Sunshine or rule out later hardware hosting. The cost is that Sunshine will stay off until it is explicitly enabled after a hardware encoder route exists.

The signed candidate system is `/nix/store/ppp31zj36a8dfpr7ma4rxkyi9kmk4kjb-nixos-system-rg35xxpro-sd-card-26.05.20251221.a653104`. Its kernel Image is `/nix/store/bcbdmv8qfabzb2jf6xn8yzvcryl1xdlj-linux-aarch64-unknown-linux-gnu-7.2/Image`.

The USB link disappeared before deployment. Wi-Fi at `192.168.1.163` presented the same pinned SSH host key, which the local connection script checked before using it.

The first system copy failed before activation. Zao's existing `ksncw0src4fj5iymn8n81ifbql37nzgq-korrid` had only the untrusted `fuji-1` signature. Fuji had the required `korri-cache-fuji-1` signature too. `nix copy` skipped the existing local path and did not refresh its signatures. The successful retry imported matching builder signatures with `nix store copy-sigs --substituter ssh-ng://fuji --recursive "$system"`. Device signature checks stay enabled. The old system remained active after the failed copy.

## 3. Encoder register comparison

Downloaded the actual [H616 user manual](https://www.scs.stanford.edu/~zyedidia/docs/allwinner/h616.pdf), and Bootlin's `cedrus/h264-encoding` branch at commit `4e9497947c56f10351ed8c000605dbe47d4aee46`.

Local evidence: `/tmp/h700-ve/h616.pdf`, `h616.txt`, and `bootlin/`. The PDF has 831 pages. Its SHA-256 is `4ca95572826a1d98b687ae74c1bb2f62597b70f5aafa20d698c4ed6792b20e05`. A fresh extraction matched the supplied text exactly. The complete encoding section and the corresponding Bootlin register definitions and writes were read, not just search results.

The H616 manual gives H.264 encoding capabilities on page 14. Its VE aperture is `0x01C0E000–0x01C0FFFF`, on page 30. Section 4.5.1, pages 302–303, contains an overview and a block diagram. It describes input, reconstruction/reference, and stream buffers. It supplies no register offsets, bitfields, DMA layouts, or start/IRQ sequence. Section 4.5.2 then describes JPEG encoding; Chapter 5 starts on page 305. PDF and printed page numbers match for these citations.

All Bootlin offsets below are relative to its VE base. They are not verified H616 addresses. Source: [`cedrus_regs.h`, lines 722–921](https://github.com/bootlin/linux/blob/4e9497947c56f10351ed8c000605dbe47d4aee46/drivers/staging/media/sunxi/cedrus/cedrus_regs.h#L722-L921), [`cedrus_enc.c`, lines 57–177](https://github.com/bootlin/linux/blob/4e9497947c56f10351ed8c000605dbe47d4aee46/drivers/staging/media/sunxi/cedrus/cedrus_enc.c#L57-L177), and [`cedrus_enc_h264.c`, lines 1116–1333](https://github.com/bootlin/linux/blob/4e9497947c56f10351ed8c000605dbe47d4aee46/drivers/staging/media/sunxi/cedrus/cedrus_enc_h264.c#L1116-L1333).

| Interface | Bootlin implementation | H616 manual comparison |
|---|---|---|
| Internal mode/reset | `0x000`, encoder bit 7, ISP bit 6; `0x004`, reset bit 24. | Undocumented. CCU clock/reset registers on pages 90–91 are a different interface. |
| Input size/stride/planes | ISP bank `0xA00`; macroblock units; Y/C input addresses at `0xA78`/`0xA7C`. | No register or stride-unit definition. |
| Picture parameters | `0xB04`/`0xB08`; entropy mode, I/P slice type, QP and stride fields. | Capabilities overlap, but fields are undocumented. |
| Reference/reconstruction | `0xBA0`/`0xBA4` and `0xBB0`/`0xBB4`; auxiliary buffers at `0xBB8–0xBC4`. | Buffer roles only, not layouts or alignment. |
| Bitstream output | `0xB80–0xB90`; DMA start, inclusive end, bit offset/max/length. | No register addresses or units. |
| Start/IRQ | `0xB14` enables interrupts; `0xB18` command 8 starts encoding; `0xB1C` reports/clears status. | No programming or acknowledgement sequence. |
| Header insertion | `0xB20`; put-bits command 1, count bits 13:8, ready status bit 9. | No corresponding interface documented. |

Verified verdict: **unresolved**. The manual cannot establish matching registers for a port or incompatible registers for a rewrite. Missing documentation is not proof of incompatible hardware. Bootlin's inspected match table has no H616/H700 entry.

Next required evidence is an H616/H700 encoder register specification or the register programming of a known-working vendor encoder. That costs documentation access or reverse engineering. Working decode does not close this gap. No encoder port, vendor library integration, or Sunshine backend was added. The detailed source-inspection notes remain in `/tmp/h700-ve/register-comparison-verified.md`.
