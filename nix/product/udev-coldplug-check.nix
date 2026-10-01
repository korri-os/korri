# Build-time behaviour of the coldplug module list and program, with fixture
# rule files and a recording udevadm. These prove rule reading, refusals and
# the exact trigger commands. They cannot prove that the kernel delivers the
# events; the RG353M boot journal did that (2026-10-01).
{ pkgs }:
let
  inherit (pkgs) lib;
  dir =
    name: files:
    pkgs.runCommand "udev-coldplug-fixture-${name}" { } (
      ''
        mkdir -p $out
      ''
      + lib.concatStrings (
        lib.mapAttrsToList (file: text: ''
          cat > $out/${file} <<'EOF'
          ${text}
          EOF
        '') files
      )
    );
  modules =
    name: files:
    pkgs.callPackage ./udev-coldplug-modules.nix {
      ruleDirs = [ (dir name files) ];
    };

  # Lines as systemd 258 ships them, including a continued rule.
  systemdLike = modules "systemd-like" {
    "60-block.rules" = ''
      ACTION=="add", SUBSYSTEM=="module", KERNEL=="block", ATTR{parameters/events_dfl_poll_msecs}=="0", \
        ATTR{parameters/events_dfl_poll_msecs}="2000"
    '';
    "80-drivers.rules" = ''
      SUBSYSTEM=="module", KERNEL=="parport_pc", RUN{builtin}+="kmod load ppdev"
    '';
    "99-systemd.rules" = ''
      # SUBSYSTEM=="drivers" in a comment is ignored
      SUBSYSTEM=="module", KERNEL=="fuse", TAG+="systemd"
      SUBSYSTEM=="module", KERNEL=="configfs", TAG+="systemd"
      SUBSYSTEM=="input", KERNEL=="event*", TAG+="uaccess"
      KERNEL=="tun", TAG+="systemd"
    '';
  };
  devicesOnly = modules "devices-only" {
    "70-x.rules" = ''SUBSYSTEM=="input", KERNEL=="event*", MODE="0600"'';
  };
  refuses =
    name: files: message:
    let
      failure = pkgs.testers.testBuildFailure (modules name files);
    in
    pkgs.runCommand "udev-coldplug-refuses-${name}" { } ''
      grep -F ${lib.escapeShellArg message} ${failure}/testBuildFailure.log
      touch $out
    '';

  # Records each call, one argv per line, into $UDEVADM_LOG.
  fakeUdevadm = pkgs.writeShellScript "udevadm" ''echo "$*" >> "$UDEVADM_LOG"'';
  program = pkgs.callPackage ./udev-coldplug-package.nix { udevadm = fakeUdevadm; };
in
pkgs.runCommand "korri-product-udev-coldplug-check"
  {
    nativeBuildInputs = [ program ];
    refusals = [
      (refuses "drivers" { "x.rules" = ''SUBSYSTEM=="drivers", DRIVER=="x", RUN+="/bin/true"''; } "rule needs drivers events")
      (refuses "bus" { "x.rules" = ''SUBSYSTEM=="usb|bus", RUN+="/bin/true"''; } "rule needs bus events")
      (refuses "module-glob" { "x.rules" = ''SUBSYSTEM=="module", KERNEL=="snd_*", RUN+="/bin/true"''; } "module rule matches KERNEL by pattern")
      (refuses "module-no-kernel" { "x.rules" = ''ACTION=="add", SUBSYSTEM=="module", RUN+="/bin/true"''; } "module rule names no KERNEL")
      (refuses "subsystem-glob" { "x.rules" = ''SUBSYSTEM=="mod*", RUN+="/bin/true"''; } "SUBSYSTEM pattern matches subsystem events")
    ];
  }
  ''
    set -euo pipefail
    diff <(printf '%s\n' block configfs fuse parport_pc) ${systemdLike}
    test ! -s ${devicesOnly}

    export UDEVADM_LOG=$PWD/named.log
    KORRI_UDEV_COLDPLUG_MODULES=${systemdLike} korri-udev-coldplug
    diff - named.log <<'EOF'
    trigger --type=subsystems --action=add --subsystem-match=module --sysname-match=block --sysname-match=configfs --sysname-match=fuse --sysname-match=parport_pc
    trigger --type=devices --action=add --prioritized-subsystem=block,tpmrm,net,tty,input
    EOF

    # No named module: no module trigger, which would otherwise replay every module.
    export UDEVADM_LOG=$PWD/none.log
    KORRI_UDEV_COLDPLUG_MODULES=${devicesOnly} korri-udev-coldplug
    diff - none.log <<'EOF'
    trigger --type=devices --action=add --prioritized-subsystem=block,tpmrm,net,tty,input
    EOF

    # A missing module list is a configuration error, not a silent full replay.
    unset KORRI_UDEV_COLDPLUG_MODULES
    if korri-udev-coldplug 2> unset.err; then echo "ran without a module list" >&2; exit 1; fi
    touch $out
  ''
