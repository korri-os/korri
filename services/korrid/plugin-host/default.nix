{
  pkgs,
  crane,
  hostModule,
}:
let
  hostPackage = import ./package.nix { inherit pkgs crane; };
  tailscalePackage = import ../../../plugins/tailscale/package.nix { inherit pkgs; };
in
{
  packages = {
    korri-plugin-host = hostPackage;
    korri-tailscale = tailscalePackage;
  };
  checks = {
    korri-plugin-host = hostPackage;
    korri-runtime-plugin-host = import ./vm-test.nix {
      inherit
        pkgs
        hostModule
        hostPackage
        tailscalePackage
        ;
    };
  };
  apps = {
    korri-plugin = {
      type = "app";
      program = "${hostPackage}/bin/korri-plugin";
    };
    korri-plugin-check = {
      type = "app";
      program = "${
        pkgs.writeShellApplication {
          name = "korri-plugin-check";
          runtimeInputs = [
            pkgs.git
            pkgs.nix
          ];
          text = ''
            root="$(git rev-parse --show-toplevel)"
            cd "$root"
            nix develop .#plugin-host --command cargo fmt --manifest-path services/korrid/plugin-host/Cargo.toml -- --check
            nix develop .#plugin-host --command cargo clippy --manifest-path services/korrid/plugin-host/Cargo.toml --all-targets -- -D warnings
            nix build --no-link .#checks.${pkgs.stdenv.hostPlatform.system}.korri-plugin-host .#checks.${pkgs.stdenv.hostPlatform.system}.korri-runtime-plugin-host
          '';
        }
      }/bin/korri-plugin-check";
    };
  };
  help = ''
    nix run .#korri-plugin -- COMMAND
        Manage independently packaged Linux plugins with explicit administrator approval.
    nix run .#korri-plugin-check
        Check plugin-host Rust code and the cold-host systemd VM lifecycle.
  '';
  devShell = pkgs.mkShell {
    packages = [
      pkgs.rust-bin.stable.latest.default
      pkgs.nixfmt-rfc-style
    ];
  };
}
