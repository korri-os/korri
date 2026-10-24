# RG353M RKMPP kernel support restoration

## Scope

Restore the RG353M kernel driver and device-tree binding needed by Sunshine's
Rockchip H.264 encoder. Keep the existing Linux 7.2 kernel, Sunshine plugin,
and MPP userspace unchanged. Temporary driver and stream tests pass. Persistent
boot acceptance remains pending.

## Reproduction

On 2026-10-24, `haku` booted Linux `7.2.0` with both `/run/current-system` and
`/run/booted-system` pointing to:

`/nix/store/q31gbqf6vf2bsna1wk18vj4qwaybgkpi-nixos-system-haku-sd-card-26.11.20260923.4975466`

The installed module tree contained neither `rk_vcodec` nor `rk_mpp_overlay`.
The live device tree contained no MPP or RKVENC nodes. `/dev/mpp_service` was
absent. Installed Sunshine tried `h264_rkmpp`, logged `Failed to init MPP
context: -1`, and ended its probe with `Unable to find display or encoder
during startup.` The same failure affected the main and trimmed Sunshine builds
in the preceding A/B check.

Hantro was bound to `fdea0000.video-codec` and `fdee0000.video-codec`. Those
engines are separate from the missing RKVENC block at `fdf40000`. Loading or
unloading Hantro does not restore the RKVENC service.

## Cause and change

Commit `dc0d6beae379a02c92918ccc637976e13e62b57f`, the shared product-module
composition, deleted the board's MPP driver and binding along with its old
Sunshine host configuration. The product restored streaming as a plugin, but
the device configuration did not retain the required kernel support.

This change restores the driver sources, module derivation, and overlay exactly
from `dc0d6beae^`. Their original introduction is `13b908dfe412`. The older
[RKMPP acceptance record](sunshine-korri-rg353m-rkmpp-2026-09-05.md) names
`a635d309`, which is not a local Git object. Use the actual introduction and
restoration commits for source attribution.

`nix/devices/rg353m/rk-mpp-service.nix` now supplies only hardware configuration:

- Build the external module against the device's selected kernel.
- Load `rk_vcodec` and the existing `system_heap` kernel module at boot.
- Merge the RKVENC service, encoder, and IOMMU nodes into the boot DTB.
- Give the existing `video` group access to `/dev/mpp_service` and
  `/dev/dma_heap/system` with mode `0660`.

The image imports this module. It does not restore the old userspace packages or
Sunshine service. The runtime overlay helper remains a diagnostic tool and is
not loaded automatically.

Restoring only the driver is insufficient with the current MPP userspace. The
first live test created `/dev/mpp_service` and bound `mpp_rkvenc`, but installed
Sunshine then failed its buffer allocations and crashed. There was no
`/dev/dma_heap` directory. MPP 1.1.0's DRM allocator opens only `renderD128` or
`card0`, both Panfrost on this boot, and uses a vendor Rockchip GEM ioctl those
nodes do not implement.

The unchanged MPP source at `c08762ebfadeb4e986d2fed993bc7a54862d3ebe` selects its
DMA-heap allocator ahead of DRM when a DMA heap is available. Its
`osal/mpp_allocator.c`, `osal/mpp_runtime.c`, and
`osal/allocator/allocator_dma_heap.c` ground the heap selection and fallback to
`system`. Loading `system_heap` and granting only the existing video group
access made the installed Sunshine encoder probe pass without a userspace
change. The rule matches only the `system` heap, not every DMA heap.

## Verification

Before the fix, `checks.x86_64-linux.rg353m-mpp` failed with:

`RG353M image must contain its RKMPP encoder kernel module`

The check reads the module and merged DTB selected by the actual image. It
checks the kernel ABI, DMA-BUF namespace, RKVENC driver symbol, enabled device
tree nodes, encoder register range, and IOMMU link. It also asserts boot loading,
overlay selection, and the device permission rule. These checks do not prove
that hardware encoding works.

The selected kernel remains:

`/nix/store/1dq3inb878q6xinpj44kd8d6lniaa1x5-linux-7.2`

This is the same kernel output selected by the running device. Independent
static review found no issues and confirmed that the restored source, build
recipe, and overlay match the prior version.

