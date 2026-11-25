# gts6lwifi kernel

Mainline Linux 7.3 for the Galaxy Tab S6 Wi-Fi (SM-T860, SM8150), cross-compiled
from x86_64. The board support is harvested from
[Bentlybro/gts6l-mainline-linux](https://github.com/Bentlybro/gts6l-mainline-linux)
(the "port"; `docs/*.md` names below are its documents), which booted Fedora
on this tablet with a 6.12.0 kernel, and rebased onto 7.3.

```sh
nix build .#tabs6-kernel        # console: result/Image, result/dtbs/qcom/sm8150-samsung-gts6lwifi.dtb,
                                # result/lib/modules/<version>, result/config, result/System.map
nix build .#tabs6-korri-kernel  # product: the same plus config/korri.config
```

The kernel builds from the `tabs6-nixpkgs` flake input (the nixpkgs pin the
tabs6 repository proved on the tablet), not from Korri's nixpkgs: Korri's pin
has Linux 7.2.7, and these patches are rebased to 7.3.4. With that input the
console kernel is the same derivation as the tabs6 repository's `kernel`.

## Product delta

`config/korri.config` is merged after `tabs6.config` for the Korri product
only (`productFragment`), checked the same way: uinput and hidraw for
InputPlumber and inputd, uhid for Bluetooth LE controllers, and the gamepad
drivers (xpad, playstation, sony, nintendo, steam). The console kernel does
not change.

The product kernel also carries upstream `54e89fab723d`, which resets q6asm's
three PCM ring positions on prepare. Without it, reusing an open PCM after a
stop or xrun can leave the application and DSP ring positions out of step.
This is a proposed fix for the game-start audio stall, not yet hardware-proven;
see `docs/acceptance/tabs6-audio-stall-2026-11-22.md`. The offline regression
check compiles the shipped prepare/map/ack functions:

```sh
nix build .#checks.x86_64-linux.tabs6-q6asm --no-link
```

The product kernel also carries the Venus video codec ("Video codec" below);
its build also fails without the `qcom,sm8150-iris` node, `venus_mem` at
`0x97800000`, `qcom-iris.ko` and an enabled `sdhc_2` ("microSD" below).

`Image` is uncompressed (the EFI stub needs it; about 45 MB). The `kernel` build fails unless `Image` is a PE ("MZ") arm64 EFI-stub image and the
DTB carries the gts6lwifi compatible, the simple-framebuffer at `0x9c400000`,
MDSS enabled with the ANA38401 panel under DSI0, and `wlan_mem` at
`0xc0000000`, and unless the panel module is built.

## USB-C data-role owner (product only)

`drivers/s2mm005.c` is a read-only I2C client on se15/0x33. It registers a
standard Type-C port and owns dwc3's data role through the USB role-switch API.
`usb-typec-wire-s2mm005.patch`, `CONFIG_TYPEC_S2MM005_READ_ONLY=m` in
`korri.config`, and `dts/sm8150-samsung-gts6lwifi-typec.dtsi` wire it into the
product. The console source, DT and kernel configuration stay unchanged.

The module is loaded late in boot by `tabs6-typec-kernel-policy.service`
(after the journal flush, boot boost-off and the gadget), not by udev.
The driver polls every 100 ms without requesting or acknowledging IRQ123.
Three fresh sink-ready/attached/not-source/DFP + MUIC VBUS=01 + CNTL mode 5
samples enable host. Any other sample or error revokes at once.
No chip register data writes, PD commands, VBUS or firmware access exist.
It does not use the fuel gauge (see the linked doc). SM5705 charger/MUIC
reads use the i2c4 adapter phandle without reserving addresses.
The charge and boot boost-off services keep their existing access.

See `../usb-role/typec-kernel.md` for register grounding, manual host unbind /
device rebind, resource lifetimes, suspend, the user-space input restart
workaround and hardware limits. The host check compiles the shipped policy:

```sh
nix build .#checks.x86_64-linux.tabs6-typec-policy --no-link
```

## From NixOS

`kernel` is a `linuxManualConfig` derivation, so NixOS takes it directly
(`../system/kernel.nix` does this):

```nix
boot.kernelPackages = pkgs.linuxPackagesFor tabs6Kernel;
hardware.deviceTree.name = "qcom/sm8150-samsung-gts6lwifi.dtb";
```

nixpkgs normally puts modules in a separate `modules` output. Here they install
into `out`, so one tree holds everything; NixOS reads modules with
`lib.getOutput "modules"`, which falls back to `out`.

It is cross-built (x86_64 build, aarch64 host). A NixOS system evaluated with the
same `crossSystem` uses it as is. A natively evaluated aarch64 system must call
`default.nix` with an aarch64 package set, or the store paths differ.

The resolved config is parsed with import-from-derivation (`allowImportFromDerivation`),
so `kernel.config.isYes` and NixOS's `system.requiredKernelConfig` see the real values.

## Base

nixpkgs `linux_latest` (7.3.4 at the pinned nixpkgs). The native display pipe
needs it: bonded dual-DSI in command mode, which the panel uses, only works
from 6.16 (docs/DISPLAY.md section 5 in the port). Until 2026-11-17 the base was
`linux_6_12` (6.12.117); the patch table below records how each patch moved.
A nixpkgs bump can move the version; a patch that stops applying fails the
build.

## Config

`config/gts6lwifi_defconfig-full` is the port's 6.12.0 config, unchanged.
`config/tabs6.config` is our fragment on top. `kernelConfig` merges them and runs
`olddefconfig`; the build fails if any fragment line does not survive unchanged.
On 7.3 two base symbols needed the fragment: `IPV6` is bool now (the port had
it as a module), and the legacy iptables tables sit behind
`NETFILTER_XTABLES_LEGACY`.

The system boots without an initrd, so the root and lifeline path is built in:
UFS (`ufshcd-qcom`, QMP UFS PHY), SCSI disk, GPT, ext4, vfat, loop, DWC3 with its
HS and SS PHYs, and the configfs gadget (RNDIS, ACM). The SS PHY
(`PHY_QCOM_QMP_COMBO`) and `TYPEC` were modules in the port; both are built in
here so the gadget never waits for a module.

## Native display

The panel is driven natively: MDSS/DPU, DSI0 and DSI1 bonded in command mode
with their 7nm PHYs, and the panel driver `drivers/panel-samsung-ana38401.c`
(Samsung AMSA05RB06 with an Anapass ANA38401 T-CON, 2x 1280x1600, no DSC).
The port ran simpledrm on the bootloader framebuffer instead; its
docs/DISPLAY.md and the "display saga" in docs/DEVLOG.md record why.

| Piece | Where | Source |
| --- | --- | --- |
| On/off commands, timings, reset, delays | panel driver | stock dtbo, `ss_dsi_panel_ANA38401_AMSA05RB06_WQXGA` (T860XXS5DWH1) |
| Power sequence: VDD, LP11, reset 70 ms, wait T-CON ready (GPIO 140), HS commands, exit sleep (not in the vendor sequence; the panel stays black without it), display on >= 220 ms after exit sleep | panel driver | Samsung kernel source for T860XXS5DWH1 (`dsi-staging/dsi_panel.c`, `samsung,anapass-power-seq`) |
| Commands to DSI0 only, one transfer each, vendor packet types | panel driver, no `qcom,sync-dual-dsi` | `samsung,cmds-unicast` |
| Brightness: vendor AOR (0x90) and ELVSS table, neutral gamma | panel driver backlight, 46 levels | `ss_dsi_interpolation_ANA38401_AMSA05RB06.c` |
| TE on GPIO 8 (`mdp_vsync`), DCS tear-on | DTS, panel driver | stock `qcom,platform-te-gpio` |
| Byte clock RCGs latch only with a running parent | `clk-qcom-dispcc-sm8250-byte-clk-parent-enable` | port DEVLOG; 7.3 already does this for the pixel clocks |

Handover: simpledrm keeps the bootloader picture until msm binds. msm's KMS
init removes simpledrm (`aperture_remove_all_conflicting_devices`), then the
first modeset resets and re-initialises the panel. `clk_ignore_unused` and
`pd_ignore_unused` (needed for UFS anyway) keep the display clocks and GDSC
alive until then, and Octa VDD stays always-on so a late msm never loses the
bootloader picture to `regulator_late_cleanup`.

What it does not do: the vendor computes per-panel gamma from MTP data
(smart dimming), HBM, ACL, mDNIe and ESD recovery. None of these is ported, so
colour at low brightness can differ from stock, and brightness 0 is the
dimmest level, not off. DPMS is display-off plus reset; Octa VDD stays on.

## Video codec

Product kernel only. Mainline 7.3 has no SM8150 video codec: neither venus nor
iris lists the SoC. The Xiaomi Pad 5 (nabu) is also SM8150, and its port has
one: an Iris driver with an "Iris1, legacy VPU5" platform, from
[CFM880/nabu-iris](https://github.com/CFM880/nabu-iris), which
[MCC45TR/nabu-linux-kernel](https://github.com/MCC45TR/nabu-linux-kernel)
(branch 7.2.2) adapted to Linux 7.2. `media-iris-sm8150-legacy-vpu5` is that
directory over 7.3.4's (the patch header lists what differs), and
`media-iris-gen1-encoder-for-sunshine` adds what Sunshine's `h264_v4l2m2m`
needs, as on the Mini V2: the force-key-frame and SPS/PPS-on-IDR controls,
and the aligned raw buffer layout that upstream Iris reports.

| Piece | Where | Source |
| --- | --- | --- |
| Node `video-codec@aa00000`, `qcom,sm8150-iris` | `dts/sm8150-samsung-gts6lwifi-venus.dtsi`, appended to the board DTS | nabu `sm8150-xiaomi-nabu-iris.dtsi`; IRQ (SPI 174) and stream 0x2300/0x60 match the stock `qcom,vidc@aa00000` |
| `venus_mem` at 0x97800000, 5 MB | same file | the free gap from the modem's end to IPA. The stock 0x96e00000 is inside the 160 MB modem span; the console's 0x9f800000 is outside TZ's firmware zone (port SENSORS.md) |
| Firmware `qcom/sm8150/gts6l/venus.mdt` + `b00`..`b04` | `../firmware`, from `apnhlos` | stock T860XXS5DWH1, VIDEO.IR.1.2-00065-PROD-4 |
| `SM_VIDEOCC_8150=y`, `VIDEO_QCOM_IRIS=m`, venus off | `config/korri.config` | |
| `options qcom_iris allow_fw_boot=1` | `../hardware.nix` | the port's driver boots the firmware on first open only with it |

`/dev/video0` is the decoder (H.264, HEVC, VP9), `/dev/video1` the encoder
(NV12 to H.264 or HEVC). Like upstream Iris, the encoder's raw format has the
aligned height (736 for 720). Stock FFmpeg (8.0, 9.0) then reads past the end
of a 720-row frame and crashes. Sunshine's FFmpeg carries the fix
(`docs/acceptance/sunshine-korri-v4l2m2m-portal-2026-09-05.md`). A CLI check
gives FFmpeg room to read:

```sh
ffmpeg -f lavfi -i testsrc=duration=5:size=1280x720:rate=30 \
  -vf format=nv12,pad=1280:736,crop=1280:720:0:0 -c:v h264_v4l2m2m -b:v 4M t.mp4
```

## microSD

Product kernel only. `dts/sm8150-samsung-gts6lwifi-sdcard.dtsi`, appended to
the board DTS, enables SDHC2 (`mmc@8804000`) with the stock T860XXS5DWH1
settings: card power from `vreg_l9c_2p9`, I/O from `vreg_l6c_2p9` (1.8 V for
UHS), card detect on GPIO 96 (active low), and the stock `sdc2_*_on/off` pin
drive strengths. `sdhci-msm` is already built in.

On the tablet (2026-11-19) the controller probes (`mmc0: SDHCI controller on
8804000.mmc`, `Got CD GPIO`) and takes both regulators. No card was read:
whether the slot held one is unknown, so card detection and enumeration are
untested.

## Patches

| Patch | Source | On 6.12.117 | On 7.3.4 |
| --- | --- | --- | --- |
| `msm-poweroff-release-ps-hold-early` | port | applied | applies unchanged |
| `qcom-pon-ps-hold-shutdown-and-smpl` | port | applied | rebased: the driver renamed `pm8916_pon` to `qcom_pon`; same logic |
| `mfd-qcom-spmi-pmic-tolerate-denied-revid` | port | `-p0` made `-p1` | applies unchanged |
| `adc-battery-report-capacity-and-status` | port | rebased | applies unchanged |
| `cs48l32-accept-cs48l33` | port | applied | applies unchanged |
| `sm8250-sndcard-sm8150-secondary-tdm` | port | rebased | rebased: 7.3 dropped `hw_params` from the backend ops (added back for Secondary TDM only), already has a `QUINARY_MI2S_RX` case (replaced by ours, which also covers TX), and takes the driver name from match data (`sm8150-sndcard` keeps `sm8250`, the name the UCM `conf.d` uses) |
| `clk-qcom-gcc-sm8150-branch-halt-skip` | ours | applied | applies unchanged |
| `regulator-qcom-rpmh-no-reads` | ours | not needed | new: 7.3's rpmh-regulator reads each regulator's enable state, mode and voltage back from RPMh. Samsung's firmware never answers; each read times out after 10 s ("failed to read VOLTAGE ret = -110") and the first 7.3 boot stalled there. The patch removes the reads, which is 6.12's behaviour |
| `drm-msm-a640-speedbin-fallback` | ours | part of `drm-msm-headless-a640-render-node` | still needed: no speed-bin cell, and every sm8150 GPU OPP is gated by `opp-supported-hw` |
| `drm-simpledrm-64bit-dma-mask` | ours | part of `drm-msm-headless-a640-render-node` | moved to `drivers/gpu/drm/sysfb/simpledrm.c` |
| `input-touchscreen-wire-fts1ba90a` | ours | applied | applies unchanged |
| `power-supply-wire-sm5705-fuelgauge` | ours | applied | applies unchanged |
| `arm64-dts-qcom-wire-gts6lwifi` | ours | applied | applies unchanged |
| `clk-qcom-dispcc-sm8250-byte-clk-parent-enable` | ours | not carried | new: `CLK_OPS_PARENT_ENABLE` on the DSI byte clock sources (native display) |
| `drm-panel-wire-samsung-ana38401` | ours | not carried | new: Kconfig/Makefile for `drivers/panel-samsung-ana38401.c` |
| `net-qrtr-announce-local-services-after-hello` (product kernel only) | upstream 743b5c86276b (v7.4-rc, Cc stable), unchanged, from the tabs6 repository (feat/modem, a85332f) | not needed | new: 7.3 drops every QRTR packet but HELLO until our HELLO has gone out. When the modem's HELLO wins that race, the NEW_SERVER replies for rmtfs and tqftpserv are dropped ("failed to announce new service"), the modem watchdog fires every 40 s ("fatal error without message") and Wi-Fi comes up minutes late. Seen on the console and the Korri kernel. Drop it when the base has it |
| `q6asm-reset-pcm-positions-on-prepare` (product kernel only) | upstream `54e89fab723d5a374972d3f5c3e0b89ae055bd18`, unchanged, Cc stable | not carried | new: reset `queue_ptr`, `dsp_buf`, and `hw_ptr` when preparing a new PCM ring; candidate for the game-start stall, pending device confirmation |
| `media-iris-sm8150-legacy-vpu5` (product kernel only) | MCC45TR/nabu-linux-kernel 7.2.2 (0b3226fc8f3c), from CFM880/nabu-iris | not carried | new: SM8150 video codec ("Video codec" above) |
| `media-iris-gen1-encoder-for-sunshine` (product kernel only) | ours, ported from the Mini V2's three gen1 encoder patches | not carried | new: FORCE_KEY_FRAME, PREPEND_SPSPPS_TO_IDR and the aligned raw layout for Sunshine |
| ~~`dwc3-allow-userspace-role-control`~~ | port | applied | dropped: upstream in 7.3 (`dwc3_setup_role_switch` sets `allow_userspace_control`) |
| ~~`asoc-cs48l32-backport-from-6.18.63`~~ | ours | applied | dropped: CS48L32 is upstream since 6.18 |
| ~~`drm-msm-headless-a640-render-node`~~ (headless master, forced MMU) | ours | applied | dropped: with MDSS enabled the GPU is a component of the display master, the normal upstream topology (the kernel-bump commit used msm's `separate_gpu_kms=1` instead) |
| ~~`ufs-qcom-keep-phy-powered-across-clock-gating`~~ | ours | applied (6.12.y only) | dropped: it undoes a half-backport that exists only in 6.12.y; 7.3 has both halves of the upstream UFS PHY power change |

The port's `fts1ba90a-per-command-echo-timeout` and
`fts1ba90a-resume-drop-redundant-ready-wait` are not carried: the port's
`fts1ba90a.c` already contains both.
