# RG DS firmware and Nixpkgs footprint

The RG DS imports RG353M's complete-family firmware policy without changing
RG353M. The shared package deliberately keeps the existing
`linux-firmware-rg353m` name and license directory. There is one allowlist and
one byte-level verifier, not a second board-specific copy of the same blobs.

## Grounding and coverage

Read-only evidence from the running RG DS on 2026-10-25 was saved at
`/tmp/rgds-cpu-evidence/rgds-firmware-modules.txt`. It reports Linux's
`rtw88_8821cs` driver loading firmware version 24.11.0, Goodix configuration
requests returning -2, Panfrost, and the RK817 codec. The board is RK3568.
No device access is needed to build or check this policy.

| Retained family/package | Reason and limits |
|---|---|
| `rtw88` | Complete Realtek Wi-Fi family; includes the observed RTL8821CS driver's `rtw8821c_fw.bin`. |
| `rtl_bt` | Complete Realtek Bluetooth family; the same RTL8821CS uses UART Bluetooth. Keep its firmware and configuration for the separate Bluetooth bring-up. This change does not enable Bluetooth. |
| `rtl_nic` | Preserve RG353M's Realtek USB Ethernet accessory coverage, including RTL8156. No RG DS adapter was observed; this is conservative accessory coverage, not a hardware verification claim. |
| `rockchip` | Preserve RG353M's Rockchip-family coverage on this RK3568 board. No firmware request from this family was observed; this is conservative SoC-family coverage. Panfrost on Mali-G52 does not need a blob. |
| `rtl8761b-firmware` | Preserve the existing effective `rtl_bt` overrides and USB Bluetooth accessory coverage. This is not the board's RTL8821CS firmware. |
| `wireless-regdb` | Keep the signed wireless regulatory database and signature unchanged. |

The other generic redistributable firmware bundles are removed. Arbitrary USB
Wi-Fi/Bluetooth adapters, other board variants, and future drivers that need
other firmware families are not covered. No missing Goodix configuration is
invented: `goodix_911_cfg.bin` was absent on the working device already, and
its touch controllers work without it. No kernel, audio/UCM, radio service,
recovery, USB console or storage policy changes belong to this cut.

The retained Linux firmware carries upstream WHENCE and license texts without
retaining the build source tree. The shared verifier compares every retained
file, internal symlink, license text, effective Realtek override, and signed
regulatory file against the full baseline. An untrimmed negative control must
fail. RG DS also checks the exact RTL8821C Wi-Fi and RTL8821CS Bluetooth files.

## Nixpkgs source delivery

Like RG353M, the native Nix registry uses the exact revision and NAR hash in
`flake.lock` instead of retaining the Nixpkgs source tree in the system closure.
The existing `nixpkgs=flake:nixpkgs` alias remains. An uncached source lookup
requires network access. Nix, the download-only device policy, and the pinned
source identity do not change. The system builder rejects accidental source
closure references. This does not remove old generations or run garbage collection.

## Build-host checks

Use build hosts, never the handheld. Supply the project's CI cache and signing
key to avoid rebuilding cached boot components:

```sh
nix build --no-link -L \
  --option extra-substituters https://korri-os.github.io/nix-cache/ \
  --option extra-trusted-public-keys korri-cache-ci-1:iH8gsPMtGrreeuXt2kt6M2ca+y4u/dO+T2km6AUuI8c= \
  .#checks.aarch64-linux.rgds-firmware \
  .#checks.aarch64-linux.rgds-registry \
  .#packages.aarch64-linux.rgds-sd-image
```

Image assembly runs `verify-image.py` against the real image and exact U-Boot
bytes before compression. This proves image structure, not hardware boot.
A later owner-approved boot must check Wi-Fi, Bluetooth, USB accessories,
displays, touch, audio, and recovery before hardware acceptance. Do not write
a card or access the device as part of these build-host checks.
