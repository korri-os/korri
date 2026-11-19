# VM test for tabs6-esp: install, LAST KNOWN GOOD seeding, boot counting
# (trial, bless, the systemd-boot rename), mark-good, rollback and the
# placeholder refusal, on a real FAT partition labelled `cache`, through the
# loop device and its cleanup trap. Runs on the x86_64 host; the script is
# architecture independent. The VM boots without UEFI, so systemd-boot's own
# rename is simulated with mren; the tablet did the real one (README.md).
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
    def entries():
        return sorted(machine.succeed(
            "mdir -b -i /dev/disk/by-partlabel/cache ::/loader/entries | sed 's|.*/||'"
        ).split())

    def name(slot):
        found = [e for e in entries() if e == f"tabs6-{slot}.conf" or e.startswith(f"tabs6-{slot}+")]
        assert len(found) == 1, found
        return found[0]

    def entry(slot):
        return machine.succeed(
            f"mtype -i /dev/disk/by-partlabel/cache ::/loader/entries/{name(slot)}"
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
    # A fresh image has nothing to fall back to: its entry is not counted.
    assert name("current") == "tabs6-current.conf"

    # The booted system is the current entry, so installing B seeds lastgood.
    machine.succeed("ln -sfn ${a} /run/booted-system")
    machine.succeed("tabs6-esp install ${b}")
    assert entry("current") == "${b}" and kernel("current") == "kernel b"
    assert entry("lastgood") == "${a}" and kernel("lastgood") == "kernel a"
    machine.succeed("test $(readlink /nix/var/nix/gcroots/tabs6-lastgood) = ${a}")
    # The new default gets one trial boot; a failed trial falls to lastgood.
    assert name("current") == "tabs6-current+1-0.conf"
    loader = machine.succeed("mtype -i /dev/disk/by-partlabel/cache ::/loader/loader.conf")
    assert "preferred tabs6-current.conf" in loader and "default tabs6-lastgood.conf" in loader
    assert "default tabs6-current.conf" not in loader
    clean()

    # Booted as A, B's entry is not this boot's trial and cannot be blessed.
    machine.fail("tabs6-esp trial")
    machine.fail("tabs6-esp bless")
    assert name("current") == "tabs6-current+1-0.conf"
    clean()

    # systemd-boot renames the entry before it boots it; B is then the trial.
    machine.succeed("mren -i /dev/disk/by-partlabel/cache ::/loader/entries/tabs6-current+1-0.conf ::/loader/entries/tabs6-current+0-1.conf")
    machine.succeed("ln -sfn ${b} /run/booted-system")
    machine.succeed("tabs6-esp trial")
    clean()
    machine.succeed("tabs6-esp bless")
    assert name("current") == "tabs6-current.conf" and entry("current") == "${b}"
    machine.fail("tabs6-esp trial")
    # Blessing a blessed entry changes nothing.
    machine.succeed("tabs6-esp bless")
    assert name("current") == "tabs6-current.conf"
    clean()
    machine.succeed("ln -sfn ${a} /run/booted-system")

    # An existing lastgood is never replaced by install, and the blessed
    # entry of B gives way to one counted entry for C.
    machine.succeed("tabs6-esp install ${c}")
    assert entry("current") == "${c}" and entry("lastgood") == "${a}"
    assert name("current") == "tabs6-current+1-0.conf"
    clean()

    # mark-good needs the booted system to be the default entry.
    machine.fail("tabs6-esp mark-good")
    clean()
    machine.succeed("ln -sfn ${c} /run/booted-system")
    machine.succeed("tabs6-esp mark-good")
    assert entry("lastgood") == "${c}" and kernel("lastgood") == "kernel c"
    # mark-good blesses the trial first.
    assert name("current") == "tabs6-current.conf"
    clean()

    # rollback restores lastgood as the default, without a counter.
    machine.succeed("tabs6-esp install ${b}")
    machine.succeed("tabs6-esp rollback")
    assert entry("current") == "${c}" and kernel("current") == "kernel c"
    assert name("current") == "tabs6-current.conf"
    assert len([e for e in entries() if e.startswith("tabs6-current")]) == 1
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
