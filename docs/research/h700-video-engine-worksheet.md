# H700 video-engine worksheet

The user approved three checks on the RG35XX Pro. Software streaming is excluded. This worksheet records hardware decode and encoder feasibility, not a working streaming host.

| Step | State | Required evidence |
|---|---|---|
| 1. Add the H616 VE series. | Built and checked. | All 18 delta settings survived configuration. Cedrus module and three board DTBs passed inspection. |
| 2. Boot and decode H.264. | Pending step 1. | Running kernel, cedrus device, decoded frames and hardware execution. |
| 3. Compare encoding registers. | Checked. Verdict unresolved. | The H616 manual omits the encoding register interface. Bootlin's interface is now identified. |

## Baseline

At 2026-10-02 01:30:16 UTC, the Pro answered over USB at `10.42.6.1`, SSH port 2222. It ran system `/nix/store/brww8qrskjz3bmzjnfpxckyzd5k3nshr-nixos-system-rg35xxpro-sd-card-26.05.20251221.a653104` and kernel `/nix/store/b8kilc1kcy6ycdmvbmm2052c755l6cba-linux-aarch64-unknown-linux-gnu-7.2/Image`.

InputPlumber, inputd, korrid, compositor, and kiosk were active. No game or failed unit was active. Sunshine was installed but inactive. No video/media device or VE tree node existed. Device compilation stayed disabled, and signatures stayed required. The current system and selected Korri bundle remain the rollback references.

Raw baseline: `/tmp/h700-ve/baseline.log`. Local scripts: `/tmp/h700-ve/run-baseline.py` and `device-baseline.sh`. They copy a read-only probe, then execute it with the installed shell.

## 1. H616 video engine

Source: Chen-Yu Tsai's [v3 seven-patch series](https://ratatoskr.run/lkml/2026/07/17247254/t), 2026-07-12. The series adds H616 binding and driver support, the VE node, and related H6 SRAM/IOMMU corrections. It reuses the H6 decoder variant. It adds no encoding.

Korri carries the seven source patches as `nix/devices/h700/kernel/patches/0224` through `0230`. `Link` headers identify each source message. The patch extractor retains the published diffs and review tags. The ROCKNIX base configuration stays unchanged. The Korri delta enables `VIDEO_SUNXI=y` and `VIDEO_SUNXI_CEDRUS=m`, as required by the actual kernel Kconfig.

The module check first failed at `kernel.config.isYes "VIDEO_SUNXI"` before enabling the driver. Command: `nix build --no-link ./.worktree/h700-video-engine#checks.x86_64-linux.h700`.

All seven patches applied to the unmodified Linux 7.2 files with zero fuzz. The cross build on Zao uses exact commit `9f8591ee207183ab454ce605ff2cb60fdc173484`. The cross build finished with exit 0. Checks `h700`, `h700-inputplumber`, and `korri-base` passed. `/tmp/h700-ve/verify-kernel.sh` checked all 18 delta settings in the generated `.config`. It checked the VE MMIO address, clocks, both IOMMU ports, and both SRAM references in the Pro, SP, and SP v2-panel DTBs. The compiled `sunxi-cedrus` module advertises the H616 compatible and aarch64 Linux 7.2 vermagic. These are artifact checks, not live decode evidence.

## 2. Hardware decode

No deployment or decode test has run yet. A `/dev/video*` node alone will not count as decode success. The test selects GStreamer 1.26.5's `v4l2slh264dec` explicitly. Its plugin uses the Linux stateless decoder API. The pipeline contains no automatic decoder selection or software fallback.

Zao generated an offline H.264 baseline fixture with FFmpeg 8.0.1, then decoded its NV12 reference. This is test-fixture preparation on a build machine, not software streaming. The Pro receives only the fixture and prebuilt tools. The fixture contains 120 frames at 640x480, 60 fps. The reference has 55,296,000 bytes.

| Artifact | SHA-256 |
|---|---|
| `fixture.h264` | `95a8aa7e418bf6fbe9c754db2762ad24da247ed4a14610d830f8e44952e2d5d2` |
| `reference.nv12` | `0c891fbb2e0740e006286f92c89aae79c19785f9e43439893ff5338c9ede6b53` |

The test will compare the actual decoded bytes with that reference and record VE interrupts and request ioctls. Nothing compiles on the Pro. No card or firmware write is needed. Scripts and logs are in `/tmp/h700-ve/`.

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
