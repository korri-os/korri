# Rescue media must not resolve either eMMC filesystem by its normal label.
{
  pkgs,
  configuration,
  rescueConfiguration,
}:
let
  normal = configuration.config;
  rescue = rescueConfiguration.config;
  rawWrite = builtins.unsafeDiscardStringContext rescue.sdImage.postBuildCommands;
  plugins =
    c:
    pkgs.lib.subtractLists [ (toString c.system.build.toplevel) ] (map toString c.sdImage.storePaths);
in
assert rescue.image.baseName == "nixos-rgds-rescue";
assert rescue.sdImage.rootVolumeLabel == "KORRI_RESCUE";
assert rescue.sdImage.firmwarePartitionName == "KORRI_RBOOT";
assert rescue.fileSystems."/".device == "/dev/disk/by-label/KORRI_RESCUE";
assert rescue.fileSystems."/boot/firmware".device == "/dev/disk/by-label/KORRI_RBOOT";
assert rescue.fileSystems."/".device != normal.fileSystems."/".device;
assert rescue.fileSystems."/boot/firmware".device != normal.fileSystems."/boot/firmware".device;
assert rescue.boot.kernelPackages.kernel == normal.boot.kernelPackages.kernel;
assert rescue.hardware.deviceTree.name == normal.hardware.deviceTree.name;
assert plugins rescue == plugins normal;
assert pkgs.lib.hasInfix "--root-label KORRI_RESCUE" rawWrite;
assert pkgs.lib.hasInfix "--firmware-label KORRI_RBOOT" rawWrite;
pkgs.runCommand "rgds-rescue-check" { } ''
  touch "$out"
''
