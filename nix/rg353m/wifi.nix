# RG353M radio settings for the shared workshop WiFi profile (../wifi.nix).
#
# The RG353M uses an RTL8821CS on SDIO. The first native boots logged SDIO
# timeouts from rtw88_8821cs, so this path is best effort until the SDIO
# clock is tuned. The USB gadget is the reliable path.
{ ... }:

{
  imports = [ ../wifi.nix ];

  # RTL8821CS deep power saving caused pairing failures and dropped
  # connections during the Sunshine feasibility run.
  networking.networkmanager.ensureProfiles.profiles.korri.wifi.powersave = 2;
}
