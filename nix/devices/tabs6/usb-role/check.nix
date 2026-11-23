# tabs6-usb-role against a temporary sysfs tree and SM5705 register file
# (usb-role.test.py), plus shellcheck. No hardware is touched.
{ pkgs }:
pkgs.runCommand "tabs6-usb-role-check"
  {
    nativeBuildInputs = [
      pkgs.python3
      pkgs.bash
      pkgs.coreutils
      pkgs.shellcheck
      pkgs.rustc
      pkgs.stdenv.cc
      pkgs.rustfmt
      pkgs.util-linux
    ];
  }
  ''
    shellcheck -s bash ${./tabs6-usb-role.sh} ${../bluetooth/tabs6-bt-pair.sh}
    cp ${./.}/*.rs ${./.}/phase-a-transition.txt .
    rustfmt --edition 2021 --check typec-status.rs typec-auto.rs typec.rs
    rustc --edition=2021 -D warnings --test typec-status.rs -o typec-status-tests
    ./typec-status-tests
    rustc --edition=2021 -D warnings typec-status.rs -o typec-status
    rustc --edition=2021 -D warnings --test typec-auto.rs -o typec-auto-tests
    ./typec-auto-tests
    rustc --edition=2021 -D warnings typec-auto.rs -o tabs6-typec-auto
    export TABS6_TEST_AUTO="$PWD/tabs6-typec-auto"
    export TABS6_TEST_TRACE=${./phase-a-transition.txt}
    python3 ${./usb-role.test.py} ${./tabs6-usb-role.sh}
    # Arguments must never open an alternate register/firmware path.
    if ./typec-status --write 2>error; then exit 1; fi
    grep -q 'read-only; no arguments' error
    touch $out
  ''
