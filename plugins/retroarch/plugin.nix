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
  retroarchSettings = import ./settings-check.nix {
    inherit pkgs;
    program = retroarch;
  };
  retroarchInputplumberAutoconfig = pkgs.callPackage ./retroarch-inputplumber-autoconfig.nix { };
in
{
  packages = {
    inherit retroarch;
    autoconfig = retroarchInputplumberAutoconfig;
    retroarch-settings = retroarchSettings;
  };
  files = {
    retroarch = "${retroarch}/bin/retroarch";
    retroarch-settings = "${retroarchSettings}/settings.json";
    autoconfig = "${retroarchInputplumberAutoconfig}/share/libretro/autoconfig";
  };
}
