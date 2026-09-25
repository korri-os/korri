# Retroid Pocket Mini V2 kernel

This is the SM8250 kernel from ROCKNIX distribution commit
`e81d1fc943458fb13cffe1646761e9452b29ddc1`. The producer selects **Linux 7.2**
with archive SHA-256
`f9fef3d14c0df53819026f4be74459835c2a0b0dcbf5b5bbd9ea19f0829402b3`.
The configuration is anchored to a working Mini V2 boot of official ROCKNIX
release `20260901`. The primary boot `Image`, DTB and modules all build from the
vendored source with GCC 15.2 and Binutils 2.44. No prebuilt kernel substitution
remains.

## Vendored sources

All source paths below are relative to that distribution commit. Patches and
device trees are byte-for-byte copies. Downloaded bytes were checked against
the Git blob IDs in the pinned recursive Git tree before vendoring. The stable
`20260901` release uses the same Linux archive and Mini V2 device-tree sources.

| Local path | Producer path |
| --- | --- |
| `patches/0000-mainline-*` | `projects/ROCKNIX/packages/linux/patches/mainline/*` |
| `patches/0000-version-*` | `projects/ROCKNIX/packages/linux/patches/7.2/*` |
| Remaining `patches/*` | `projects/ROCKNIX/devices/SM8250/patches/linux/*` |
| `dts/sm8250-retroidpocket-common.dtsi` and board DTS files | `projects/ROCKNIX/devices/SM8250/linux/dts/qcom/*` |
| `config` baseline | `/proc/config.gz` from official ROCKNIX `20260901` running on the target Mini V2 |
| `config-tty-trim` | Hardware-proven TTY trim derived from that full baseline |
| `config-korri` | Product delta restoring gamepad, fan PWM, PMIC thermal sensors, and the power key |

`projects/ROCKNIX/packages/linux/package.mk` and `scripts/unpack` define the
order: four mainline patches, three version patches, then 30 SM8250 patches,
with each queue sorted by filename. The prefixes preserve that order under
one sorted glob. The project package overrides `packages/linux`; its unrelated
`default` patches do not enter this queue.

No SM8550 queue was copied. The SM8250 producer itself contains patches whose
names mention other handhelds; those remain part of its exact queue. The
common `0010-msm-resource-cleanup.patch` remains enabled. Odin's hardware-specific
exclusion and local codec patches do not apply here.

The three DT files are the V2 DTS, the Mini DTS it includes, and their shared
DTSI. V2 overrides the panel compatible to `ch13726a,rpminiv2` and the touch
coordinates to 1080 by 1240. The common DTSI still includes the kernel's native
SM8250 and PM8150 family files.

## NixOS differences

The full `config` is the configuration exported by the working ROCKNIX
`20260901` kernel. That boot bound DSI, DisplayPort and the Adreno GPU before
registering DRM and `fb0`. It also showed the same initial
`DSI PLL(0) lock failed` warning as the failed NixOS image, proving that warning
is recoverable and is not by itself the display failure.

The first Nix source build used GCC 14.3 and Binutils 2.44. It stayed black even
with the full configuration, embedded firmware, exact GRUB handoff and a
byte-identical DTB. Rebuilding the same source with GCC 15.2 and Binutils 2.44
produced a working DSI display, DRM framebuffer, VT1 and USB serial console.
Matching ROCKNIX's Binutils 2.47 was therefore unnecessary.

`config-tty-trim` removes 727 enabled feature symbols from that proven compiler
baseline while retaining MSM DRM/DPU/DSI, the CH13726A panel, framebuffer
console, SD root, VFAT boot, AUTOFS, the SM8250 combo and high-speed USB PHY
drivers, and modular USB serial. The
hardware-proven output has these properties:

- `Image`: 18,383,360 bytes, SHA-256
  `cb88bd10b292d8202b49920ad9ff49cb6aa05f0aef4a94ec78b10a3caa38d71b`
- Mini V2 DTB: SHA-256
  `f9e32c33e14f3d974c461c674435a7002c73ec4243e96e3aef158620a730eee4`
- module closure: 444 KB containing `g_serial` and its four selected function
  modules

`config-korri` starts from that exact trim. It restores
`CONFIG_INPUT_JOYSTICK`, modular `CONFIG_JOYSTICK_RETROID`,
`CONFIG_INPUT_MISC`, `CONFIG_INPUT_UINPUT`, and
`CONFIG_INPUT_QCOM_SPMI_HAPTICS`. The patched Retroid module directly
references `qcom_spmi_haptics_rumble`; modpost rejects it without that provider.
The product module check requires both symbols. Namespace, cgroup and seccomp
features needed by sandboxed Chromium were already present in the TTY trim.

