# SD image for the AYN Thor.
#
# The boot chain is the Portal's, so this module imports the Portal's
# sd-image.nix unchanged and forces only what differs:
#
#   PBL -> XBL -> ABL (BOOT MODE = Loader) -> loader_a (AYN U-Boot) ->
#   bootefi bootmgr -> EFI/BOOT/BOOTAA64.EFI on this card -> systemd-boot ->
#   kernel + initrd + `devicetree` qcs8550-ayn-thor.dtb
#
# From the Portal module, unchanged: the ESP layout systemd-boot-builder.py
# writes (populateFirmwareCommands reads the DTB from hardware.deviceTree, so
# the entry names the Thor DTB), the MBR-to-GPT conversion with 1 MiB of
# slack, systemd-boot with graceful = true and canTouchEfiVariables = false,
# the gzip initrd with includeDefaultModules = false, the pre-root Adreno 740
# firmware, firmwareCompression = "none", and the SM8550 platform policy
# (kernel command line, governors with cpu0 state1 held off, fan curve,
# ft5x06 hwdb, no sleep states).
#
# Labels: THOR_ESP / THOR_ROOT. They differ from the Portal's internal
# ODIN2P_ESP / ODIN2P_ROOT and from ROCKNIX's STORAGE, so U-Boot, systemd-boot
# and the initrd cannot pick an internal partition by label. The root is
# selected by THOR_ROOT (sd-card.nix mounts / by rootVolumeLabel) and /boot
# by THOR_ESP.
{
  lib,
  thorDtb,
  ...
}:

let
  esp = "THOR_ESP";
  root = "THOR_ROOT";
in
{
  imports = [ ../odin2portal/sd-image.nix ];

  # The Portal's Linux 7.0.2 rescue entry is Portal history: that kernel was
  # its last known good build before 7.2. The Thor has no such history.
  specialisation = lib.mkForce { };

  # The Thor runs the Portal kernel with its own DTB, built from the same
  # patched source (./kernel).
  hardware.deviceTree = {
    dtbSource = "${thorDtb}/dtbs";
    name = lib.mkForce "qcom/${thorDtb.dtbName}.dtb";
  };

  image.baseName = lib.mkForce "nixos-thor";

  sdImage = {
    firmwarePartitionName = lib.mkForce esp;
    rootVolumeLabel = lib.mkForce root;
  };

  fileSystems."/boot".device = lib.mkForce "/dev/disk/by-label/${esp}";

  networking.hostName = lib.mkForce "thor";
}
