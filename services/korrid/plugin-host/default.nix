{
  pkgs,
  crane,
  hostModule,
  korridPackage,
  inputdPackage,
  sunshinePackage,
}:
let
  hostPackage = import ./package.nix { inherit pkgs crane; };
  vmHostPackage = import ./package.nix {
    inherit pkgs crane;
    cargoFeatures = [ "vm-lifecycle-policy" ];
  };
  tailscalePackage = import ./tests/fixtures/tailscale/package.nix { inherit pkgs; };
  mkPlugin = import ./builder.nix { inherit pkgs; };
  firstPartyPlugin =
    name:
    mkPlugin {
      publisher.namespace = "@korri";
      source = ../../../plugins/${name};
      plugin = ../../../plugins/${name}/plugin.nix;
    };
  sunshinePlugin = mkPlugin {
    publisher.namespace = "@korri";
    source = ../../../plugins/sunshine;
    plugin = { pkgs }:
      import ../../../plugins/sunshine/plugin.nix {
        inherit pkgs inputdPackage sunshinePackage;
      };
  };
in
{
  lib = {
    inherit pkgs mkPlugin;
  };
  packages = {
    korri-plugin-host = hostPackage;
    korri-plugin-ssh = firstPartyPlugin "ssh";
    korri-plugin-sunshine = sunshinePlugin;
    # Hand-written, no family and no shared helper. It is built by the same
    # builder as a generated core and admitted by the same rules.
    korri-plugin-ppsspp = firstPartyPlugin "ppsspp";
  };
  checks = {
    korri-plugin-host = hostPackage;
    korri-ssh-upstream = (import ../../../plugins/ssh/upstream.nix { inherit pkgs; }).report;
    korri-ssh-host-support = import ./ssh-support-check.nix { inherit pkgs hostModule hostPackage; };
    korri-plugin-builder = import ./builder-check.nix { inherit pkgs; };
    korri-sunshine-plugin = sunshinePlugin;
    korri-sunshine-plugin-admission = pkgs.runCommand "korri-sunshine-plugin-admission" {
      nativeBuildInputs = [ pkgs.jq ];
    } ''
      ${hostPackage}/bin/korri-plugin seed ${sunshinePlugin} https://cache.example.invalid > receipt.json
      jq -e '.id == "@korri:sunshine" and .desired.state == "Enabled" and .previous == null' receipt.json
      touch "$out"
    '';
    korri-plugin-image-seed = import ./image-seed-check.nix {
      inherit pkgs hostPackage;
      sshPackage = firstPartyPlugin "ssh";
    };
    korri-runtime-plugin-host = import ./vm-test.nix {
      inherit
        pkgs
        hostModule
        hostPackage
        vmHostPackage
        korridPackage
        tailscalePackage
        ;
      sshPackage = firstPartyPlugin "ssh";
    };
  };
  apps = {
    korri-plugin = {
      type = "app";
      program = "${hostPackage}/bin/korri-plugin";
    };
    # Build-side publisher: converts a plugin package to a CA closure archive
    # and emits the catalog record. Does not require administrator privileges.
    korri-publish = {
      type = "app";
      program = "${hostPackage}/bin/korri-publish";
    };
    # CA conversion writes the builder's Nix store. Keep it outside a pure
    # derivation, while ordinary Rust and HTTPS tests stay in hostPackage.
    korri-publisher-check = {
      type = "app";
      program = "${
        pkgs.writeShellApplication {
          name = "korri-publisher-check";
          runtimeInputs = [
            pkgs.coreutils
            pkgs.nix
            pkgs.rust-bin.stable.latest.default
            pkgs.stdenv.cc
            pkgs.stdenv.cc.bintools
          ];
          text = ''
            if (( $# > 1 )); then
              echo 'usage: korri-publisher-check [STORE_PACKAGE]' >&2
              exit 2
            fi
            package="''${1:-${tailscalePackage}}"
            if [[ "$package" != /nix/store/* || ! -f "$package/plugin.ts" ]]; then
              echo 'STORE_PACKAGE must be an existing /nix/store package with plugin.ts' >&2
              exit 2
            fi
            workspace="$(mktemp -d -t korri-publisher-check.XXXXXXXX)"
            trap 'rm -rf "$workspace"' EXIT
            trap 'exit 130' INT
            trap 'exit 143' TERM
            # Use the same immutable source as the host build, including the
            # shared ../../src/script.rs. Tests need a writable crate cwd too.
            cp -R ${hostPackage.src}/. "$workspace/"
            chmod -R u+w "$workspace"
            cd "$workspace/plugin-host"
            export CARGO_TARGET_DIR="$workspace/target"
            export KORRI_PUBLISH_NIX=${pkgs.nix}/bin/nix
            export KORRI_PUBLISH_TEST_PACKAGE="$package"
            export KORRI_PUBLISH_TEST_SYSTEM=${pkgs.stdenv.hostPlatform.system}
            cargo test --locked --manifest-path Cargo.toml --test publish --test namespace -- --ignored
          '';
        }
      }/bin/korri-publisher-check";
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
    nix run .#korri-publish -- STORE_PATH RELEASE_VERSION PLATFORM ARCHIVE_URL OUTPUT_DIR
        Convert a built plugin package to a content-addressed archive and emit a catalog record.
    nix run .#korri-publish -- archive-name STORE_PATH RELEASE PLATFORM
        Derive the upload name from the actual plugin declaration and publisher naming contract.
    nix run .#korri-publish -- catalog RECORD_FILE...
        Merge records through the same strict Rust contract used by devices.
    nix run .#korri-publish -- verify-release CATALOG ASSET_DIR BASE_URL ID RELEASE PLATFORM...
        Verify the complete platform set, upload URLs, sizes and archive hashes.
    nix run .#korri-publisher-check -- [STORE_PACKAGE]
        Test the actual Tailscale package (default: core test fixture) with pinned core and toolchain on a build machine.
    nix run .#korri-plugin-check
        Check plugin-host Rust code and the cold-host systemd VM lifecycle.
  '';
  devShell = pkgs.mkShell {
    KORRI_TEST_CURL = "${pkgs.curl}/bin/curl";
    packages = [
      pkgs.rust-bin.stable.latest.default
      pkgs.nixfmt-rfc-style
    ];
  };
}
