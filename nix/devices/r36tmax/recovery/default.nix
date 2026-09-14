# Build these helpers with native build packages, not the aarch64 target set.
{
  pkgs,
  rocknixArchive ? null,
}:
let
  archive =
    if rocknixArchive != null then
      rocknixArchive
    else
      pkgs.fetchurl {
        url = "https://github.com/ROCKNIX/distribution/releases/download/20260901/ROCKNIX-RK3326.aarch64-20260901-a.img.gz";
        sha256 = "e3272fc4266363332b830612db1abe4e20bb6a17c58d3c9f32815997556c768e";
      };
  tools = pkgs.writeShellApplication {
    name = "r36tmax-recovery";
    runtimeInputs = [
      pkgs.python3
      pkgs.mtools
      pkgs.ubootTools
      pkgs.e2fsprogs
    ];
    text = ''
      export PYTHONDONTWRITEBYTECODE=1
      exec python3 ${./boot.py} "$@"
    '';
  };
  checks =
    pkgs.runCommand "r36tmax-recovery-contract-tests"
      {
        nativeBuildInputs = [
          pkgs.python3
          pkgs.mtools
          pkgs.dosfstools
          pkgs.e2fsprogs
        ];
      }
      ''
        cp ${./boot.py} boot.py
        cp ${./test_boot.py} test_boot.py
        export PYTHONDONTWRITEBYTECODE=1
        python3 -m unittest -v test_boot
        touch "$out"
      '';
  loader =
    pkgs.runCommand "r36tmax-rocknix-20260901-a-loader"
      {
        nativeBuildInputs = [ tools ];
        # The source contains proprietary Rockchip firmware. See README.md before
        # publishing this output; extraction is not redistribution clearance.
        preferLocalBuild = true;
        allowSubstitutes = false;
      }
      ''
        export SOURCE_DATE_EPOCH=1
        r36tmax-recovery extract ${archive} "$out"
      '';
in
{
  inherit tools checks loader;
  # Use exactly this system for populateRootCommands and module closure too.
  bootFiles =
    { system }:
    pkgs.runCommand "r36tmax-recovery-boot-files"
      {
        nativeBuildInputs = [ tools ];
      }
      ''
        r36tmax-recovery populate ${system} ${loader} "$out"
      '';
}
