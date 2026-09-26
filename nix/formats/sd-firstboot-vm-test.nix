# Run the evaluated SD postBootCommands unchanged, with real Nix and partition
# tools, inside chroots on disposable VM disks. This is not a board boot test.
{ pkgs, nixpkgs }:
let
  inherit (pkgs) lib;
  evaluate =
    gpt: extra:
    (nixpkgs.lib.nixosSystem {
      system = pkgs.stdenv.hostPlatform.system;
      modules = [
        (import ./sd-card.nix { inherit gpt; })
        { system.stateVersion = "25.11"; }
        extra
      ];
    }).config.boot.postBootCommands;
  scripts = {
    mbr = pkgs.writeText "mbr-firstboot.sh" (evaluate false { });
    gpt = pkgs.writeText "gpt-firstboot.sh" (evaluate true { });
    # Reproduce the old hook's two removed commands after expansion but before
    # upstream maintenance. Upstream itself is never copied or reimplemented.
    broken = pkgs.writeText "broken-firstboot.sh" (
      evaluate false (
        { config, lib, ... }:
        {
          boot.postBootCommands = lib.mkOrder 750 ''
            if [ -f /nix-path-registration ]; then
              ${config.nix.package.out}/bin/nix-store --load-db < /nix-path-registration
              rm -f /nix-path-registration
            fi
          '';
        }
      )
    );
  };
  tools = lib.makeBinPath [
    pkgs.coreutils
    pkgs.util-linux
    pkgs.e2fsprogs
    pkgs.gptfdisk
    pkgs.nix
    pkgs.gnugrep
    pkgs.diffutils
  ];
  # All mounts disappear when this mount namespace exits, including on failure.
  inRoot = pkgs.writeShellScript "sd-test-in-root" ''
    set -euo pipefail
    export PATH=${tools}
    disk="$1"
    shift
    mkdir -p /mnt/sd-test
    mount "$disk"2 /mnt/sd-test
    mkdir -p /mnt/sd-test/{dev,sys,proc,nix/store,nix/var/nix,run,etc/nix,tmp,root}
    mount --rbind /dev /mnt/sd-test/dev
    mount --rbind /sys /mnt/sd-test/sys
    mount -t proc proc /mnt/sd-test/proc
    mount --bind /nix/store /mnt/sd-test/nix/store
    ln -sfn "$(readlink -f /run/current-system)" /mnt/sd-test/run/current-system
    # Force the local store, not the VM's daemon/database.
    export NIX_REMOTE=local HOME=/root
    exec chroot /mnt/sd-test "$@"
  '';
  run = pkgs.writeShellScript "sd-test-run" ''
    exec ${pkgs.util-linux}/bin/unshare --mount --propagation private ${inRoot} "$@"
  '';
  prepare = pkgs.writeShellScript "sd-test-prepare" ''
    set -euo pipefail
    export PATH=${tools}
    disk="$1"
    table="$2"
    # These are the two empty disks attached by runNixOSTest, never host disks.
    case "$disk" in /dev/vdb|/dev/vdc) ;; *) exit 1 ;; esac
    wipefs -a "$disk"
    printf 'label: %s\nstart=2048,size=16384\nstart=18432,size=262144\n' "$table" | sfdisk "$disk"
    partx -u "$disk"
    mkfs.ext4 -F "$disk"2
    mkdir -p /mnt/sd-seed
    mount "$disk"2 /mnt/sd-seed
    cp /var/lib/sd-test/registration /mnt/sd-seed/nix-path-registration
    echo preserve-user-data > /mnt/sd-seed/user-data
    umount /mnt/sd-seed
  '';
