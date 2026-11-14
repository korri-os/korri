{
  pkgs,
  crane,
  proseql,
  buildIdentity ? null,
}:
let
  lib = pkgs.lib;
  craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.stable.latest.default;
  proseqlSource = import ../../services/korrid/proseql-source.nix { inherit pkgs proseql; };
  root = ../..;
  cleanSource = lib.cleanSourceWith {
    src = root;
    filter =
      path: type:
      let
        relative = lib.removePrefix "${toString root}/" (toString path);
      in
      toString path == toString root
      || builtins.elem relative [
        "clients"
        "clients/cli"
        "services"
        "services/korrid"
        "packages"
        "packages/input-trace"
        "contracts"
        "contracts/settings"
        "contracts/input"
        "contracts/surface"
        "contracts/viewer"
        "contracts/generated"
        "contracts/generated/korrid.ts"
        # korrid.ts imports the surface treaty's types from ./surface.
        "contracts/generated/surface.ts"
        "contracts/generated/settings.ts"
        # korrid's selection.rs compares @korri:viewer's treaty with this one.
        "contracts/generated/viewer.ts"
      ]
      || (
        (
          lib.hasPrefix "packages/input-trace/" relative
          || lib.hasPrefix "clients/cli/" relative
          || lib.hasPrefix "services/korrid/" relative
          || lib.hasPrefix "contracts/settings/" relative
          || lib.hasPrefix "contracts/input/" relative
          # korrid's path dependencies on the surface and viewer treaty crates.
          || lib.hasPrefix "contracts/surface/" relative
          || lib.hasPrefix "contracts/viewer/" relative
        )
        && craneLib.filterCargoSources path type
      )
      || builtins.elem relative [
        "clients/cli/tests/settings-terminal.py"
        "clients/cli/tests/plugin-terminal.py"
        "clients/cli/tests/play-library-terminal.py"
        "services/korrid/src/script/completion.js"
        "services/korrid/src/script/code_generation.js"
      ]
      # korrid's rlib exposes its real fixture registry to integration tests.
      || lib.hasPrefix "services/korrid/examples/" relative
      || lib.hasPrefix "services/korrid/tests/fixtures/first-party/" relative;
  };
  common = {
    pname = "korri";
    version = "0.0.0";
    src = proseqlSource.composeCargoSource cleanSource;
    postUnpack = "sourceRoot+=/clients/cli";
    cargoVendorDir = craneLib.vendorCargoDeps {
      src = cleanSource;
      cargoLock = ./Cargo.lock;
    };
    strictDeps = true;
    nativeBuildInputs = [
      pkgs.clang
      pkgs.llvmPackages.libclang
    ];
    LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
  };
  cargoArtifacts = craneLib.buildDepsOnly (
    (builtins.removeAttrs common [ "src" ])
    // {
      dummySrc = craneLib.mkDummySrc {
        src = cleanSource;
        cargoLock = ./Cargo.lock;
        extraDummyScript = ''
          mv "$out/Cargo.lock" "$out/clients/cli/Cargo.lock"
          ${proseqlSource.dummySourceScript}
        '';
      };
    }
  );
in
craneLib.buildPackage (
  common
  // {
    inherit cargoArtifacts;
    cargoBuildExtraArgs = "--bin korri";
    nativeCheckInputs = [
      pkgs.python3
      pkgs.typescript
    ];
    meta.mainProgram = "korri";
  }
  # Only the final binary carries source identity. Dependencies must stay
  # substitutable across revisions with the same dependency inputs.
  // lib.optionalAttrs (buildIdentity != null) { KORRI_BUILD_ID = buildIdentity; }
)
