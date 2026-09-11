# Phase 2 targets the inspected RTL8821CS RG353M and its RTL8156 USB NIC.
# Retain complete driver families, not individual chip files. The separate
# rtl8761b package overrides two rtl_bt files in the working baseline, so keep
# it too. The kernel, UCM audio profiles and regulatory database are unchanged.
{ pkgs, lib, ... }:
{
  # all-hardware.nix sets this to true for generic installers.
  hardware.enableRedistributableFirmware = lib.mkForce false;
  hardware.wirelessRegulatoryDatabase = true;
  hardware.firmware = [
    pkgs.linux-firmware
    pkgs.rtl8761b-firmware
  ];
  nixpkgs.overlays = [
    (_final: prev: {
      linux-firmware = prev.linux-firmware.overrideAttrs (old: {
        pname = "linux-firmware-rg353m";
        postInstall = (old.postInstall or "") + ''
          fw="$out/lib/firmware"
          for family in rtw88 rtl_bt rtl_nic rockchip; do
            test -d "$fw/$family"
          done
          find "$fw" -mindepth 1 -maxdepth 1 \
            ! -name rtw88 ! -name rtl_bt ! -name rtl_nic ! -name rockchip \
            -exec rm -rf -- {} +
          # Upstream's installed firmware tree omits notices. Carry the pinned
          # source's WHENCE and license texts alongside the selected blobs.
          notices="$out/share/licenses/linux-firmware-rg353m"
          mkdir -p "$notices"
          find . -maxdepth 1 -type f \
            \( -name 'LICEN*' -o -name 'COPYING*' -o -name WHENCE \) \
            -exec cp -- {} "$notices/" \;
          test -f "$notices/WHENCE"
          test -f "$notices/LICENCE.rtlwifi_firmware.txt"
          test -f "$notices/LICENCE.rockchip"
        '';
        disallowedReferences = (old.disallowedReferences or [ ]) ++ [
          prev.linux-firmware
          old.src
        ];
      });
    })
  ];
}
