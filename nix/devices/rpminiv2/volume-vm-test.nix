# Exercise the emitted inputd mount sandbox with real PipeWire and wpctl.
# Only the executable/identity are replaced for this x86 test; ARM binaries,
# controller devices and the board kernel are not needed to test this boundary.
{ pkgs, configuration }:
let
  inherit (pkgs) lib;
  c = configuration.config;
  host = c.services.korriLinuxHost;
  inputd = c.systemd.services.korri-inputd;
  runtimeDir = "/run/user/${toString host.runtimeUid}";
  actions = c.services.korriLinuxInput.inputd.actions;
  command =
    name:
    lib.escapeShellArgs (
      [ "env" ]
      ++ lib.mapAttrsToList (key: value: "${key}=${value}") actions.${name}.environment
      ++ [ "${pkgs.wireplumber}/bin/wpctl" ]
      ++ builtins.tail actions.${name}.command
    );
in
pkgs.testers.runNixOSTest {
  name = "rpminiv2-volume";
  nodes.machine =
    { pkgs, ... }:
    {
      users.groups.${host.runtimeGroup}.gid = host.runtimeGid;
      users.users.${host.runtimeUser} = {
        uid = host.runtimeUid;
        group = host.runtimeGroup;
        isNormalUser = true;
        linger = true;
      };
      services.pipewire = {
        enable = true;
        wireplumber.enable = true;
        extraConfig.pipewire."99-test-sink"."context.objects" = [
          {
            factory = "adapter";
            args = {
              "factory.name" = "support.null-audio-sink";
              "node.name" = "volume-test-sink";
              "media.class" = "Audio/Sink";
              "audio.position" = [
                "FL"
                "FR"
              ];
            };
          }
        ];
      };
      systemd.user.services.test-playback = {
        serviceConfig.ExecStart = pkgs.writeShellScript "test-playback" ''
          exec ${pkgs.pipewire}/bin/pw-cat --raw --playback --target=volume-test-sink --rate=48000 --channels=2 --format=s16 - < /dev/zero
        '';
      };
      systemd.tmpfiles.rules = [
        "d /dev/inputplumber/sources 0755 root root -"
        "d /var/lib/korrid 0700 root root -"
      ];
      systemd.services.volume-sandbox = {
        wantedBy = [ "multi-user.target" ];
        # Retain the board's user-manager ordering, not controller dependencies.
        wants = lib.filter (lib.hasPrefix "user@") inputd.wants;
        after = [ "systemd-tmpfiles-setup.service" ] ++ lib.filter (lib.hasPrefix "user@") inputd.after;
        serviceConfig =
          (builtins.removeAttrs inputd.serviceConfig [
            "ExecStart"
            "ExecStartPre"
            "ExecStopPost"
            "User"
            "Group"
            "Type"
            "NotifyAccess"
          ])
          // {
            Type = "simple";
            ExecStart = "${pkgs.coreutils}/bin/sleep infinity";
            User = host.runtimeUser;
            Group = host.runtimeGroup;
          };
      };
      environment.systemPackages = [
        pkgs.wireplumber
        pkgs.util-linux
      ];
      virtualisation.memorySize = 1024;
    };
  testScript = ''
    start_all()
    runtime = "${runtimeDir}"
    uid = "${toString host.runtimeUid}"
    gid = "${toString host.runtimeGid}"
    user = "${host.runtimeUser}"
    wpctl = "${pkgs.wireplumber}/bin/wpctl"
    identity = f"setpriv --reuid={uid} --regid={gid} --clear-groups env XDG_RUNTIME_DIR={runtime}"

    def outside(command):
        return f"{identity} {command}"

    def audio(command):
        return outside(f"systemctl --user {command}")

    def sandbox(command):
        pid = machine.succeed("systemctl show volume-sandbox -p MainPID --value").strip()
        return f"nsenter -t {pid} -m -- {identity} {command}"

    def volume(expected):
        print(machine.succeed(outside(f"{wpctl} get-volume @DEFAULT_AUDIO_SINK@")))
        machine.wait_until_succeeds(outside(f"{wpctl} get-volume @DEFAULT_AUDIO_SINK@") + f" | grep -Fx 'Volume: {expected}'", timeout=30)

    def private_paths():
        # Check as root too: absence must come from mounts, not DAC permissions.
        pid = machine.succeed("systemctl show volume-sandbox -p MainPID --value").strip()
        for path in [f"/home/{user}/private", "/root/private", f"{runtime}/private", f"{runtime}/bus"]:
            machine.succeed(f"test -e {path}")
            machine.succeed(f"nsenter -t {pid} -m -- test ! -e {path}")
        machine.succeed(f"nsenter -t {pid} -m -- findmnt -n -o OPTIONS -T {runtime}/pipewire-0 | grep -w ro")

    machine.wait_for_unit(f"user@{uid}.service")
    with subtest("cold boot binds the socket before manual audio setup"):
        machine.wait_for_unit("volume-sandbox.service")
        machine.succeed(sandbox(f"test -S {runtime}/pipewire-0"))
        assert machine.succeed("systemctl show volume-sandbox -p NRestarts --value").strip() == "0"
    machine.succeed(audio("start pipewire.socket pipewire.service wireplumber.service"))
    machine.wait_until_succeeds(outside(f"{wpctl} get-volume @DEFAULT_AUDIO_SINK@") + " | grep '^Volume:'")
    machine.succeed(f"touch /home/{user}/private /root/private {runtime}/private")
    machine.succeed(audio("start test-playback.service"))
    # The null adapter applies volume only after negotiating an audio format.
    machine.wait_until_succeeds(outside("${pkgs.pipewire}/bin/pw-link -l") + " | grep volume-test-sink", timeout=30)
    machine.succeed(outside(f"{wpctl} set-volume @DEFAULT_AUDIO_SINK@ 40%"))
    volume("0.40")
    machine.wait_for_unit("volume-sandbox.service")

    with subtest("both production volume actions cross only the audio socket"):
        machine.succeed(sandbox(f"{wpctl} get-volume @DEFAULT_AUDIO_SINK@"))
        private_paths()
        machine.succeed(sandbox(${builtins.toJSON (command "volume-up")}))
        volume("0.45")
        machine.succeed(sandbox(${builtins.toJSON (command "volume-down")}))
        volume("0.40")

    with subtest("socket activation preserves the bound inode across daemon restart"):
        inode = machine.succeed(f"stat -c %i {runtime}/pipewire-0").strip()
        machine.succeed(audio("stop wireplumber.service pipewire.service"))
        machine.succeed(audio("is-active pipewire.socket"))
        machine.succeed(sandbox(f"timeout 20 {wpctl} status"))
        machine.succeed(audio("start wireplumber.service"))
        machine.wait_until_succeeds(sandbox(f"{wpctl} get-volume @DEFAULT_AUDIO_SINK@") + " | grep '^Volume:'")
        assert machine.succeed(f"stat -c %i {runtime}/pipewire-0").strip() == inode
        private_paths()

    with subtest("replacing the socket requires rebinding the sandbox"):
        machine.succeed(audio("stop test-playback.service wireplumber.service pipewire.service pipewire.socket"))
        machine.succeed(f"rm {runtime}/pipewire-0")
        machine.succeed(audio("start pipewire.socket pipewire.service wireplumber.service"))
        machine.wait_until_succeeds(outside(f"{wpctl} get-volume @DEFAULT_AUDIO_SINK@") + " | grep '^Volume:'", timeout=30)
        machine.fail(sandbox(f"timeout 5 {wpctl} get-volume @DEFAULT_AUDIO_SINK@"))
        machine.succeed("systemctl restart volume-sandbox")
        machine.succeed(sandbox(f"{wpctl} get-volume @DEFAULT_AUDIO_SINK@"))
        private_paths()

    with subtest("missing optional audio does not prevent sandbox startup"):
        machine.succeed("systemctl stop volume-sandbox")
        machine.succeed(audio("stop test-playback.service wireplumber.service pipewire.service pipewire.socket"))
        # systemd leaves the pathname on socket stop (RemoveOnStop=false).
        machine.succeed(f"rm {runtime}/pipewire-0; test ! -e {runtime}/pipewire-0")
        machine.succeed("systemctl start volume-sandbox")
        machine.wait_for_unit("volume-sandbox.service")
        machine.fail(sandbox(f"{wpctl} get-volume @DEFAULT_AUDIO_SINK@"))
        # A missing/replaced socket needs a new bind mount, not more exposure.
        machine.succeed(audio("start pipewire.socket pipewire.service wireplumber.service"))
        machine.wait_until_succeeds(outside(f"{wpctl} get-volume @DEFAULT_AUDIO_SINK@") + " | grep '^Volume:'")
        machine.succeed("systemctl restart volume-sandbox")
        machine.wait_until_succeeds(sandbox(f"{wpctl} get-volume @DEFAULT_AUDIO_SINK@") + " | grep '^Volume:'")
        private_paths()
  '';
}
