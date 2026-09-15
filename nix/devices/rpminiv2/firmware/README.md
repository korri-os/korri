# Retroid Pocket Mini V2 firmware

The selection is grounded in ROCKNIX distribution
`e81d1fc943458fb13cffe1646761e9452b29ddc1`:

- `projects/ROCKNIX/devices/SM8250/config/kernel-firmware.dat`
- `projects/ROCKNIX/devices/SM8250/linux/dts/qcom/sm8250-retroidpocket-common.dtsi`
- `projects/ROCKNIX/packages/linux/package.mk`, especially `pre_make_target`
- `packages/linux-firmware/kernel-firmware/package.mk`
- `projects/ROCKNIX/packages/linux-firmware/extra-firmware/package.mk`

## Source and licenses

The producer pins linux-firmware **20260622**, archive SHA-256
`2b9d8a358e76eb766588609135e53fa548b902c551daae33ee32f26f25e60dbb`.
The derivation fetches that exact archive, not nixpkgs' older firmware release.
Only the board files and license documents enter the output. The archive is
about 610 MiB; selecting files reduces the installed image, not that download.

The producer also pins ROCKNIX extra-firmware
`30c56e2f34af37fe372166b739d6ab277f5155b5`, archive SHA-256
`b6e422b953fec72666a84c0060f1ac32dd3fce452a6d6311e5051ab923497600`.
Its recursive Git tree has only three SM8250 files: Thor ADSP, Thor speaker
calibration and Mangmi MCU firmware. None is named by the Mini V2 DT chain.
**No extra-firmware artifact is fetched or installed for this board.** Copying
that whole SoC directory would add firmware for unrelated hardware.

linux-firmware's `WHENCE`, `LICENSE` and `LICENSES/` are retained under
`share/doc/rpminiv2-firmware`; the QCA6390 notice remains beside its binaries.
The blobs are redistributable proprietary firmware under their individual
source licenses, not GPL merely because the ROCKNIX recipe is GPL. Nixpkgs'
locked `wireless-regdb` supplies the signed regulatory database.

## Paths and companions

| Consumer | Installed source |
| --- | --- |
| Adreno 650 | `qcom/a650_gmu.bin`, `qcom/a650_sqe.fw`, `qcom/sm8250/a650_zap.mbn` |
| ADSP/CDSP | SM8250 `.mbn` images and their three `.jsn` service manifests |
| SLPI | RB5 image and two service manifests; relative links at `qcom/sm8250/slpi*` match the common DTS and ROCKNIX's copy step |
| Venus | `qcom/vpu/vpu20_p4.mbn`; the two `vpu-1.0/venus` links come from `WHENCE` |
| QCA6390 WiFi | `amss.bin`, `m3.bin`, `board-2.bin` and notice |
| QCA6390 Bluetooth | `qca/htbtfw20.tlv`, `qca/htnv20.bin` |
| USB Ethernet | `rtl_nic/rtl8153a-4.fw`, from the producer's board firmware list |
| cfg80211 | `regulatory.db`, `regulatory.db.p7s` |

The archive carries complete monolithic DSP images, not split MDT segments.
The selected JSON manifests are real archive files. Venus's aliases are
WHENCE-generated links, not additional downloads. Every installed symlink has
its target inside this output and is checked during the build.

`passthru.firmwarePaths` lists the blobs, service manifests, aliases and signed
regulatory database for NixOS initrd inclusion. The built-in MSM/remoteproc
and cfg80211 drivers need early firmware access. Files stay uncompressed because
the native SM8250 config disables compressed firmware loading. Installing this
package on the root filesystem alone does not establish early-probe behavior.
