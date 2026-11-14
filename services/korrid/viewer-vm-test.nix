# #435: korrid's viewer selection on a cold NixOS guest, with the real
# plugin host and korrid's real module, units, polkit and sandbox.
#
# korrid starts a viewer only inside a federated stream (upstreams.rs). This
# test drives the same production `installed_viewer` through
# src/bin/viewer_probe.rs instead, so the peer RPC that triggers a start is
# not exercised. The probe runs as korrid, with korrid's environment and
# service sandbox. It owns only the peer's part: the certificate broker
# returns the sender's certificate for the one paired client certificate.
#
# The sender is a private Sunshine on a second headless Sway in the guest,
# with a software encoder. It never stands for a device's picture path.
{
  pkgs,
  korridPackage,
  korridModule,
  hostModule,
  hostPackage,
  # The real `@korri:viewer`, the same plugin with one byte of its treaty
  # changed, and `@korri:moonlight`, all built by the plugins repository.
  viewerPlugin,
  otherContractPlugin,
  moonlightPlugin,
}:
let
  host = hostPackage.overrideAttrs { doCheck = false; };
  probe = korridPackage.viewerProbe;
  # The disposable VM cache key of the plugin-host tests. It grants nothing.
  binding = {
    publicKey = "korri-plugin-test:r7JN4vIsXpQU56Pfw9c9FD4IavDfrsxHH/uXSwxWvSU=";
    cacheUrl = "https://cache.example.invalid";
  };
  key = pkgs.writeText "viewer-lifecycle-test-key" "korri-plugin-test:XMn+6POJ5fj568Beg6v8OLo4wMcNKehDPxH+7bUrt0Svsk3i8ixelBTno9/D1z0UPghq8N+uzEcf+5dLDFa9JQ==";
  bindings = pkgs.writeText "viewer-lifecycle-publishers.json" (
    builtins.toJSON { "@korri" = binding; }
  );
  receiptFor =
    package:
    pkgs.runCommand "viewer-lifecycle-receipt" { } ''
      ${host}/bin/korri-plugin seed-graph ${bindings} ${package} > graph.json
      ${pkgs.jq}/bin/jq -c '.[0]' graph.json > "$out"
    '';
  seeded = [
    {
      id = "@korri:viewer";
      package = viewerPlugin;
    }
    {
      id = "@korri:moonlight";
      package = moonlightPlugin;
    }
  ];
  korridUid = 976;
  runtimeUid = 1000;
  hostUuid = "6b6f7272-6976-4965-b77e-7266697874e5";
  sway = pkgs.sway;
  viewerSway = pkgs.writeText "viewer-sway.conf" ''
    output HEADLESS-1 mode 1280x720
    xwayland enable
    default_border none
  '';
  senderSway = pkgs.writeText "sender-sway.conf" ''
    output HEADLESS-1 mode 640x480
    xwayland disable
    exec ${pkgs.foot}/bin/foot ${pkgs.procps}/bin/top -d 0.2
    exec ${pkgs.sunshine}/bin/sunshine /var/lib/sender/sunshine.conf
  '';
  # Sunshine's own file format (stream-cadence sender.py), on fixed ports.
  sunshineConfig = pkgs.writeText "sunshine.conf" ''
    port = 47989
    capture = wlr
    encoder = software
    file_apps = /var/lib/sender/apps.json
    file_state = /var/lib/sender/state.json
    credentials_file = /var/lib/sender/credentials.json
    pkey = /var/lib/sender/sunshine.key
    cert = /var/lib/sender/sunshine.crt
    log_path = /var/lib/sender/sunshine.log
    upnp = disabled
    system_tray = disabled
    mouse = disabled
    keyboard = disabled
    controller = disabled
    origin_web_ui_allowed = pc
  '';
  apps = pkgs.writeText "apps.json" (
    builtins.toJSON {
      env = { };
      apps = [ { name = "Korri Stream"; } ];
    }
  );
  # Sunshine's nvhttp state: the host UUID and one paired client.
  pairing = pkgs.writeText "pair.py" ''
    import json, sys
    certificate = open(sys.argv[1]).read()
    json.dump({"root": {"uniqueid": "${hostUuid}", "named_devices": [
        {"name": "korri-viewer-test", "cert": certificate,
         "uuid": "00000000-0000-4000-8000-000000000001", "enabled": True}]}},
        open(sys.argv[2], "w"))
  '';
  # One byte of the real treaty changed, as the other plugin declares it.
  flipped = pkgs.writeText "flip.py" ''
    import hashlib, sys
    data = bytearray(open(sys.argv[1], "rb").read())
    data[0] ^= 1
    print(hashlib.sha256(bytes(data)).hexdigest())
  '';
