# Native IPK assembly only. Cross compilation is composed in ../webos.nix.
{ pkgs }:
let
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
  apps.slint-spike-webos-package = {
    type = "app";
    program = "${package}/bin/slint-spike-webos-package";
  };
  help = ''

    nix run .#slint-spike-webos-package -- BINARY OUTPUT --source-sha SHA
        Package the tested ARM native spike, enforcing the measured TV ABI.
  '';
}
