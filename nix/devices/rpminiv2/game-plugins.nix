# Opt-in for the existing Linux game plugins on the RP Mini V2.
#
# The plugin host restores administrator-approved selections from its receipt
# directory. The image carries those receipts and the plugin packages, and this
# module binds the one publisher the device trusts. Nothing here installs or
# enables a plugin by name: the receipts are the approval record, and the host
# re-verifies each one against the bound key before it starts anything.
{
  config,
  lib,
  ...
}:
let
  root = config.services.korridLinuxDevice.storageRoot;
  user = config.services.korriLinuxHost.runtimeUser;
  group = config.services.korriLinuxHost.runtimeGroup;
  account = "${root}/users/default";
  # One directory per library slug the SD card holds. The runtime user gets
  # read and traverse on each one, and inheritance for new content.
  systems = [
    "nec-pc98"
    "nec-turbografx-16"
    "nintendo-64"
    "nintendo-entertainment-system"
    "nintendo-gameboy-advance"
    "nintendo-gameboy-color"
    "nintendo-super-nintendo-entertainment-system"
    "sega-genesis"
    "sega-master-system"
    "sinclair-zx-spectrum"
    "sony-playstation"
    "sony-playstation-portable"
    "zelda-classic"
  ];
in
{
  # Catalog documents and private daemon state stay private. The launched game
  # runs as the existing untrusted runtime identity.
  systemd.tmpfiles.rules = [
    "a+ ${root} - - - - u:${user}:--x,d:u:${user}:---"
    # Documents that predate the default deny ACL, and the catalog directory,
    # are denied by name. Inheritance alone would depend on creation order.
    "a+ ${root}/*.yaml - - - - u:${user}:---"
    "a+ ${root}/catalog - - - - u:${user}:---,d:u:${user}:---"
    "a+ ${root}/catalog/*.yaml - - - - u:${user}:---"
    # The host's d/0700 rule otherwise masks named ACLs on every activation.
    # z runs after directory creation; group:: stays empty, other:: stays empty.
    "z ${root} 0710 korrid korrid -"
    "a+ ${root}/roms - - - - u:${user}:--x"
    "d ${root}/users 2710 korrid ${group} -"
    "a+ ${root}/users - - - - u:${user}:--x"
    "d ${account} 2770 korrid ${group} -"
    "a+ ${account} - - - - u:${user}:rwx,d:u:${user}:rwx,d:g:${group}:rwx"
  ]
  ++ lib.concatMap (name: [
    "a+ ${root}/roms/${name} - - - - u:${user}:r-x,d:u:${user}:r-x"
  ]) systems
  ++
    lib.concatMap
      (name: [
        "d ${account}/${name} 2770 korrid ${group} -"
        "a+ ${account}/${name} - - - - u:${user}:rwx,d:u:${user}:rwx,d:g:${group}:rwx"
      ])
      [
        "system"
        "saves"
        "states"
        "screenshots"
      ];
}
