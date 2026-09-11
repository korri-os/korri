# Phase 1 only: remove large firmware families for absent PCIe GPUs/radios.
# The inspected RK3566 board uses RTL8821CS Wi-Fi and an RTL8156 USB NIC.
# Keep complete Realtek/Rockchip families, Intel Bluetooth, all other blobs,
# licenses, and the separate regulatory/ALSA/SOF firmware packages for now.
{ ... }:
{
  nixpkgs.overlays = [
    (_final: prev: {
      linux-firmware = prev.linux-firmware.overrideAttrs (old: {
        pname = "linux-firmware-rg353m";
        postInstall = (old.postInstall or "") + ''
          fw="$out/lib/firmware"
          for family in amdgpu radeon nvidia i915 xe; do
            test -d "$fw/$family"
            rm -r "$fw/$family"
          done
          find "$fw" -maxdepth 1 -name 'iwlwifi-*' -delete
        '';
        # Build inputs must not be retained via output symlinks/references.
        disallowedReferences = (old.disallowedReferences or [ ]) ++ [
          prev.linux-firmware
          old.src
        ];
      });
    })
  ];
}
