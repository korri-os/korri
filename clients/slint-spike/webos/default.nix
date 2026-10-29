# Native IPK assembly only. Cross compilation is composed in ../webos.nix.
{ pkgs }:
let
  check = pkgs.writeShellApplication {
    name = "slint-spike-webos-package-check";
    runtimeInputs = [
      pkgs.python3
      pkgs.binutils
      pkgs.coreutils
    ];
    text = ''
      root="''${KORRI_ROOT:-$PWD}"
      while [[ "$root" != / && ! -f "$root/clients/slint-spike/Cargo.toml" ]]; do
        root="$(dirname "$root")"
      done
      exec python3 "$root/clients/slint-spike/webos/package_test.py" "$@"
    '';
  };
  package = pkgs.writeShellApplication {
    name = "slint-spike-webos-package";
    runtimeInputs = [
      pkgs.python3
      pkgs.binutils
      pkgs.git
      pkgs.coreutils
    ];
    text = ''
      root="''${KORRI_ROOT:-$PWD}"
      while [[ "$root" != / && ! -f "$root/clients/slint-spike/Cargo.toml" ]]; do
        root="$(dirname "$root")"
      done
      if [[ ! -f "$root/clients/slint-spike/Cargo.toml" ]]; then
        echo "Run inside the native spike checkout" >&2
        exit 1
      fi
      exec python3 "$root/clients/slint-spike/webos/package.py" "$@"
    '';
  };
in
{
  apps.slint-spike-webos-package-check = {
    type = "app";
    program = "${check}/bin/slint-spike-webos-package-check";
  };
  apps.slint-spike-webos-package = {
    type = "app";
    program = "${package}/bin/slint-spike-webos-package";
  };
  help = ''

    nix run .#slint-spike-webos-package-check
        Check native archive headers and odd-sized member padding.
    nix run .#slint-spike-webos-package -- BINARY OUTPUT --source-sha SHA --build-record RELEASE_DIR
        Package the ARM spike only when its source, build checksum and TV ABI match.
  '';
}