The restored modules compile unchanged against Linux 7.2. Their signed output
is `/nix/store/ijgzs39iybskd7a69k62y6a06z747swj-rk-mpp-service-rkvenc-7.2`.
The extended module/DTB check also rejects an image without `system_heap` and
checks the actual heap module's ABI. It passed as
`/nix/store/rf7kgy65hja0kplv5qm7lp0ps4wpii36-rg353m-mpp-check`.

All eight image checks passed with the final heap support. They cover MPP,
audio, Bluetooth, native ARM diagnostics, the Nixpkgs registry, firmware,
InputPlumber, and product composition.

The approved temporary test loaded the driver and runtime overlay on the
unchanged booted system. Both device nodes were `root:video 0660`. Installed
Sunshine logged `Found H.264 encoder: h264_rkmpp [rkmpp]` at 04:48:46 UTC.
The full system build remains pending. Persistent boot acceptance is not claimed.

### Native-resolution stream tests

One baseline and one trimmed Sunshine run used the same kernel, compositor,
installed service sandbox, and hardware. The helper changed only the service's
`ExecStart` through a temporary drop-in. Baseline is plugin commit `b36dc92`;
trimmed is `832874f`. Neither is the separate upstream Sunshine port.

Moonlight Embedded 2.7.0 requested H.264 at 640x480, 30 fps, and 10 Mb/s. Each
client ran for 75 seconds. Sunshine CPU was sampled over 40 seconds mid-stream.

| Build | Received frames | Active seconds | Received FPS | Missing frame numbers | Sunshine CPU |
| --- | --- | --- | --- | --- | --- |
| Baseline | 2,203 | 73.379 | 30.009 | 0 | 37.3% of one core |
| Trimmed | 2,206 | 73.463 | 30.015 | 0 | 36.9% of one core |

Both server logs report `Using zero-copy KMS DRM_PRIME capture for RKMPP`.
RKVENC IRQ counts increased by 2,206 and 2,209 respectively, including setup.
No RKMPP allocation failures, encode timeouts, or stream crashes appeared in
these two runs. Auto-probe errors for unavailable NVENC, VAAPI, and V4L2 encoders
remain in both startup logs and are not stream failures.

The headless receive observer captured the actual H.264 payloads. FFmpeg 8.0.1
then decoded every captured frame without errors. `ffprobe` counted exactly
2,203 and 2,206 decoded 640x480 H.264 frames, matching the observer reports.
This establishes encoded stream validity, not live displayed decoding, physical
input, or audible audio. One run per build is not a performance soak.

Local evidence is under `/tmp/rg353m-kernel-test/`, including `results-rg353m/`,
`installed-probe-with-heap.txt`, and the decode and cleanup scripts. Raw video
captures stay outside the repository.

Run the module/DTB check on a build host:

```sh
nix build --no-link -L .#checks.x86_64-linux.rg353m-mpp
```

The existing diagnostics check executes the selected ARM `v4l2-ctl`, so run it
on the native ARM builder rather than as an x86 check:

```sh
nix build --no-link -L .#checks.aarch64-linux.rg353m-diagnostics
```

## Deployment boundary

At reproduction time, `/` and `/boot` were on `/dev/mmcblk0p2`, with the eMMC
boot devices also present. Do not assume the historical SD-card deployment
instructions describe the current boot medium. No raw storage or firmware
writes are part of this fix.

The user approved a temporary module test and cleanup reboot. The helper
restored the installed Sunshine executable, removed the test service drop-in,
and removed the temporary pairing selection before reboot. The runtime overlay
was not unloaded.

After the cleanup reboot, `/run/booted-system`, `/run/current-system`, and the
persistent system profile still selected the original `q31gbqf6…` generation.
The compositor, korrid, and Sunshine services were active. The runtime overlay,
MPP node, test permission rule, and pairing selection were absent, as expected
on that unchanged generation. The extlinux file's SHA-256 remained
`a5e1337e16d9abdeeed80c1a77b6242ab05dce40732ddc4744bdd90d99f85e7b`.

This is cleanup verification, not acceptance of the fixed image after boot.
The unchanged generation still lacks RKMPP support. A persistent image update
requires separate approval.
