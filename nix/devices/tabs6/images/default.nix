# Flashable images and the over-SSH updater, all built on the x86_64 host.
#
#   rootfs  ext4 for `userdata`, as an Android sparse image (img2simg)
#   esp     FAT32 for `cache` (400 MiB, 512 byte logical sectors, as the port)
#   deploy  deploy-over-ssh: update a running tablet without flashing
#
# The tabs6 repository's esp-installer image (the ESP plus its installer
# initramfs, for the first flash) stays there with the installer and the
# flash scripts: flashing is not a Korri path.
#
# pluginStorePaths and tabs6PluginSeed carry the product's seeded plugins
# (../system/plugin-seed.nix); on the console they are the system alone and
# an empty seed. `deployTarget` sets the deploy tool's target and signing.
{
  nixpkgs,
  pkgsHost,
  toplevel,
  firmware,
  placeholderKernel,
  tabs6PluginSeed,
  pluginStorePaths,
  deployTarget,
}:
let
  inherit (pkgsHost) lib;
  tabs6-esp = pkgsHost.callPackage ../esp/package.nix { };
  # Marks images that cannot boot the tablet in their file name.
  suffix = lib.optionalString placeholderKernel "-PLACEHOLDER-KERNEL";

  ext4-cat = pkgsHost.writeShellApplication {
    name = "ext4-cat";
    runtimeInputs = [
      pkgsHost.e2fsprogs
      pkgsHost.gnugrep
      pkgsHost.gnused
    ];
    text = builtins.readFile ./ext4-cat.sh;
  };

  ext4 = pkgsHost.callPackage "${nixpkgs}/nixos/lib/make-ext4-fs.nix" {
    storePaths = pluginStorePaths ++ [ tabs6PluginSeed ];
    volumeLabel = "nixos";
    populateImageCommands = ''
      mkdir -p ./files
      # Plugin receipts and their GC roots, with the seed's own modes.
      tar -C ./files -xpf ${tabs6PluginSeed}/seed.tar
      mkdir -p ./files/lib ./files/etc ./files/nix/var/nix/gcroots
      # Device firmware, outside the closure (modules/device.nix).
      cp -a ${firmware}/lib/firmware ./files/lib/firmware
      # What activation would create, so the image is self-describing before
      # its first boot. Activation replaces both.
      ln -s ${toplevel}/etc ./files/etc/static
      ln -s /etc/static/os-release ./files/etc/os-release
      ln -s ${toplevel} ./files/nix/var/nix/gcroots/tabs6-current
    '';
  };
in
rec {
  inherit ext4-cat;
  rootfs =
    pkgsHost.runCommand "tabs6-rootfs${suffix}"
      {
        nativeBuildInputs = [
          pkgsHost.android-tools
          ext4-cat
        ];
        preferLocalBuild = true;
        allowSubstitutes = false;
      }
      ''
        mkdir -p $out
        # The image's own /etc/os-release must say NixOS before it ships.
        ext4-cat ${ext4} /etc/os-release > os-release
        cat os-release
        grep -q '^ID=nixos$' os-release
        ext4-cat ${ext4} /lib/firmware/ath10k/WCN3990/hw1.0/board-2.bin | cmp - ${firmware}/lib/firmware/ath10k/WCN3990/hw1.0/board-2.bin
        img2simg ${ext4} $out/tabs6-rootfs${suffix}.simg
        cp os-release $out/os-release
        echo ${toplevel} > $out/toplevel
        sha256sum $out/tabs6-rootfs${suffix}.simg > $out/SHA256SUMS
      '';

  esp =
    pkgsHost.runCommand "tabs6-esp${suffix}"
      {
        nativeBuildInputs = [
          pkgsHost.dosfstools
          pkgsHost.mtools
          tabs6-esp
        ];
      }
      ''
        mkdir -p $out esp
        tabs6-esp stage ${toplevel} esp
        img=$out/tabs6-esp${suffix}.img
        # cache is 102400 x 4096 byte blocks (GTS6LWIFI_EUR_OPEN.pit) = 400 MiB.
        # FAT32 with 512 byte logical sectors, as the port's ESP; on the device it
        # mounts only through a 512 byte loop device.
        truncate -s 400M $img
        mkfs.vfat -F 32 -S 512 -n TABS6ESP -i 7AB60001 --invariant $img
        export MTOOLS_SKIP_CHECK=1
        mcopy -s -i $img esp/* ::/
        mdir -/ -i $img ::/ > $out/listing.txt
        cat $out/listing.txt
        echo ${toplevel} > $out/toplevel
        sha256sum $img > $out/SHA256SUMS
      '';

  deploy = pkgsHost.writeShellApplication {
    name = "deploy-over-ssh";
    # It names the proprietary firmware: build it here, never fetch or send it.
    derivationArgs = {
      preferLocalBuild = true;
      allowSubstitutes = false;
    };
    runtimeInputs = [
      pkgsHost.openssh
      pkgsHost.rsync
      pkgsHost.nix
      pkgsHost.gnutar
      pkgsHost.gnugrep
    ];
    text = ''
      toplevel=${toplevel}
      firmware=${firmware}
      placeholder=${lib.boolToString placeholderKernel}
      plugin_seed=${tabs6PluginSeed}
      sign_key="${if deployTarget.signKey == null then "" else deployTarget.signKey}"
      default_target=${deployTarget.target}
      default_port=${toString deployTarget.port}
      ${builtins.readFile ./deploy-over-ssh.sh}
    '';
  };
}
