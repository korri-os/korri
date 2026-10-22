# One firmware tree for image assembly and internal-UFS updates. Keep the
# hardware-proven filenames, GRUB/GOP handoff and inactive BLS metadata together.
{
  config,
  lib,
  pkgs,
  rpminiRocknixBaseline,
}:
let
  activeKernel = config.boot.kernelPackages.kernel;
  toplevel = config.system.build.toplevel;
  kernel = "${activeKernel}/${config.system.boot.loader.kernelFile}";
  initrd = "${config.system.build.initialRamdisk}/${config.system.boot.loader.initrdFile}";
  dtb = "${config.hardware.deviceTree.package}/${config.hardware.deviceTree.name}";
  kernelName = "${baseNameOf activeKernel}-${config.system.boot.loader.kernelFile}";
  initrdName = "${baseNameOf config.system.build.initialRamdisk}-${config.system.boot.loader.initrdFile}";
  # deviceTree.package ends in /dtbs, not at the owning store path.
  dtbName = "${baseNameOf activeKernel}-${baseNameOf config.hardware.deviceTree.name}";
  params = lib.concatStringsSep " " ([ "init=${toplevel}/init" ] ++ config.boot.kernelParams);
  loaderEntryTitle =
    if config.image.baseName == "nixos-rpminiv2-korri" then
      "NixOS Retroid Pocket Mini V2 Korri"
    else
      "NixOS Retroid Pocket Mini V2 candidate";
  grubEntryTitle =
    if config.image.baseName == "nixos-rpminiv2-korri" then
      "NixOS Retroid Pocket Mini V2 Korri"
    else
      "NixOS Retroid Pocket Mini V2";
  loaderConf = pkgs.writeText "loader.conf" ''
    timeout 0
    default nixos-generation-1.conf
    console-mode keep
  '';
  entry = pkgs.writeText "nixos-generation-1.conf" ''
    title ${loaderEntryTitle}
    version Generation 1 ${config.system.nixos.label}
    linux /EFI/nixos/${kernelName}
    initrd /EFI/nixos/${initrdName}
    options ${params}
    devicetree /EFI/nixos/${dtbName}
  '';
  grubCfg = pkgs.writeText "grub.cfg" ''
    insmod part_gpt
    insmod part_msdos
    set timeout=2
    set default=0
    set timeout_style=menu
    set lang=en_US
    loadfont /boot/grub/dejavu-mono.pf2
    set rotation=270
    set gfxmode=auto
    insmod efi_gop
    insmod gfxterm
    terminal_output gfxterm
    set menu_color_normal=cyan/blue
    set menu_color_highlight=white/blue

    menuentry '${grubEntryTitle}' {
            search --set -f /EFI/nixos/${kernelName}
            linux /EFI/nixos/${kernelName} ${params}
            initrd /EFI/nixos/${initrdName}
            devicetree /EFI/nixos/${dtbName}
    }
  '';
in
pkgs.runCommand "rpminiv2-firmware" { } ''
  mkdir -p \
    $out/EFI/BOOT \
    $out/EFI/systemd \
    $out/EFI/nixos \
    $out/boot/grub \
    $out/loader/entries
  # This is the exact GRUB/GOP handoff that remained visible and booted
  # the accepted NixOS TTY. Keep systemd-boot only as an inactive backup.
  cp ${rpminiRocknixBaseline}/bootaa64.efi $out/EFI/BOOT/BOOTAA64.EFI
  cp ${pkgs.systemd}/lib/systemd/boot/efi/systemd-bootaa64.efi $out/EFI/systemd/systemd-bootaa64.efi
  cp ${rpminiRocknixBaseline}/dejavu-mono.pf2 $out/boot/grub/dejavu-mono.pf2
  cp ${grubCfg} $out/boot/grub/grub.cfg
  cp ${kernel} $out/EFI/nixos/${kernelName}
  cp ${initrd} $out/EFI/nixos/${initrdName}
  cp ${dtb} $out/EFI/nixos/${dtbName}
  # Retain BLS metadata for offline inspection and rollback tooling. GRUB
  # is the active removable-media loader.
  cp ${loaderConf} $out/loader/loader.conf
  cp ${entry} $out/loader/entries/nixos-generation-1.conf
''
