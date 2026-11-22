# Exercise deploy ordering and bundle verification without a target device.
{ pkgs }:
pkgs.runCommand "tabs6-deploy-check"
  {
    nativeBuildInputs = [
      pkgs.python3
      pkgs.bash
      pkgs.coreutils
      pkgs.shellcheck
    ];
  }
  ''
    # The wrapper supplies these variables; the standalone source does not.
    shellcheck -s bash -e SC2154 ${./deploy-over-ssh.sh}
    python3 ${./deploy-over-ssh.test.py} ${./deploy-over-ssh.sh}
    touch $out
  ''
