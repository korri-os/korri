# RG DS uses the same RTL8821CS and Rockchip firmware families as RG353M.
# Keep its USB Realtek accessory coverage too. Reuse the exact package (including
# its historical linux-firmware-rg353m name and notices), not a second allowlist.
# Board evidence and the limits of accessory coverage are in FIRMWARE.md.
import ../rg353m/firmware.nix