in
pkgs.testers.runNixOSTest {
  name = "korri-sd-firstboot";
  nodes.machine =
    { lib, ... }:
    {
      virtualisation.memorySize = 2048;
      virtualisation.emptyDiskImages = [
        384
        384
      ];
      environment.systemPackages = [
        pkgs.nix
        pkgs.gptfdisk
        pkgs.e2fsprogs
        pkgs.util-linux
      ];
      # A second actual VM boot invokes the same merged script on persisted disks.
      # The first boot leaves the disks blank for the failure/negative controls.
      boot.postBootCommands = lib.mkAfter ''
        if [ -e /var/lib/sd-test/second-boot ]; then
          ${run} /dev/vdb ${pkgs.bash}/bin/bash ${scripts.mbr} > /var/lib/sd-test/mbr-second.log 2>&1
          ${run} /dev/vdc ${pkgs.bash}/bin/bash ${scripts.gpt} > /var/lib/sd-test/gpt-second.log 2>&1
        fi
      '';
    };
  testScript = ''
    import shlex

    machine.start()
    machine.wait_for_unit("multi-user.target")
    machine.succeed("mkdir -p /var/lib/sd-test")
    # The real running NixOS closure, not an invented registration record.
    machine.succeed("nix-store --dump-db $(nix-store -qR /run/current-system) > /var/lib/sd-test/registration")
    runner = "${run}"
    scripts = {"mbr": "${scripts.mbr}", "gpt": "${scripts.gpt}", "broken": "${scripts.broken}"}

    def inside(disk, command):
        return f"{runner} {disk} ${pkgs.bash}/bin/bash -euc {shlex.quote(command)}"

    def boot(disk, kind, log):
        return f"{runner} {disk} ${pkgs.bash}/bin/bash {scripts[kind]} > /var/lib/sd-test/{log} 2>&1"

    def seed(disk, table):
        machine.succeed(f"${prepare} {disk} {table}")
        machine.succeed(inside(disk, "test -f /nix-path-registration; test ! -e /nix/var/nix/profiles/system; test ! -e /etc/NIXOS"))
        machine.fail(inside(disk, "nix-store --check-validity /run/current-system"))

    def initialized(disk):
        machine.succeed(inside(disk, "nix-store --check-validity /run/current-system; test /nix/var/nix/profiles/system -ef /run/current-system; test -f /etc/NIXOS; test ! -e /nix-path-registration; grep -Fx preserve-user-data /user-data"))

    with subtest("pre-fix marker consumption registers store but loses system profile"):
        seed("/dev/vdb", "dos")
        machine.succeed(boot("/dev/vdb", "broken", "broken.log"))
        machine.succeed(inside("/dev/vdb", "nix-store --check-validity /run/current-system; test ! -e /nix/var/nix/profiles/system; test ! -e /etc/NIXOS; test ! -e /nix-path-registration"))

    with subtest("failed real store registration retains the marker and retries"):
        seed("/dev/vdb", "dos")
        machine.succeed(inside("/dev/vdb", "printf 'not-a-store-path\\n' > /nix-path-registration"))
        machine.fail(boot("/dev/vdb", "mbr", "registration-failure.log"))
        machine.succeed(inside("/dev/vdb", "test -f /nix-path-registration; test ! -e /etc/NIXOS; test ! -e /nix/var/nix/profiles/system"))
        machine.succeed(f"cat /var/lib/sd-test/registration | {runner} /dev/vdb ${pkgs.bash}/bin/bash -c 'cat > /nix-path-registration'")
        machine.succeed(boot("/dev/vdb", "mbr", "registration-retry.log"))
        initialized("/dev/vdb")

    with subtest("failed real profile creation retains the marker after registration"):
        seed("/dev/vdb", "dos")
        machine.succeed(inside("/dev/vdb", "mkdir -p /nix/var/nix/profiles/system; touch /nix/var/nix/profiles/system/obstruction"))
        machine.fail(boot("/dev/vdb", "mbr", "profile-failure.log"))
        machine.succeed(inside("/dev/vdb", "nix-store --check-validity /run/current-system; test -f /etc/NIXOS; test -f /nix-path-registration; test -d /nix/var/nix/profiles/system"))
        machine.succeed(inside("/dev/vdb", "rm /nix/var/nix/profiles/system/obstruction; rmdir /nix/var/nix/profiles/system"))
        machine.succeed(boot("/dev/vdb", "mbr", "profile-retry.log"))
        initialized("/dev/vdb")

    with subtest("MBR and GPT grow before native registration and profile initialization"):
        for disk, kind, table in [("/dev/vdb", "mbr", "dos"), ("/dev/vdc", "gpt", "gpt")]:
            seed(disk, table)
            before = int(machine.succeed(f"blockdev --getsize64 {disk}2"))
            # vdb2/vdc2 have partition number 2, not their device minor number.
            machine.succeed(f"test $(cat /sys/class/block/{disk[5:]}2/partition) = 2")
            machine.succeed(f"test $(lsblk -dnro MAJ:MIN {disk}2 | cut -d: -f2) != 2")
            machine.succeed(boot(disk, kind, f"{kind}-first.log"))
            after = int(machine.succeed(f"blockdev --getsize64 {disk}2"))
            assert after > before, (kind, before, after)
            initialized(disk)
            # Filesystem growth, not just partition-table growth.
            blocks = int(machine.succeed(f"dumpe2fs -h {disk}2 2>/dev/null | sed -n 's/^Block count: *//p'"))
            block_size = int(machine.succeed(f"dumpe2fs -h {disk}2 2>/dev/null | sed -n 's/^Block size: *//p'"))
            assert blocks * block_size > before
            boot_log = machine.succeed(f"cat /var/lib/sd-test/{kind}-first.log")
            positions = [boot_log.index(s) for s in ["/bin/sfdisk", "/bin/resize2fs", "/bin/nix-store --load-db", "touch /etc/NIXOS", "/bin/nix-env -p", "rm -f /nix-path-registration"]]
            assert positions == sorted(positions), boot_log
            assert boot_log.count("/bin/nix-store --load-db") == 1
            assert boot_log.count("/bin/nix-env -p") == 1
            assert ("/bin/sgdisk -e" in boot_log) == (kind == "gpt")
            machine.succeed(inside(disk, "stat -c '%i %Y %Z %N' /nix/var/nix/profiles/system* > /profile-before-reboot"))
        machine.succeed("sgdisk -v /dev/vdc | grep -F 'No problems found'")

    with subtest("second VM boot leaves profiles and data unchanged without a marker"):
        machine.succeed("touch /var/lib/sd-test/second-boot")
        machine.shutdown()
        machine.start()
        machine.wait_for_unit("multi-user.target")
        for disk, kind in [("/dev/vdb", "mbr"), ("/dev/vdc", "gpt")]:
            initialized(disk)
            machine.succeed(inside(disk, "stat -c '%i %Y %Z %N' /nix/var/nix/profiles/system* > /profile-after-reboot; cmp /profile-before-reboot /profile-after-reboot"))
            machine.succeed(f"test -f /var/lib/sd-test/{kind}-second.log; test ! -s /var/lib/sd-test/{kind}-second.log")
        machine.copy_from_vm("/var/lib/sd-test", "evidence")
  '';
}
