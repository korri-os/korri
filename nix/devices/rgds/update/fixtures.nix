# The actual pinned NixOS generator, tiny store paths, and an isolated native
# profile directory. This does not build an ARM system or contact a handheld.
{ pkgs }:
let
  inherit (pkgs) lib;
  builder = import ./renderer.nix { inherit pkgs; };
  file =
    name: relative:
    pkgs.runCommand name { } ''
      mkdir -p "$out/$(dirname ${lib.escapeShellArg relative})"
      echo ${lib.escapeShellArg name} > "$out/${relative}"
    '';
  kernel = version: file "linux-test-${version}" "Image";
  initrd = version: file "initrd-test-${version}" "initrd";
  dtbs = version: file "dtbs-test-${version}" "rockchip/rk3568-anbernic-rg-ds.dtb";
  system =
    version:
    pkgs.runCommand "nixos-system-rgds-sd-card-${version}" { } ''
      mkdir -p "$out/specialisation"
      echo init > "$out/init"
      chmod +x "$out/init"
      ln -s ${kernel version}/Image "$out/kernel"
      ln -s ${initrd version}/initrd "$out/initrd"
      ln -s ${dtbs version} "$out/dtbs"
      echo ${version} > "$out/nixos-version"
      echo 'quiet console=ttyS2,1500000n8' > "$out/kernel-params"
    '';
  old = system "old";
  new = system "new";
in
pkgs.runCommand "rgds-update-boot-fixtures" { } ''
  mkdir -p "$out" profiles
  ln -s ${old} "$out/old-system"
  ln -s ${new} "$out/new-system"
  ln -s ${old} profiles/system-1-link
  cp ${builder} builder.sh
  chmod +w builder.sh
  substituteInPlace builder.sh --replace-fail /nix/var/nix/profiles "$PWD/profiles"
  ${pkgs.bash}/bin/bash builder.sh -g 0 -t 3 -n rockchip/rk3568-anbernic-rg-ds.dtb -c ${old} -d "$out/old"
  ${pkgs.bash}/bin/bash builder.sh -g 1 -t 3 -n rockchip/rk3568-anbernic-rg-ds.dtb -c ${new} -d "$out/new"
  ln -s ${builder} "$out/renderer"
''
