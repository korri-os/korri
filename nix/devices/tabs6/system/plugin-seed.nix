# The image-plugin interface for a device that is not an SD image.
#
# nix/product/image-plugins.nix seeds the product's plugins by setting two
# options of NixOS's SD image module: sdImage.storePaths (closures the image
# carries beside the system) and sdImage.populateRootCommands (shell that
# writes receipts and GC roots under ./files). The Tab S6 has no SD image: its
# root is an ext4 image for `userdata`, and running systems update over SSH.
# This module declares just those two options, with the SD module's types and
# meaning, so image-plugins.nix applies unchanged. Nothing else of the SD
# module exists here.
#
# The commands run once, on an aarch64 builder (they call the aarch64 plugin
# host), into system.build.tabs6PluginSeed. The rootfs image copies that tree
# and its store paths in (../images), and deploy-over-ssh installs it on a
# device that has no plugin receipts yet (README.md, "First product deploy").
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.sdImage;
in
{
  options.sdImage = {
    storePaths = lib.mkOption {
      type = lib.types.listOf lib.types.package;
      default = [ ];
      description = "Store paths the root image carries beside the system closure, as in NixOS's SD image module.";
    };
    populateRootCommands = lib.mkOption {
      type = lib.types.lines;
      default = "";
      description = "Shell commands that populate ./files, copied to / of the root image, as in NixOS's SD image module.";
    };
  };

  config = {
    # The SD module lists the system itself first; image-plugins-check reads
    # the plugin selection as everything after it.
    sdImage.storePaths = lib.mkBefore [ config.system.build.toplevel ];

    # A tar keeps the modes the seed sets (0700 state, 0600 receipts); the
    # store would make them read-only. Its symlinks keep the plugin closures.
    system.build.tabs6PluginSeed = pkgs.runCommand "tabs6-plugin-seed" { } ''
      mkdir files
      ${cfg.populateRootCommands}
      mkdir -p $out
      tar -C files --owner=0 --group=0 --numeric-owner --sort=name -cf $out/seed.tar .
      tar -tvf $out/seed.tar
    '';
  };
}
