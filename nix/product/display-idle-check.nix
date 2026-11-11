# Exercise the actual generated idle helper, without a compositor or product
# plugin packages. The subprocesses expose controlled RPC and native outcomes.
{ pkgs }:
let
  curl = pkgs.writeShellScriptBin "curl" ''
    printf 'rpc\n' >> "$attempts"
    case "$behavior" in
      success) printf '{"outcome":{"_tag":"Ok"}}';;
      refused) printf '{"outcome":{"_tag":"Err"}}';;
      disconnected) exit 7;;
    esac
  '';
  swaymsg = pkgs.writeShellScriptBin "swaymsg" ''
    printf '%s\n' "$*" >> "$notifications"
  '';
  swayidle = pkgs.writeShellScriptBin "swayidle" ''
    printf '%s\n' "$@" > "$arguments"
  '';
  controls = import ./controls.nix {
    inherit (pkgs) lib;
    pkgs = pkgs // {
      inherit curl swayidle;
      sway-unwrapped = swaymsg;
    };
    config.services = {
      korriLinuxHost.runtimeUid = 1000;
      korridLinuxDevice.cliSocket = "/unused-test-cli.sock";
    };
  };
in
pkgs.runCommand "korri-display-idle-check" { } ''
  export arguments="$TMPDIR/arguments" attempts="$TMPDIR/attempts" notifications="$TMPDIR/notifications"
  ${controls.systemd.services.korri-display-idle.serviceConfig.ExecStart}
  # Native swayidle receives the timeout helper as its fourth argument.
  test "$(sed -n '2p' "$arguments")" = timeout
  test "$(sed -n '3p' "$arguments")" = 300
  idle=$(sed -n '4p' "$arguments")
  for behavior in success refused disconnected; do
    export behavior
    : > "$attempts"
    : > "$notifications"
    if "$idle"; then
      test "$behavior" = success
    else
      test "$behavior" != success
    fi
    # Never loop or retry blindly; the single RPC has its existing 10 s bound.
    test "$(wc -l < "$attempts")" -eq 1
    if test "$behavior" = success; then
      test ! -s "$notifications"
    else
      # Re-arm only. Never fall back to power-off of a surviving capture.
      test "$(cat "$notifications")" = 'seat * idle_notify' || {
        echo "idle RPC failure must re-arm the native timer ($behavior)" >&2
        exit 1
      }
    fi
  done
  touch "$out"
''
