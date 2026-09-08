{ pkgs, tailscale }:
pkgs.runCommand "korri-tailscale-package-check" { } ''
  test -x ${tailscale}/bin/tailscale
  test -x ${tailscale}/bin/tailscaled

  ${tailscale}/bin/tailscale version > cli-version
  ${tailscale}/bin/tailscaled --version > daemon-version
  test "$(head -n 1 cli-version)" = ${pkgs.lib.escapeShellArg tailscale.version}
  test "$(head -n 1 daemon-version)" = ${pkgs.lib.escapeShellArg tailscale.version}

  # Never consult the build machine's running daemon or join a tailnet.
  if ${tailscale}/bin/tailscale --socket="$TMPDIR/not-running.sock" status --json > status 2> error; then
    echo "tailscale status succeeded without a daemon" >&2
    exit 1
  fi
  test -s error

  mkdir -p "$out"
  cp cli-version daemon-version "$out/"
''
