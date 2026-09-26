# Evaluate the real merged SD module, including upstream native initialization.
# Keep these composition checks alongside the disposable-disk VM regression.
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
  profile = "/bin/nix-env -p /nix/var/nix/profiles/system --set /run/current-system";
  remove = "rm -f /nix-path-registration";
  occurrences = needle: text: builtins.length (lib.splitString needle text) - 1;
  before =
    first: second: text:
    lib.hasInfix second (builtins.elemAt (lib.splitString first text) 1);
  nativeInitialization =
    config:
    let
      script = config.boot.postBootCommands;
    in
    occurrences expand script == 1
    && occurrences register script == 1
    && occurrences "touch /etc/NIXOS" script == 1
    && occurrences profile script == 1
    && occurrences remove script == 1
    && lib.hasInfix ''/sys/class/block/'' script
    && lib.hasInfix ''/partition")"'' script
    && !(lib.hasInfix "MAJ:MIN" script)
    && before expand register script
    && before register "touch /etc/NIXOS" script
    && before "touch /etc/NIXOS" profile script
    && before profile remove script;
in
assert !mbr.sdImage.expandOnBoot && !gpt.sdImage.expandOnBoot;
assert lib.hasInfix "mkdir -p ./files/boot" mbr.sdImage.populateRootCommands;
assert lib.hasInfix "mkdir -p ./files/boot" gpt.sdImage.populateRootCommands;
assert !(lib.hasInfix repair mbr.boot.postBootCommands);
assert lib.hasInfix repair gpt.boot.postBootCommands;
assert lib.all nativeInitialization [
  mbr
  gpt
];
# The only difference between the existing first-boot paths is GPT repair.
assert
  builtins.replaceStrings [ "${repair}\n  " ] [ "" ] gpt.boot.postBootCommands
  == mbr.boot.postBootCommands;
pkgs.runCommand "korri-sd-card-module-check"
  {
    mbrScript = pkgs.writeText "mbr-post-boot.sh" mbr.boot.postBootCommands;
    gptScript = pkgs.writeText "gpt-post-boot.sh" gpt.boot.postBootCommands;
  }
  ''
    mkdir -p "$out"
    cp "$mbrScript" "$out/mbr-post-boot.sh"
    cp "$gptScript" "$out/gpt-post-boot.sh"
  ''
