# Bring up the RG35XXSP display kernel

Status: ready-for-agent
Blocked by: None

## What to build

Make the RG35XXSP's panel work with the approved ROCKNIX-derived Linux 7.2 starting point. Produce hardware evidence for the later product migration without claiming that the whole device is supported.

## Acceptance criteria

- [ ] Work continues from the existing `.worktree/rg35xxsp-display` staged changes; the work is not reset, recreated, or replaced with the mainline 6.12 kernel.
- [ ] The device uses the approved ROCKNIX-derived Linux 7.2 kernel and retains Panfrost, mainline U-Boot v2025.10 with `anbernic_rg35xx_h700_defconfig`, AXP717, and RTL8821CS support.
- [ ] The kernel and image are built off-device. No build runs on the RG35XXSP.
- [ ] A permitted first-boot path produces stable visible output on the built-in 640x480 panel, and the display driver binds without a fatal kernel error.
- [ ] The result records the observed DRM device, connector, mode, render device, renderer, rotation, and input facts needed by ticket 18. Unknown values are not guessed.
- [ ] The shared graphics/seat check is satisfied by correct integration rather than a new exclusion.
- [ ] No bootloader, firmware, or forbidden internal-storage partition is written. Any device operation follows the repository's separate approval rules.
- [ ] The ticket records the exact hardware validation performed and labels all untested hardware behavior as unverified.
- [ ] Buttons, Wi-Fi, audio, product-module integration, delivery, and supported-image qualification remain ticket 18 work.
