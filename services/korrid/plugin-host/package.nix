{ pkgs, crane }:
let
  craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.stable.latest.default;
  # PPSSPP remains a relative symlink into its plugin package. Nix cannot
  # retain a symlink that escapes this source root, so the source derivation
  # below materializes it. RetroArch is now a core-owned test fixture.
  linkedDeclarations = [ "ppsspp" ];
  clean = pkgs.lib.cleanSourceWith {
    src = ./.;
    filter =
      path: type:
      craneLib.filterCargoSources path type
      || path == toString ./tests/fixtures/selection.json
      || path == toString ./tests/fixtures/retroarch.plugin.ts
      || path == toString ./tests/fixtures/streaming-host
      || pkgs.lib.hasPrefix "${toString ./tests/fixtures/streaming-host}/" path
      || builtins.elem path (
        map (name: toString (./tests/fixtures + "/${name}.plugin.ts")) linkedDeclarations
      );
  };
  vendor = craneLib.vendorCargoDeps { src = clean; };
  common = {
    pname = "korri-plugin-host";
    version = "0.0.0";
    strictDeps = true;
    cargoVendorDir = vendor;
  };
  artifacts = craneLib.buildDepsOnly (common // { src = clean; });
  # The separate CLI crate uses korrid's actual evaluator. Materialize that
  # source, its example, and the linked declaration used by admission tests.
  source = pkgs.runCommand "korri-plugin-host-source" { } ''
    mkdir -p "$out/src" "$out/examples"
    cp -R ${clean} "$out/plugin-host"
    chmod -R u+w "$out/plugin-host"
    cp ${../src/script.rs} "$out/src/script.rs"
    cp -R ${../src/script} "$out/src/script"
    cp ${../src/plugin_installation.rs} "$out/src/plugin_installation.rs"
    cp ${../src/plugin_references.rs} "$out/src/plugin_references.rs"
    cp ${../examples/catalog.plugin.ts} "$out/examples/catalog.plugin.ts"
    rm -f "$out/plugin-host/tests/fixtures/ppsspp.plugin.ts"
    cp ${../../../plugins/ppsspp/plugin.ts} "$out/plugin-host/tests/fixtures/ppsspp.plugin.ts"
  '';
in
craneLib.buildPackage (
  common
  // {
    src = source;
    cargoArtifacts = artifacts;
    postUnpack = ''sourceRoot="$sourceRoot/plugin-host"'';
    nativeBuildInputs = [ pkgs.makeWrapper ];
    KORRI_TEST_CURL = "${pkgs.curl}/bin/curl";
    postInstall = ''
      wrapProgram "$out/bin/korri-plugin" \
        --set KORRI_PLUGIN_NIX ${pkgs.nix}/bin/nix \
        --set KORRI_PLUGIN_SYSTEMCTL ${pkgs.systemd}/bin/systemctl \
        --set KORRI_PLUGIN_IPTABLES ${pkgs.iptables}/bin/iptables \
        --set KORRI_PLUGIN_IP6TABLES ${pkgs.iptables}/bin/ip6tables \
        --set KORRI_PLUGIN_CURL ${pkgs.curl}/bin/curl

      # korri-publish is build-side only; it does not need systemctl.
      # The Nix binary is injected so publication does not require an ambient
      # nix on PATH, matching the same isolation used by korri-plugin.
      wrapProgram "$out/bin/korri-publish" \
        --set KORRI_PUBLISH_NIX ${pkgs.nix}/bin/nix
    '';
    meta.mainProgram = "korri-plugin";
  }
)
