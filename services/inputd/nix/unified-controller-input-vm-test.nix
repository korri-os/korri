# Receiver/kernel acceptance only; no physical capture, browser, or GUI claim.
# The package is built outside the guest. No host input device is forwarded.
{ pkgs, inputdPackage }:
let
  proofName = "korri-unified-controller-input";
  python = pkgs.python3;
  proof = ../tests/unified-controller-input-vm.py;
  guard = pkgs.writeShellScript "unified-controller-vm-guard" ''
    set -eu
    test "$(cat /etc/hostname)" = '${proofName}'
    test "$(cat /etc/${proofName}-vm)" = 'isolated-kernel-proof-only'
    test "$(cat /sys/class/dmi/id/product_name)" = '${proofName}'
  '';
in
pkgs.testers.runNixOSTest {
  name = proofName;
  nodes.machine = {
    networking.hostName = proofName;
    environment.etc."${proofName}-vm".text = "isolated-kernel-proof-only\n";
    environment.etc."${proofName}-sources.json".text = builtins.toJSON {
      inputdPackage = toString inputdPackage;
      receiver = builtins.hashFile "sha256" ../src/input_seat_receiver.rs;
      runtime = builtins.hashFile "sha256" ../src/input_seat.rs;
      pool = builtins.hashFile "sha256" ../src/seat_pool.rs;
      encoder = builtins.hashFile "sha256" ../src/input_seat_uinput.rs;
      contract = builtins.hashFile "sha256" ../../../contracts/input/src/lib.rs;
      rules = builtins.hashFile "sha256" ./input-seat-rules.nix;
    };
    environment.systemPackages = [ python ];
    virtualisation = {
      memorySize = 1024;
      cores = 2;
      qemu.options = [ "-smbios type=1,product=${proofName}" ];
    };
    boot.kernelModules = [
      "uinput"
      "joydev"
    ];
    users.groups = {
      korrid.gid = 976;
      sunshine.gid = 980;
      korri.gid = 1000;
      unrelated.gid = 1001;
      uinput = { };
    };
    users.users = {
      korrid = {
        isSystemUser = true;
        uid = 976;
        group = "korrid";
      };
      korri = {
        isNormalUser = true;
        uid = 1000;
        group = "korri";
      };
      unrelated = {
        isNormalUser = true;
        uid = 1001;
        group = "unrelated";
      };
    };
    # Import the exact production rule, including all identity matches. Never
    # chown/chmod event nodes in the proof to make receiver readiness succeed.
    services.udev.packages = [
      (import ./input-seat-rules.nix {
        inherit pkgs;
        eventGid = 1000;
      })
    ];
    # Deliberately not wantedBy: both the driver guard and ExecStartPre must
    # pass before this prebuilt binary can make any uinput calls.
    systemd.services.korri-input-seat-receiver = {
      after = [ "systemd-udevd.service" ];
      serviceConfig = {
        Type = "notify";
        User = "root";
        Group = "root";
        SupplementaryGroups = [ "uinput" ];
        RuntimeDirectory = "korri-input-seat";
        RuntimeDirectoryMode = "0711";
        ExecStartPre = guard;
        ExecStart = "${inputdPackage}/bin/korri-input-seat-receiver --runtime-dir /run/korri-input-seat --control-uid 976 --control-gid 976 --sunshine-uid 1000 --sunshine-gid 980 --event-gid 1000";
        # Match production confinement; omit bundle selection and automatic
        # restart so a crash cannot silently satisfy the acceptance assertions.
        UMask = "0077";
        NoNewPrivileges = true;
        CapabilityBoundingSet = [ "CAP_CHOWN" ];
        PrivateTmp = true;
        PrivatePIDs = true;
        PrivateDevices = false;
        DevicePolicy = "closed";
        DeviceAllow = [ "/dev/uinput rw" ];
        ProtectSystem = "strict";
        ProtectHome = true;
        ProtectProc = "invisible";
        ProcSubset = "pid";
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectControlGroups = true;
        ProtectClock = true;
        ProtectHostname = true;
        RestrictSUIDSGID = true;
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        SystemCallArchitectures = "native";
        RestrictAddressFamilies = [ "AF_UNIX" ];
        ReadWritePaths = [ "/run/korri-input-seat" ];
      };
    };
  };
  testScript = ''
    machine.start()
    machine.wait_for_unit("multi-user.target")
    machine.succeed("${guard}")
    machine.fail("systemctl is-active --quiet korri-input-seat-receiver.service")
    machine.succeed("systemctl start korri-input-seat-receiver.service")
    machine.wait_for_unit("korri-input-seat-receiver.service")
    # Test the real gamepad APIs as the gameplay UID, without guest compilation.
    print(machine.succeed("KORRI_SEAT_SDL2_LIBRARY=${pkgs.SDL2}/lib/libSDL2.so KORRI_SEAT_SDL3_LIBRARY=${pkgs.sdl3}/lib/libSDL3.so ${python}/bin/python3 ${proof}", timeout=180))
    machine.succeed("systemctl stop korri-input-seat-receiver.service")
    print(machine.succeed("${python}/bin/python3 ${proof} --assert-absent"))
  '';
}
