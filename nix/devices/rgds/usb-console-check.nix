# Exercise the real NixOS unit-directory builder. Checking wantedBy alone
# missed an instance file that masked the serial-getty template's base rules.
# An instance drop-in is equally wrong: systemd keeps only the most specific
# drop-in of a given name, which would discard the packaged agetty command.
{ pkgs, config }:
let
  inherit (pkgs) lib;
  utils = import (pkgs.path + "/nixos/lib/utils.nix") { inherit lib config pkgs; };
  systemdLib = import (pkgs.path + "/nixos/lib/systemd-lib.nix") {
    inherit
      lib
      config
      pkgs
      utils
      ;
  };
  present = lib.filter (name: config.systemd.units ? ${name}) [
    "serial-getty@.service"
    "serial-getty@ttyGS0.service"
  ];
  units = lib.mapAttrs (
    name: unit:
    unit
    // {
      # Preserve the exported device's exact unit text and override strategy,
      # but materialize that text using this check's native build platform.
      unit = pkgs.writeTextDir name unit.text;
    }
  ) (lib.getAttrs present config.systemd.units);
  directory = systemdLib.generateUnits {
    type = "system";
    inherit units;
    upstreamUnits = [ "serial-getty@.service" ];
    upstreamWants = [ "multi-user.target.wants" ];
    packages = [ ];
    package = pkgs.systemd;
  };
  rules = config.services.udev.extraRules;
in
assert lib.hasInfix ''KERNEL=="ttyGS0"'' rules;
assert lib.hasInfix ''ENV{SYSTEMD_WANTS}+="serial-getty@ttyGS0.service"'' rules;
pkgs.runCommand "rgds-usb-console-check" { } ''
  # The generated tree must not shadow the template unit or its drop-in.
  test ! -e ${directory}/serial-getty@ttyGS0.service
  test ! -e ${directory}/serial-getty@ttyGS0.service.d
  template=${directory}/serial-getty@.service
  grep -Fx 'BindsTo=dev-%i.device' "$template"
  grep -E '^After=.*dev-%i.device' "$template"
  grep -Fx 'Restart=always' "$template"
  grep -Fx 'StandardInput=tty' "$template"
  grep -Fx 'TTYPath=/dev/%I' "$template"
  # The drop-in must clear the upstream command and supply exactly one
  # packaged login command with root autologin and the serial tty arguments.
  # Native getty arguments can start with --login-program and include
  # --issue-file before --autologin; allow zero or more arguments in each gap.
  drop_in=${directory}/serial-getty@.service.d/overrides.conf
  grep -Fx 'ExecStart=' "$drop_in"
  test "$(grep -c '^ExecStart=' "$drop_in")" -eq 2
  grep -E '^ExecStart=/nix/store/[^ ]+/bin/agetty (.* )?--login-program /nix/store/[^ ]+/bin/login (.* )?--autologin root %I --keep-baud [$]TERM$' "$drop_in"
  touch "$out"
''
