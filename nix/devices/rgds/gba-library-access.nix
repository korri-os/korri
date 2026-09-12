# Access for the operator-imported GBA directory and the existing RetroArch
# users/default producer. Catalog documents and private daemon state stay private.
{ config, lib, ... }:
let
  root = config.services.korridLinuxDevice.storageRoot;
  # The launched game runs as the existing untrusted runtime identity.
  user = config.services.korriLinuxHost.runtimeUser;
  group = config.services.korriLinuxHost.runtimeGroup;
  account = "${root}/users/default";
in
{
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
    "a+ ${root}/roms/nintendo-gameboy-advance - - - - u:${user}:r-x,d:u:${user}:r-x"
    "d ${root}/users 2710 korrid ${group} -"
    "a+ ${root}/users - - - - u:${user}:--x"
    "d ${account} 2770 korrid ${group} -"
    "a+ ${account} - - - - u:${user}:rwx,d:u:${user}:rwx,d:g:${group}:rwx"
  ]
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
