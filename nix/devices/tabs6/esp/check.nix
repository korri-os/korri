# VM test for tabs6-esp: install, LAST KNOWN GOOD seeding, mark-good, rollback
# and the placeholder refusal, on a real FAT partition labelled `cache`, through
# the loop device and its cleanup trap. Runs on the x86_64 host; the script is
# architecture independent.
{ pkgs }:
let
  tabs6-esp = pkgs.callPackage ./package.nix { };
  fakeSystem =
    name: kind:
    pkgs.runCommand "fake-system-${name}" { } ''
      mkdir -p $out
      echo "kernel ${name}" > $out/kernel
      echo "dtb ${name}" > $out/tabs6-dtb
      echo "efi" > $out/tabs6-systemd-boot.efi
      echo "#!/bin/sh" > $out/init
      echo "console=tty0 root=PARTLABEL=userdata" > $out/kernel-params
      echo ${kind} > $out/tabs6-kernel-kind
    '';
  a = fakeSystem "a" "device";
  b = fakeSystem "b" "device";
  c = fakeSystem "c" "device";
  p = fakeSystem "p" "placeholder";
in
pkgs.testers.runNixOSTest {
  name = "tabs6-esp";
  nodes.machine = {
    virtualisation.emptyDiskImages = [ 512 ];
    environment.systemPackages = [
      tabs6-esp
      pkgs.gptfdisk
      pkgs.dosfstools
      pkgs.mtools
    ];
    system.extraDependencies = [
      a
      b
      c
      p
    ];
  };
  testScript = ''
    def entry(slot):
        return machine.succeed(
            f"mtype -i /dev/disk/by-partlabel/cache ::/loader/entries/tabs6-{slot}.conf"
            " | sed -n 's/^options init=\\(\\/nix\\/store\\/[^ ]*\\)\\/init.*/\\1/p'"
        ).strip()

    def kernel(slot):
        return machine.succeed(f"mtype -i /dev/disk/by-partlabel/cache ::/tabs6/{slot}/Image").strip()

    def clean():
        machine.succeed("test -z \"$(losetup -a)\"")
        machine.fail("grep -q /run/tabs6-esp /proc/mounts")

    machine.wait_for_unit("multi-user.target")
    machine.succeed(
        "sgdisk -n 1:0:0 -c 1:cache /dev/vdb",
        "udevadm settle",
        "mkfs.vfat -F 32 -S 512 /dev/disk/by-partlabel/cache",
        "tabs6-esp stage ${a} /tmp/stage",
        "MTOOLS_SKIP_CHECK=1 mcopy -s -i /dev/disk/by-partlabel/cache /tmp/stage/* ::/",
    )
    machine.succeed("echo 'mtools_skip_check=1' > /etc/mtools.conf")
    assert entry("current") == "${a}"

    # The booted system is the current entry, so installing B seeds lastgood.
    machine.succeed("ln -sfn ${a} /run/booted-system")
    machine.succeed("tabs6-esp install ${b}")
    assert entry("current") == "${b}" and kernel("current") == "kernel b"
    assert entry("lastgood") == "${a}" and kernel("lastgood") == "kernel a"
    machine.succeed("test $(readlink /nix/var/nix/gcroots/tabs6-lastgood) = ${a}")
    clean()

    # An existing lastgood is never replaced by install.
    machine.succeed("tabs6-esp install ${c}")
    assert entry("current") == "${c}" and entry("lastgood") == "${a}"
    clean()

    # mark-good needs the booted system to be the default entry.
    machine.fail("tabs6-esp mark-good")
    clean()
    machine.succeed("ln -sfn ${c} /run/booted-system")
    machine.succeed("tabs6-esp mark-good")
    assert entry("lastgood") == "${c}" and kernel("lastgood") == "kernel c"
    clean()

    # rollback restores lastgood as the default.
    machine.succeed("tabs6-esp install ${b}")
    machine.succeed("tabs6-esp rollback")
    assert entry("current") == "${c}" and kernel("current") == "kernel c"
    clean()

    # The placeholder kernel is refused before the ESP is opened.
    machine.fail("tabs6-esp install ${p}")
    assert entry("current") == "${c}"
    clean()

    # A partition that is not FAT is refused, never formatted.
    machine.succeed("wipefs -a /dev/disk/by-partlabel/cache")
    machine.fail("tabs6-esp install ${b}")
    clean()
  '';
}
