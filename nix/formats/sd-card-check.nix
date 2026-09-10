# Use the real NixOS SD module. The root-population fragments must compose
# without a second option vocabulary, and MBR must not receive GPT repair.
{ pkgs, nixpkgs }:
let
  inherit (pkgs) lib;
  evaluate =
    gpt:
    (nixpkgs.lib.nixosSystem {
      system = pkgs.stdenv.hostPlatform.system;
      modules = [
        (import ./sd-card.nix { inherit gpt; })
        {
          system.stateVersion = "25.11";
          sdImage.populateRootCommands = lib.mkAfter ''
            mkdir -p ./files/boot
          '';
        }
      ];
    }).config;
  mbr = evaluate false;
  gpt = evaluate true;
  # hasInfix uses a regex and needs context-free strings for these comparisons.
  repair = builtins.unsafeDiscardStringContext ''${pkgs.gptfdisk}/bin/sgdisk -e "$bootDevice" || true'';
  expand = ''/bin/sfdisk -N"$partNum" --no-reread --force "$bootDevice" || true'';
  register = "/bin/nix-store --load-db < /nix-path-registration";
in
assert !mbr.sdImage.expandOnBoot && !gpt.sdImage.expandOnBoot;
assert lib.hasInfix "mkdir -p ./files/boot" mbr.sdImage.populateRootCommands;
assert lib.hasInfix "mkdir -p ./files/boot" gpt.sdImage.populateRootCommands;
assert !(lib.hasInfix repair mbr.boot.postBootCommands);
assert lib.hasInfix repair gpt.boot.postBootCommands;
assert lib.all (config: lib.hasInfix expand config.boot.postBootCommands) [
  mbr
  gpt
];
assert lib.all (config: lib.hasInfix register config.boot.postBootCommands) [
  mbr
  gpt
];
# The only difference between the existing first-boot paths is GPT repair.
assert
  builtins.replaceStrings [ "${repair}\n  " ] [ "" ] gpt.boot.postBootCommands
  == mbr.boot.postBootCommands;
pkgs.runCommand "korri-sd-card-module-check" { } ''
  touch "$out"
''
