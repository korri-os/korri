# Game library access for the runtime user, on every product device. Moved
# unchanged from the RP Mini V2 (game-plugins.nix). Without it /var/lib/korri
# stays korrid-only (0700), and a launched game cannot read ROMs or write saves:
# the RG353M card showed exactly that on 2026-09-30.
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
    # Paired discovery sources (one folder per system or group, written by
    # `korrid catalog migrate` and `pair`). The game reads and traverses the
    # whole tree; new folders and files inherit that access.
    "d ${root}/games 0710 korrid korrid -"
    # The explicit mask matters: `korrid catalog migrate` and `pair` create
    # folders 0700, whose ACL mask is empty, and tmpfiles keeps that mask, so
    # the named entry alone stays ineffective (the RG353M Wario Land 4 folder,
    # 2026-10-20). The game user gets read and traverse, never write.
    "A+ ${root}/games - - - - u:${user}:rX,d:u:${user}:rX,m::rX,d:m::rX"
    "d ${root}/users 2710 korrid ${group} -"
    "a+ ${root}/users - - - - u:${user}:--x"
    "d ${account} 2770 korrid ${group} -"
    "a+ ${account} - - - - u:${user}:rwx,d:u:${user}:rwx,d:g:${group}:rwx"
    # Reconstructible artifacts belong to korrid (ADR 0012). The game user
    # can read and traverse ready artifacts, but cannot replace a contribution.
    # Upgrade from prepare-owned caches requires the ADR's one-off deletion.
    "d ${root}/cache 2755 korrid ${group} -"
    "a+ ${root}/cache - - - - u:${user}:r-x,d:u:${user}:r-x,d:g:${group}:r-x"
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
