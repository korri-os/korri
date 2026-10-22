# Tiny real firmware trees from the production renderer. No device, board
# kernel, system closure or network is needed by the update regression tests.
{ pkgs }:
let
  inherit (pkgs) lib;
  file =
    name: relative:
    pkgs.runCommand name { } ''
      mkdir -p "$out/$(dirname ${lib.escapeShellArg relative})"
      printf '%s\n' ${lib.escapeShellArg name} > "$out/${relative}"
    '';
  kernel = file "linux-test" "Image";
  initrd = file "initrd-test" "initrd";
  dtbs = file "dtbs-test" "qcom/board.dtb";
  baseline = pkgs.runCommand "grub-assets-test" { } ''
    mkdir "$out"
    echo loader > "$out/bootaa64.efi"
    echo font > "$out/dejavu-mono.pf2"
  '';
  render =
    version:
    import ../esp.nix {
      inherit lib;
      pkgs = pkgs // {
        systemd = file "systemd-boot-test" "lib/systemd/boot/efi/systemd-bootaa64.efi";
      };
      rpminiRocknixBaseline = baseline;
      config = {
        boot.kernelPackages.kernel = kernel;
        boot.kernelParams = [ "quiet" ];
        system.build.toplevel = file "nixos-system-rpminiv2-sd-card-${version}" "init";
        system.build.initialRamdisk = initrd;
        system.boot.loader.kernelFile = "Image";
        system.boot.loader.initrdFile = "initrd";
        system.nixos.label = version;
        hardware.deviceTree.package = dtbs;
        hardware.deviceTree.name = "qcom/board.dtb";
        image.baseName = "nixos-rpminiv2-korri";
      };
    };
in
pkgs.runCommand "rpminiv2-update-firmware-fixtures" { } ''
  mkdir "$out"
  ln -s ${render "old"} "$out/old"
  ln -s ${render "new"} "$out/new"
''
