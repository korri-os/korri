{ pkgs }:
let
  retroarchReadOnlyPatch = ./patches/retroarch-udev-read-only.patch;
  retroarchUdevReadOnlyCheck =
    pkgs.buildPackages.callPackage ./patches/retroarch-udev-read-only-check.nix
      {
        retroarchSource = pkgs.retroarch-bare.src;
        readOnlyPatch = retroarchReadOnlyPatch;
      };
  retroarch = pkgs.retroarch-bare.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ retroarchReadOnlyPatch ];
    # Keep the actual packaged emulator behind the compiled source regression.
    postPatch = (old.postPatch or "") + ''
      test -e ${retroarchUdevReadOnlyCheck}
    '';
  });
  retroarchInputplumberAutoconfig = pkgs.callPackage ./retroarch-inputplumber-autoconfig.nix { };
in
{
  packages = {
    inherit retroarch;
    autoconfig = retroarchInputplumberAutoconfig;
  };
  files = {
    retroarch = "${retroarch}/bin/retroarch";
    autoconfig = "${retroarchInputplumberAutoconfig}/share/libretro/autoconfig";
  };
}
