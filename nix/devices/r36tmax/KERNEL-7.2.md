# Linux 7.2.6 SD cutover

The owner confirmed that the diagnostic SD image boots. A command through the
USB root console returned `7.2.6` from `uname -r`. This establishes boot and
console access, not peripheral acceptance.

## Changes and checks

The kernel uses upstream Linux 7.2.6 with the retained RK3326 configuration
seed, board DTS, generic panel driver and RK915 MMC quirks. The MMC patch
follows the newer DWMMC host structure. RK915 has a source patch for removed
kernel APIs; see `wifi/README.md`.

Native and cross kernel builds passed. The native RK915 build passed its
artifact checks, including 162 imported symbol CRCs. These Nix checks passed:

- `checks.x86_64-linux.r36tmax`
- `checks.x86_64-linux.r36tmax-mmc`
- `checks.x86_64-linux.r36tmax-recovery`

Built and written artifact:

```
/nix/store/x9qqcv5v74ynax00998q62s11zgn6i5c-nixos-r36t-max.img.zst/sd-image/nixos-r36t-max.img.zst
```

The owner approved erasing the removable card reporting 53,739,520,000 bytes.
The write completed successfully and flushed. The owner requested no readback;
there is no byte-for-byte card verification. Internal eMMC remained disabled.

## Remaining acceptance

Restore owner provisioning before network tests: the existing public SSH key,
Wi-Fi environment file, and the two RK915 firmware blobs. Do not copy private
keys or put credentials in the image. See the device README for exact paths.

Verify Wi-Fi association and stability, GPU rendering, buttons and audio on
this kernel. Check temperature and browser load before sustained use. Ask for
owner readiness before each hands-on test. Suspend/resume and video codec jobs
are not accepted by this boot result. Do not resume prior faulting codec jobs
without their separate recovery and test gates.

The USB network appeared at `10.42.2.1`, but SSH authentication was not
established. USB serial root access worked through
`/dev/serial/by-id/usb-Korri_R36T_Max_NixOS_r36tmax-nixos-if02`.
