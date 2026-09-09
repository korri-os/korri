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
    korri-publication-workflow =
      pkgs.runCommand "korri-publication-workflow-check"
        {
          nativeBuildInputs = [
            (pkgs.python3.withPackages (p: [ p.pyyaml ]))
            pkgs.actionlint
          ];
        }
        ''
          python3 ${./publication_test.py} ${./publication.py} ${../../../.github/workflows/plugin-repository.yml}
          actionlint ${../../../.github/workflows/plugin-repository.yml}
          touch "$out"
        '';
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
            pkgs.git
            pkgs.nix
          ];
          text = ''
            root="$(git rev-parse --show-toplevel)"
            cd "$root"
            export KORRI_PUBLISH_NIX=${pkgs.nix}/bin/nix
            export KORRI_PUBLISH_TEST_PACKAGE=${tailscalePackage}
            export KORRI_PUBLISH_TEST_SYSTEM=${pkgs.stdenv.hostPlatform.system}
            nix develop .#plugin-host --command cargo test --manifest-path services/korrid/plugin-host/Cargo.toml --test publish -- --ignored
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
    nix run .#korri-publisher-check
        Run opt-in actual publisher tests using locked Nix and Tailscale in a writable builder store.
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