in
pkgs.testers.runNixOSTest {
  name = "korri-viewer-lifecycle";
  nodes.machine =
    { config, lib, ... }:
    {
      imports = [
        korridModule
        hostModule
      ];
      virtualisation.memorySize = 4096;
      virtualisation.cores = 4;
      virtualisation.diskSize = 8192;
      # Signed closures of all three plugins, for offline seeding.
      virtualisation.additionalPaths = [
        viewerPlugin
        otherContractPlugin
        moonlightPlugin
        (receiptFor otherContractPlugin)
      ];
      hardware.graphics.enable = true;
      fonts.enableDefaultPackages = true;
      # The viewer's audio has no sound server here: ALSA's null device.
      environment.etc."asound.conf".text = "pcm.!default { type null }\n";
      environment.systemPackages = [
        pkgs.acl
        pkgs.curl
        pkgs.jq
        pkgs.python3
        sway
      ];
      users.groups.korri.gid = runtimeUid;
      users.users.korri = {
        isNormalUser = true;
        uid = runtimeUid;
        group = "korri";
      };
      users.groups.sender = { };
      users.users.sender = {
        isSystemUser = true;
        group = "sender";
        home = "/var/lib/sender";
      };
      services.korridLinuxDevice = {
        enable = true;
        uid = korridUid;
        gid = korridUid;
        runtimeUser = "korri";
        inherit runtimeUid;
        runtimeGid = runtimeUid;
        inputdUid = 977;
        controlGid = 977;
        localSignerUid = 978;
        localSignerGid = 978;
        deviceConfig = pkgs.writeText "viewer-lifecycle-host.toml" ''label = "viewer-lifecycle"'';
        relays = [ "ws://127.0.0.1:7447/" ];
        compositorControlSocket = "/run/korri-compositor/sway-ipc.sock";
      };
      systemd.services.korrid.environment.KORRID_SHELL_UNIT = "korri-shell.service";
      services.korri.pluginHost = {
        enable = true;
        package = host;
        publishers."@korri" = binding;
      };
      systemd.tmpfiles.rules = [
        # As korri-linux-host-core.nix makes them on a device.
        "d /var/lib/korri 0700 korrid korrid -"
        # Both setup units write below it, as on a device.
        "d /home/korri/.config 0700 korri korri -"
        # The viewer unit binds the runtime user's Pulse directory and hides
        # its runtime directory; a session would make both.
        "d /run/user/${toString runtimeUid} 0700 korri korri -"
        "d /run/user/${toString runtimeUid}/pulse 0700 korri korri -"
        "d /var/lib/sender 0700 sender sender -"
        "d /run/viewer-test 0755 root root -"
        "d /run/viewer-probe 0755 root root -"
        "p /run/viewer-probe/in 0600 root root -"
      ];
      systemd.services = {
        seed-viewer-plugins = {
          requiredBy = [ "korri-plugin-host.service" ];
          before = [ "korri-plugin-host.service" ];
          serviceConfig.Type = "oneshot";
          script =
            lib.concatMapStrings (plugin: ''
              ${pkgs.nix}/bin/nix --extra-experimental-features nix-command store sign --recursive --key-file ${key} ${plugin.package}
              name="$(${host}/bin/korri-plugin unit-name '${plugin.id}')"
              install -d -m 0700 "/var/lib/korri-plugin-host/$name" "/nix/var/nix/gcroots/korri-plugin-host/$name"
              if ! test -e "/var/lib/korri-plugin-host/$name/selection.json"; then
                install -m 0600 ${receiptFor plugin.package} "/var/lib/korri-plugin-host/$name/selection.json"
                ln -s ${plugin.package} "/nix/var/nix/gcroots/korri-plugin-host/$name/active"
              fi
            '') seeded
            + ''
              ${pkgs.nix}/bin/nix --extra-experimental-features nix-command store sign --recursive --key-file ${key} ${otherContractPlugin}
            '';
        };
        # The device's compositor: Sway with Xwayland on :0, its control
        # socket reachable by korrid's group, as the product makes it.
        korri-compositor = {
          wantedBy = [ "multi-user.target" ];
          after = [ "systemd-tmpfiles-setup.service" ];
          before = [ "korrid.service" ];
          path = [
            pkgs.dbus
            pkgs.xwayland
          ];
          environment = {
            XDG_RUNTIME_DIR = "/run/user/${toString runtimeUid}";
            SWAYSOCK = "/run/korri-compositor/sway-ipc.sock";
            WLR_BACKENDS = "headless";
            WLR_RENDERER = "pixman";
            WLR_LIBINPUT_NO_DEVICES = "1";
          };
          serviceConfig = {
            User = "korri";
            Group = "korri";
            RuntimeDirectory = "korri-compositor";
            RuntimeDirectoryMode = "0750";
            UMask = "0007";
            ExecStart = "${sway}/bin/sway --config ${viewerSway}";
          };
        };
        # A stand-in shell window in its own unit, for korrid's Background.
        korri-shell = {
          after = [ "korri-compositor.service" ];
          path = [ pkgs.coreutils ];
          environment.XDG_RUNTIME_DIR = "/run/user/${toString runtimeUid}";
          serviceConfig = {
            User = "korri";
            Group = "korri";
          };
          script = ''
            export WAYLAND_DISPLAY="$(cd "$XDG_RUNTIME_DIR" && ls wayland-? | head -n1)"
            exec ${pkgs.foot}/bin/foot --app-id korri-shell ${pkgs.coreutils}/bin/sleep infinity
          '';
        };
        sender = {
          # Sway runs each `exec` line with sh.
          path = [
            pkgs.bash
            pkgs.dbus
          ];
          serviceConfig = {
            User = "sender";
            Group = "sender";
            RuntimeDirectory = "sender";
            ExecStart = "${sway}/bin/sway --config ${senderSway}";
          };
          environment = {
            HOME = "/var/lib/sender";
            XDG_RUNTIME_DIR = "/run/sender";
            SWAYSOCK = "/run/sender/sway.sock";
            WLR_BACKENDS = "headless";
            WLR_RENDERER = "pixman";
            WLR_LIBINPUT_NO_DEVICES = "1";
          };
        };
        # korrid's own service, but running the probe: same user, groups,
        # environment and sandbox. Only its runtime and state directories
        # stay with the daemon, which keeps running beside it.
        viewer-probe = {
          after = [
            "korri-plugin-host.service"
            "korri-compositor.service"
          ];
          inherit (config.systemd.services.korrid) environment;
          serviceConfig =
            builtins.removeAttrs config.systemd.services.korrid.serviceConfig [
              "ExecStart"
              "Restart"
              "RestartSec"
              "RuntimeDirectory"
              "RuntimeDirectoryMode"
              "RuntimeDirectoryPreserve"
              "StateDirectory"
              "StateDirectoryMode"
            ]
            // {
              Type = "exec";
              ExecStart = "${probe}/bin/viewer_probe 127.0.0.1:47989 /run/viewer-test/server.pem /run/viewer-test/client.pem ${config.services.korridLinuxDevice.storageRoot}";
              StandardInput = "file:/run/viewer-probe/in";
              StandardOutput = "append:/run/viewer-probe/out";
              StandardError = "journal";
            };
        };
      };
      system.stateVersion = "26.05";
    };
  testScript = ''
    import json

    def manifest(package):
        return json.loads(machine.succeed(f"cat {package}/manifest.json"))

    viewer = manifest("${viewerPlugin}")["files"]
    moonlight = manifest("${moonlightPlugin}")["files"]
    replies = 0

    def ask(action, launch, timeout=120):
        global replies
        machine.succeed(f"timeout 10 sh -c \"echo '{action} {launch}' > /run/viewer-probe/in\"")
        replies += 1
        machine.wait_until_succeeds(f"test $(wc -l < /run/viewer-probe/out) -ge {replies}", timeout=timeout)
        reply = json.loads(machine.succeed(f"sed -n {replies}p /run/viewer-probe/out"))
        print(f"probe {action} {launch}: {reply}")
        assert reply["action"] == action and reply["launch"] == launch, reply
        return reply

    def journal():
        return machine.succeed("journalctl -u viewer-probe.service -o cat")

    def diagnose():
        print(machine.execute("journalctl -o cat -u viewer-probe.service -u 'korri-game-*' -u sender.service -u korri-compositor.service | tail -n 200")[1])
        print(machine.execute("tail -n 80 /var/lib/sender/sunshine.log")[1])

    def ok(action, launch, timeout=120):
        reply = ask(action, launch, timeout)
        if not reply["ok"]:
            diagnose()
        assert reply["ok"], reply
        return reply.get("result")

    moonlight_retries = []

    def stop_moonlight(launch):
        # Moonlight's adapter is unchanged: it reads the unit right after a
        # --no-block stop and can find it still deactivating. A second stop
        # then finds it gone. The test records how often that happened.
        reply = ask("stop", launch)
        if not reply["ok"]:
            assert reply["error"] == "viewer stop was not confirmed", reply
            moonlight_retries.append(launch)
            machine.wait_until_fails(f"systemctl is-active korri-game-{launch}.service")
            ok("stop", launch)

    def tree():
        return json.loads(machine.succeed("swaymsg -s /run/korri-compositor/sway-ipc.sock -t get_tree -r"))

    def focused(node):
        if node.get("focused"):
            return node
        for child in node.get("nodes", []) + node.get("floating_nodes", []):
            found = focused(child)
            if found:
                return found
        return None

    machine.start()
    machine.wait_for_unit("korri-plugin-host.service")
    machine.wait_for_unit("korrid.service")
    machine.wait_for_unit("korri-compositor.service")
    machine.wait_until_succeeds("test -S /run/korri-compositor/sway-ipc.sock && test -S /tmp/.X11-unix/X0")
    machine.succeed("systemctl start korri-shell.service")
    machine.wait_until_succeeds("swaymsg -s /run/korri-compositor/sway-ipc.sock -t get_tree | grep -q korri-shell")

    with subtest("the plugin host installs and enables both viewers"):
        enabled = json.loads(machine.succeed("cat /run/korri-plugin-host/enabled-packages.json"))
        ids = sorted(package["id"] for package in enabled)
        assert ids == ["@korri:moonlight", "@korri:viewer"], ids
        machine.wait_for_unit("korri-viewer-setup.service")
        machine.succeed("test \"$(stat -c '%U %a' /home/korri/.config/moonlight)\" = 'korri 700'")

    with subtest("a sender pairs the viewer's own bootstrap identity"):
        machine.succeed(
            f"runuser -u korri -- {viewer['identity-bootstrap']} {viewer['viewer']} /home/korri/.config/moonlight > /run/viewer-test/client.pem"
        )
        machine.succeed("python3 ${pairing} /run/viewer-test/client.pem /var/lib/sender/state.json")
        machine.succeed("install -m 0644 ${sunshineConfig} /var/lib/sender/sunshine.conf")
        machine.succeed("install -m 0644 ${apps} /var/lib/sender/apps.json")
        machine.succeed("chown sender:sender /var/lib/sender/*")
        machine.succeed("systemctl start sender.service")
        try:
            machine.wait_until_succeeds("curl -fsS http://127.0.0.1:47989/serverinfo | grep -q '<uniqueid>${hostUuid}</uniqueid>'", timeout=180)
        except Exception:
            diagnose()
            raise
        machine.succeed("install -m 0644 /var/lib/sender/sunshine.crt /run/viewer-test/server.pem")

    # One writer holds the probe's input open, so it never reads an end.
    machine.succeed("systemd-run --unit=probe-input ${pkgs.bash}/bin/sh -c 'exec ${pkgs.coreutils}/bin/sleep infinity > /run/viewer-probe/in'")
    machine.succeed("systemctl start viewer-probe.service")
    korri_launch = "0123456789abcdef0123456789abcdef"
    unit = f"korri-game-{korri_launch}.service"
    runtime = f"/run/korri-game-{korri_launch}"
    socket = f"{runtime}/control/viewer.sock"

    with subtest("korrid starts @korri:viewer and talks to it through its socket"):
        ok("start", korri_launch, timeout=180)
        assert f"launch {korri_launch} uses @korri:viewer" in journal()
        command = machine.succeed(f"systemctl show {unit} -p ExecStart --value")
        print(f"viewer unit command: {command}")
        assert viewer["viewer"] in command, command
        for argument in (f"--lifecycle-socket {socket}", f"--launch-id {korri_launch}", "--controller-uid ${toString korridUid}", "-viewonly"):
            assert argument in command, (argument, command)
        main = machine.succeed(f"systemctl show {unit} -p MainPID --value").strip()
        assert machine.succeed(f"stat -c %u /proc/{main}").strip() == "${toString runtimeUid}"
        assert machine.succeed(f"systemctl show {unit} -p RuntimeDirectory --value").strip() == f"korri-game-{korri_launch}"
        assert machine.succeed(f"systemctl show {unit} -p RuntimeDirectoryMode --value").strip() == "0700"
        # Mode 0700 plus a search-only grant for korrid: the group class
        # shows the ACL mask, and the owning group itself has nothing.
        acl = machine.succeed(f"getfacl -n -p {runtime}")
        print(f"runtime directory: {acl}")
        for entry in ("# owner: ${toString runtimeUid}", "user::rwx", "user:${toString korridUid}:--x", "group::---", "mask::--x", "other::---"):
            assert entry in acl.splitlines(), (entry, acl)
        socket_acl = machine.succeed(f"getfacl -n -p {socket}")
        print(f"lifecycle socket: {socket_acl}")
        assert "user:${toString korridUid}:rw-" in socket_acl.splitlines(), socket_acl
        assert "other::---" in socket_acl.splitlines(), socket_acl
        # The server certificate arrives as Moonlight's does: a unit credential.
        machine.succeed(f"cmp /run/credentials/{unit}/moonlight-server.pem /run/viewer-test/server.pem")
        environment = machine.succeed(f"systemctl show {unit} -p Environment --value")
        assert "KORRI_MOONLIGHT_SERVER_CERTIFICATE=/run/credentials/" + unit + "/moonlight-server.pem" in environment
        # The unit names its viewer, for a restarted korrid.
        assert "KORRI_LIVE_RUNNER_ID=@korri:viewer/viewer" in environment, environment
        # korrid focused the viewer's window after Start was confirmed.
        window = focused(tree())
        assert window and window.get("app_id") != "korri-shell" and window.get("fullscreen_mode") == 1, window

    with subtest("korrid observes the stream only through the socket"):
        assert ok("status", korri_launch) == "Running"
        snapshot = ok("snapshot", korri_launch)
        assert snapshot["connected"] is True, snapshot
        shape = snapshot["shape"]
        assert shape["screen"] == {"width": 1280, "height": 720}, shape
        assert isinstance(shape["connection"], str) and shape["connection"], shape
        ok("background", korri_launch)
        assert ok("status", korri_launch) == "Backgrounded"
        assert focused(tree()).get("app_id") == "korri-shell"
        ok("resume", korri_launch)
        assert ok("status", korri_launch) == "Running"
        assert focused(tree()).get("app_id") != "korri-shell"

    with subtest("korrid stops the viewer and its unit and directory are gone"):
        ok("stop", korri_launch)
        machine.fail(f"systemctl is-active {unit}")
        machine.fail(f"test -e {runtime}")
        assert ok("status", korri_launch) == "Stopped"
        assert focused(tree()) is None or focused(tree()).get("app_id") == "korri-shell"

    with subtest("with @korri:viewer disabled, the same selection gives Moonlight"):
        machine.succeed("korri-plugin disable @korri:viewer")
        moonlight_launch = "11111111111111111111111111111111"
        moonlight_unit = f"korri-game-{moonlight_launch}.service"
        ok("start", moonlight_launch, timeout=180)
        assert f"launch {moonlight_launch} uses @korri:moonlight" in journal()
        command = machine.succeed(f"systemctl show {moonlight_unit} -p ExecStart --value")
        assert moonlight["moonlight"] in command and "--lifecycle-socket" not in command, command
        assert machine.succeed(f"systemctl show {moonlight_unit} -p RuntimeDirectory --value").strip() == ""
        assert ok("status", moonlight_launch) == "Running"
        assert "KORRI_LIVE_RUNNER_ID" not in machine.succeed(f"systemctl show {moonlight_unit} -p Environment --value")

    with subtest("after a korrid restart the running Moonlight unit, not the new choice, answers"):
        # Both plugins are enabled again, so a start would now choose
        # @korri:viewer, and a new korrid process knows no launch.
        machine.succeed("korri-plugin enable @korri:viewer")
        machine.succeed("systemctl restart viewer-probe.service")
        machine.wait_for_unit("viewer-probe.service")
        assert ok("status", moonlight_launch) == "Running"
        stop_moonlight(moonlight_launch)
        machine.fail(f"systemctl is-active {moonlight_unit}")
        machine.succeed("korri-plugin disable @korri:viewer")

    with subtest("a viewer built from another contract is refused, and Moonlight stays"):
        name = machine.succeed("korri-plugin unit-name @korri:viewer").strip()
        machine.succeed("install -m 0600 ${receiptFor otherContractPlugin} /var/lib/korri-plugin-host/" + name + "/selection.json")
        machine.succeed("ln -sfn ${otherContractPlugin} /nix/var/nix/gcroots/korri-plugin-host/" + name + "/active")
        # The seeded receipt is Enabled; restore-all applies it as at boot.
        machine.succeed("korri-plugin restore-all")
        enabled = json.loads(machine.succeed("cat /run/korri-plugin-host/enabled-packages.json"))
        other = [package for package in enabled if package["id"] == "@korri:viewer"]
        assert other and other[0]["package"] == "${otherContractPlugin}", enabled
        theirs = machine.succeed(f"python3 ${flipped} {viewer['treaty']}").strip()
        assert machine.succeed(f"sha256sum < {other[0]['files']['treaty']}").split()[0] == theirs
        ours = machine.succeed(f"sha256sum < {viewer['treaty']}").split()[0]
        refused_launch = "22222222222222222222222222222222"
        ok("start", refused_launch, timeout=180)
        log = journal()
        print(log)
        refusal = f"korrid refuses @korri:viewer: it was built from a different viewer contract (its treaty is sha256 {theirs}, korrid's is sha256 {ours})"
        assert refusal in log, log
        assert f"launch {refused_launch} uses @korri:moonlight" in log
        stop_moonlight(refused_launch)
        # Without Moonlight the refusal is the start's own error.
        machine.succeed("korri-plugin disable @korri:moonlight")
        reply = ask("start", "33333333333333333333333333333333")
        assert not reply["ok"] and refusal in reply["error"], reply
        machine.fail("systemctl is-active korri-game-33333333333333333333333333333333.service")
    print(f"Moonlight stops that needed a second request: {moonlight_retries}")
  '';
}