The earlier thermal follow-up restored the board's `pwm-fan` consumer, Qualcomm
PM8150L LPG PWM provider, IIO/PMIC ADC thermal sensors, and PM8941-compatible
power-key driver. The fan and its provider are built in, so the earlier startup
PWM of 70/255 did not depend on module auto-loading. It added no fan curve or
automatic thermal shutdown. A product-only one-shot service logs a read-only
snapshot before the compositor, without timed sampling. Recovery stays on
`config-tty-trim`.

The next product-only change ports the **moderate kernel fan map** from
[ROCKNIX SM8250 PR #3339](https://github.com/ROCKNIX/distribution/pull/3339.patch):
`cluster1-thermal` active trips at 45, 55, 62, 68, 73 and 78°C with 5°C
hysteresis map to PWM 51, 77, 102, 128, 179 and 255. It changes the fan PWM
period to 50,000 ns and the built-in driver startup duty to 51/255. The
ROCKNIX startup patch still labels that duty as maximum cooling; this port
initializes the driver's cooling state from its actual PWM instead. Otherwise
a first request for maximum cooling could be ignored as a no-op. The map is
appended only to the product V2 DTS, not to the hardware-proven recovery DTB.
No ROCKNIX userspace profile service, thermal-trip writer, or unverified tach
GPIO is added. The patch author tested the Pocket 5, not this Mini V2. The fan
still has no exposed RPM on this image; neither PWM nor a build proves physical
rotation. See [the source comparison](../../../../docs/briefs/2026-09-24-rpminiv2-fan-practices.md).
The chosen map may cost fan noise and power; it does not fix unreadable PMIC
zones, CPU `performance` policy, or the snapshot script's failed-read output.
The owner chose no automatic power-off guard.

The product kernel also restores IPv6 and Netfilter from the ROCKNIX baseline,
with nftables compatibility, connection tracking, and reject support for
NixOS's firewall and the plugin host's IPv4/IPv6 port rules. The
first offline restore of all 20 signed plugins failed when `iptables-nft`
reported `Protocol not supported`; the old product config disabled both
facilities even though NixOS enabled the firewall and IPv6. The next boot
stopped at NixOS's LOG rule. Its next packet-type rule also needs the
packet-type matcher, so both are enabled in the product kernel. Recovery stays
unchanged. The
added networking code increases kernel attack surface; a build
alone does not prove that Sunshine's service will run on the Mini V2.

The product kernel also restores sound from the ROCKNIX baseline: the SM8250
ASoC card with the WCD9385 headphone codec, the two WSA881x speaker amplifiers,
the LPASS macros, Qualcomm SoundWire, DisplayPort audio, USB audio, and the
ADSP remoteproc that runs the audio DSP. The drivers are modules, so the ADSP
loads `qcom/sm8250/adsp.mbn` from the root file system; the config embeds no
firmware. With remoteproc on, the CDSP and SLPI that the device tree enables
also start. Recovery stays without sound. ROCKNIX's UCM (`../ucm`) sets the
speaker amplifier volume to 12 at boot. No one has heard this build yet.

No touchscreen, Wi-Fi, Bluetooth, media, UFS, or extra display driver is
restored. The following sizes and hashes describe the **earlier** product
build, before the thermal follow-up; a new build needs new artifact hashes:

- `Image`: 18,448,896 bytes, SHA-256
  `cce753a8d3e93d9b90aadb6c85c1d58aef6bc28b17e2c3c65acee81c8d3a38a8`
- Mini V2 DTB: the same hardware-proven
  `f9e32c33e14f3d974c461c674435a7002c73ec4243e96e3aef158620a730eee4`
- module closure: 476 KB containing `g_serial`, `retroid`, and the USB gadget
  dependencies

The thermal follow-up product kernel built on the development host with an
18,516,480-byte `Image`, SHA-256
`889fb9670054089d1d1dea8efad605f98c4bb1de2023c249b0b4af1a011e2c82`.
Its Mini V2 DTB remains SHA-256
`f9e32c33e14f3d974c461c674435a7002c73ec4243e96e3aef158620a730eee4`.
Neither hash proves a safe device boot or physical fan rotation.

The moderate-map product kernel built on the development host with `Image`
SHA-256 `5d92482eef176f65f3d5394d9e1205d10c66585f0578cde4a18a216f98fe5fee`
and Mini V2 DTB SHA-256
`6b2cb59d4be1ae7c3cbb543eed83cc96b1171194874df7e0ff40887f60a33eb8`.
The recovery DTB retains the hardware-proven hash above. These hashes pin
artifacts for image acceptance, not hardware fan operation or safe temperatures.

In that earlier build, the matching gadget and Retroid modules loaded from the root system. The companion
firmware remains available to the initrd and root system for aliases, service
manifests and module-time requests, including the three Adreno A650 blobs.

The companion `rocknix-baseline` package extracts the exact GRUB EFI binary and
font used by the accepted handoff. It retains checksum-pinned ROCKNIX GRUB and
DTB controls, but no longer extracts or installs the ROCKNIX `KERNEL`. It
includes no device-writing tooling and does not modify the installed Retroid
loader or internal storage.
