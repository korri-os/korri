# Real InputPlumber, inputd, udev and uinput. No host input reaches the guest.
{
  pkgs,
  module,
  inputdPackage,
  inputplumberKorri,
}:
let
  proofName = "korri-inputplumber-adoption";
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
  name = proofName;
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
    systemd.services.korri-inputd.wantedBy = lib.mkForce [ ];
    systemd.services.adoption-profile = {
      wantedBy = [ "multi-user.target" ];
      requires = [ "inputplumber.service" ];
      after = [ "inputplumber.service" ];
      serviceConfig = {
        User = "korri-inputd";
        Group = "korri-control";
        ExecStart = "${reconciler}/bin/inputplumber-adoption-proof ${profile}";
      };
    };
    # InputPlumber normally skips USB uinput devices before profile matching.
    # In this guest ONLY, admit them through its Bluetooth virtual-device check.
    # Keep their kernel USB IDs/version intact (especially 360 0101 vs 0001).
    # This also admits our output fixtures, so an ignored result proves the
    # explicit exclusion, not InputPlumber's general virtual-device heuristic.
    services.udev.extraRules = ''
      SUBSYSTEM=="input", KERNEL=="event*", ENV{ID_BUS}="bluetooth"
    '';
  };
  testScript = ''
    machine.start()
    machine.wait_for_unit("multi-user.target")
    machine.wait_for_unit("inputplumber.service")
    machine.wait_for_unit("adoption-profile.service")
    try:
        print(machine.succeed("${python}/bin/python3 ${../tests/inputplumber-adoption-vm.py} ${profile}", timeout=240))
        machine.succeed("systemctl is-active --quiet inputplumber.service adoption-profile.service")
    finally:
        print(machine.succeed("journalctl -u inputplumber -u adoption-profile --no-pager"))
  '';
}
