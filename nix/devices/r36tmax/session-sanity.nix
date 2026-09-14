{ pkgs }:
pkgs.writeShellApplication {
  name = "r36tmax-session-sanity";
  runtimeInputs = [
    pkgs.coreutils
    pkgs.curl
    pkgs.gnugrep
    pkgs.procps
    pkgs.systemd
  ];
  text = ''
    if [ "$#" -ne 0 ]; then
      echo "usage: r36tmax-session-sanity (read-only, at most 47 seconds)" >&2
      exit 2
    fi
    exec timeout --signal=TERM --kill-after=2s 45s ${pkgs.bash}/bin/bash ${./session-sanity.sh}
  '';
}
