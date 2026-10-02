# H700 video-engine worksheet

The user approved three checks on the RG35XX Pro. Software streaming is excluded. This worksheet records hardware decode and encoder feasibility, not a working streaming host.

| Step | State | Required evidence |
|---|---|---|
| 1. Add the H616 VE series. | In progress. | Applied patches, kernel configuration, compiled module and board DTBs. |
| 2. Boot and decode H.264. | Pending step 1. | Running kernel, cedrus device, decoded frames and hardware execution. |
| 3. Compare encoding registers. | In progress. | Actual H616 manual and commit-pinned Bootlin code, with an honest compatibility verdict. |

## Baseline

At 2026-10-02 01:30:16 UTC, the Pro answered over USB at `10.42.6.1`, SSH port 2222. It ran system `/nix/store/brww8qrskjz3bmzjnfpxckyzd5k3nshr-nixos-system-rg35xxpro-sd-card-26.05.20251221.a653104` and kernel `/nix/store/b8kilc1kcy6ycdmvbmm2052c755l6cba-linux-aarch64-unknown-linux-gnu-7.2/Image`.

InputPlumber, inputd, korrid, compositor, and kiosk were active. No game or failed unit was active. Sunshine was installed but inactive. No video/media device or VE tree node existed. Device compilation stayed disabled, and signatures stayed required. The current system and selected Korri bundle remain the rollback references.

Raw baseline: `/tmp/h700-ve/baseline.log`. Local scripts: `/tmp/h700-ve/run-baseline.py` and `device-baseline.sh`. They copy a read-only probe, then execute it with the installed shell.

## 1. H616 video engine

Source: Chen-Yu Tsai's [v3 seven-patch series](https://ratatoskr.run/lkml/2026/07/17247254/t), 2026-07-12. The series adds H616 binding and driver support, the VE node, and related H6 SRAM/IOMMU corrections. It reuses the H6 decoder variant. It adds no encoding.

Korri carries the seven source patches as `nix/devices/h700/kernel/patches/0224` through `0230`. `Link` headers identify each source message. The patch extractor retains the published diffs and review tags. The ROCKNIX base configuration stays unchanged. The Korri delta enables `VIDEO_SUNXI=y` and `VIDEO_SUNXI_CEDRUS=m`, as required by the actual kernel Kconfig.

The module check first failed at `kernel.config.isYes "VIDEO_SUNXI"` before enabling the driver. Command: `nix build --no-link ./.worktree/h700-video-engine#checks.x86_64-linux.h700`.

Build, DTB, and module verification results remain pending.

## 2. Hardware decode

No deployment or decode test has run yet. A `/dev/video*` node alone will not count as decode success. The test must select the stateless cedrus decoder explicitly and verify decoded output. Nothing will compile on the Pro. No card or firmware write is authorized or needed.

## 3. Encoder register comparison

Downloaded the actual [H616 user manual](https://www.scs.stanford.edu/~zyedidia/docs/allwinner/h616.pdf), and Bootlin's `cedrus/h264-encoding` branch at commit `4e9497947c56f10351ed8c000605dbe47d4aee46`.

Local evidence: `/tmp/h700-ve/h616.pdf`, `h616.txt`, and `bootlin/`. The first research agent could only inspect search snippets. Its report is not a completed comparison. A second pass reads the actual downloaded files.

Verdict and register table remain pending. Decoder support will not prove encoder compatibility.
