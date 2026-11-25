# Real InputPlumber, inputd, udev and uinput. No host input reaches the guest.
{
  pkgs,
  module,
  inputdPackage,
  inputplumberKorri,
  testProgram ? ../tests/inputplumber-adoption-vm.py,
  captureProof ? false,
  # Guest-only udev rule that holds or delays the USB gamepad's event node,
  # so InputPlumber can see it before udev initializes it.
  udevHold ? false,
  name ? null,
}:
let
  proofName = "korri-inputplumber-adoption";
  udevHoldProgram = pkgs.writeShellScript "korri-udev-hold" ''
    for _ in $(${pkgs.coreutils}/bin/seq 300); do
      test -e /run/korri-udev-hold || break
      ${pkgs.coreutils}/bin/sleep 0.1
    done
    test ! -e /run/korri-udev-delay || ${pkgs.coreutils}/bin/sleep "$(${pkgs.coreutils}/bin/cat /run/korri-udev-delay)"
  '';
  python = pkgs.python3.withPackages (ps: [
    ps.dbus-next
    ps.evdev
  ]);
  profile = "${inputplumberKorri}/share/inputplumber/profiles/${
    (import ./inputplumber-data.nix { inherit pkgs; }).resolvedProfile
  }";
  # Build off-device; invoke the very same public reconciler as inputd's main
  # loop, without adding korrid/streaming/session setup to this adoption proof.
  reconciler = inputdPackage.overrideAttrs {
    CARGO_BUILD_JOBS = "2";
    buildPhase = ''
      runHook preBuild
      cargoBuildLog=$(mktemp cargoBuildLogXXXX.json)
      cargo build --release --locked --example inputplumber-adoption-proof \
        --message-format json-render-diagnostics >"$cargoBuildLog"
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      mkdir -p "$out/bin"
      cp target/release/examples/inputplumber-adoption-proof "$out/bin/"
      runHook postInstall
    '';
    postInstall = "";
  };
in
pkgs.testers.runNixOSTest {
  name =
    if name != null then
      name
    else if captureProof then
      "korri-inputplumber-reattach"
    else
      proofName;
  nodes.machine = { lib, ... }: {
    imports = [ module ];
    networking.hostName = proofName;
    environment.etc."${proofName}-vm".text = "isolated-kernel-proof-only\n";
    environment.systemPackages = [ python ];
    virtualisation = {
      memorySize = 1536;
      cores = 2;
      qemu.options = [ "-smbios type=1,product=${proofName}" ];
    };
    boot.kernelModules = [
      "uinput"
      "joydev"
    ]
    ++ pkgs.lib.optionals captureProof [
      "dummy_hcd"
      "libcomposite"
      "usb_f_hid"
    ];
    users.groups.korri.gid = 1000;
    users.users.korri = {
      isNormalUser = true;
      uid = 1000;
      group = "korri";
    };
    services.korriLinuxInput = {
      provider = {
        enable = true;
        package = inputplumberKorri;
      };
      inputd = {
        enable = true;
        package = inputdPackage;
        requireProvider = true;
        uid = 977;
        controlGid = 977;
        actionUser = "korri";
        actionUid = 1000;
        actionGid = 1000;
      };
    };
    systemd.services.inputplumber.environment.LOG_LEVEL = lib.mkForce "debug";
    # The pinned daemon passes /devices/... (without /sys) to udevadm info.
    # Normalize only that argument in this guest, then use the real udevadm:
    # the admission property below must come from udev, not a canned reply.
    systemd.services.inputplumber.path = lib.mkBefore [
      (pkgs.writeShellScriptBin "udevadm" ''
        if test "$#" -eq 2 && test "$1" = info && [[ "$2" == /devices/* ]]; then
          exec ${pkgs.systemd}/bin/udevadm info --path="/sys$2"
        fi
        exec ${pkgs.systemd}/bin/udevadm "$@"
      '')
    ];
    systemd.services.korri-inputd.wantedBy = lib.mkForce [ ];
    systemd.services.adoption-profile = {
      wantedBy = [ "multi-user.target" ];
      # The driver's bounded system-bus connect must not race dbus startup
      # on a loaded builder (TimedOut at boot, 2026-11-25).
      requires = [
        "inputplumber.service"
        "dbus.service"
      ];
      after = [
        "inputplumber.service"
        "dbus.service"
      ];
      serviceConfig = {
        User = "korri-inputd";
        Group = "korri-control";
        ExecStart = "${reconciler}/bin/inputplumber-adoption-proof ${profile}${pkgs.lib.optionalString captureProof " --capture"}";
      };
    };
    # InputPlumber normally skips USB uinput devices before profile matching.
    # In this guest ONLY, admit them through its Bluetooth virtual-device check.
    # Keep their kernel USB IDs/version intact (especially 360 0101 vs 0001).
    # This also admits our output fixtures, so an ignored result proves the
    # explicit exclusion, not InputPlumber's general virtual-device heuristic.
    services.udev.extraRules =
      pkgs.lib.optionalString (!captureProof) ''
        SUBSYSTEM=="input", KERNEL=="event*", ENV{ID_BUS}="bluetooth"
      ''
      + pkgs.lib.optionalString udevHold ''
        SUBSYSTEM=="input", KERNEL=="event*", ATTRS{idVendor}=="1532", ATTRS{idProduct}=="0712", PROGRAM="${udevHoldProgram}"
      '';
  };
  testScript = ''
    machine.start()
    machine.wait_for_unit("multi-user.target")
    machine.wait_for_unit("inputplumber.service")
    machine.wait_for_unit("adoption-profile.service")
    try:
        print(machine.succeed("${python}/bin/python3 ${testProgram} ${profile} ${../tests/inputplumber-adoption-vm.py} ${../tests/inputplumber-reattach-vm.py}", timeout=240))
        machine.succeed("systemctl is-active --quiet inputplumber.service adoption-profile.service")
    finally:
        print(machine.succeed("journalctl -u inputplumber -u adoption-profile -u systemd-udevd --no-pager"))
        print(machine.succeed("${pkgs.acl}/bin/getfacl /dev/input/event*"))
  '';
}
