# Driver and prior-proof options only. Public images contain no RK915 blobs.
# Linux's firmware loader can read owner-supplied files from /lib/firmware;
# that is its native search path, not a new Korri firmware format.
{ config, ... }:
{
  boot.extraModulePackages = [ (config.boot.kernelPackages.callPackage ./driver.nix { }) ];
  boot.extraModprobeConfig = ''
    options rk915 down_fw_in_probe=1 default_phy_threshold=180 lpw_no_sleep=1
  '';
  networking.networkmanager.unmanaged = [ "usb0" ];
}
