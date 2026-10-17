# A game unit's PrivatePIDs=yes makes its main process the PID namespace init,
# which ignores signals it has no handler for. Check that a game without a
# SIGTERM handler still stops on systemd's first SIGTERM when launched through
# `korrid plugin-launch`, and that the init keeps the game's exit status.
{ pkgs, korrid }:
let
  # The smallest packaged plugin: run the program with the content as a script.
  plugin = pkgs.runCommand "game-stop-plugin" { } ''
    mkdir -p $out
    cat > $out/plugin.ts <<'EOF'
    export const name = "game-stop"
    export const handlers = {
      "launch.prepare": (input: { program: string; contentPath: string }) => ({
        command: input.program,
        args: ["-c", input.contentPath],
        env: {},
      }),
    }
    EOF
    echo '{"entry":"plugin.ts","sources":["plugin.ts"]}' > $out/manifest.json
  '';
  input =
    script:
    builtins.toJSON {
      runnerId = "@test:game-stop/sh";
      program = "${pkgs.bash}/bin/sh";
      contentPath = script;
      accountRoot = "/tmp";
      files = { };
    };
  # The production game unit's process and stop properties.
  unit =
    name:
    "systemd-run --unit=${name} --uid=1000 --gid=1000 --service-type=exec -p PrivatePIDs=yes -p KillMode=control-group -p TimeoutStopSec=8";
in
pkgs.testers.runNixOSTest {
  name = "korrid-game-stop";
  nodes.machine = {
    users.users.korri = {
      isNormalUser = true;
      uid = 1000;
      group = "korri";
    };
    users.groups.korri.gid = 1000;
  };
  testScript = ''
    import shlex
    import time

    def stop_seconds(name):
        start = time.monotonic()
        machine.succeed(f"systemctl stop {name}")
        return time.monotonic() - start

    machine.wait_for_unit("multi-user.target")

    with subtest("control: an exec'd game without a SIGTERM handler waits for the timeout"):
        machine.succeed("${unit "bare"} ${pkgs.coreutils}/bin/sleep 1000")
        machine.wait_until_succeeds("systemctl is-active bare")
        elapsed = stop_seconds("bare")
        result = machine.succeed("systemctl show bare -p Result --value").strip()
        print(f"bare stop: {elapsed:.1f} s, result {result}")
        assert elapsed >= 7 and result == "timeout", (elapsed, result)

    with subtest("plugin-launch: the same game stops on the first SIGTERM"):
        script = "(${pkgs.coreutils}/bin/sleep 0.2 &); exec ${pkgs.coreutils}/bin/sleep 1000"
        machine.succeed(
            "${unit "game"} ${korrid}/bin/korrid plugin-launch ${plugin}/plugin.ts "
            + shlex.quote(${builtins.toJSON (input "SCRIPT")}.replace("SCRIPT", script))
            + " " + shlex.quote('{"modifiers":[]}')
        )
        machine.wait_until_succeeds("systemctl is-active game")
        machine.wait_until_succeeds("pgrep -f '^/nix/store/[^ ]*/bin/sleep 1000$'", timeout=30)
        time.sleep(1)
        main = machine.succeed("systemctl show game -p MainPID --value").strip()
        print(machine.succeed("systemd-cgls --no-pager -u game.service"))
        tree = machine.succeed("ps -eo pid=,ppid=,stat=,args=")
        children = [line.split(None, 3) for line in tree.splitlines() if line.split()[1] == main]
        print(f"init {main} children: {children}")
        # korrid stays PID 1; the game is its child and the finished orphan is reaped.
        assert "korrid plugin-launch" in machine.succeed(f"cat /proc/{main}/cmdline | tr '\\0' ' '")
        assert [c for c in children if c[3].endswith("sleep 1000")], children
        assert not [c for c in children if c[2].startswith("Z")], children
        elapsed = stop_seconds("game")
        result = machine.succeed("systemctl show game -p Result --value").strip()
        print(f"game stop: {elapsed:.1f} s, result {result}")
        assert elapsed < 4 and result != "timeout", (elapsed, result)
        machine.fail("pgrep -f '^/nix/store/[^ ]*/bin/sleep 1000$'")

    with subtest("plugin-launch: the unit reports the game's exit status"):
        machine.succeed(
            "${unit "status"} ${korrid}/bin/korrid plugin-launch ${plugin}/plugin.ts "
            + shlex.quote(${builtins.toJSON (input "exit 7")})
            + " " + shlex.quote('{"modifiers":[]}')
        )
        machine.wait_until_succeeds("test \"$(systemctl show status -p ExecMainStatus --value)\" = 7")
  '';
}
