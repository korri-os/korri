{
  pkgs,
  crane,
  proseql,
}:
let
  lib = pkgs.lib;
  craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.stable.latest.default;
  proseqlSource = import ./proseql-source.nix { inherit pkgs proseql; };
  sourceRoot = ./.;
  sourceRootString = toString sourceRoot;
  relativeSourcePath = path: lib.removePrefix "${sourceRootString}/" (toString path);
  cleanSource = lib.cleanSourceWith {
    src = sourceRoot;
    filter =
      path: type:
      (craneLib.filterCargoSources path type)
      # The shared scheduler is host runtime source included by completion.rs.
      || (type != "directory" && relativeSourcePath path == "src/script/completion.js")
      # The script unit tests include the checked-in example plugin source.
      || lib.hasPrefix "${sourceRootString}/examples/" (toString path);
  };
  composedSource = proseqlSource.composeCargoSource cleanSource;
  # korrid ships no plugins. The one plugin-owned file it still reads is the
  # shared libretro helper, reached through a relative symlink so the checkout
  # keeps a single copy beside the cores that import it. Nix sources cannot
  # retain a symlink that escapes sourceRoot, so materialize it here.
  src = pkgs.runCommand "korrid-source-with-materialized-examples" { } ''
    mkdir -p "$out"
    cp -R --no-preserve=mode,ownership ${composedSource}/. "$out/"
    rm -f "$out/examples/libretro-retroarch.ts"
    cp ${../../plugins/libretro/retroarch.ts} "$out/examples/libretro-retroarch.ts"
  '';
  commonArgs = {
    inherit src;
    # crane reads Cargo.lock and .cargo/config.toml during evaluation. Vendor
    # from the filtered checkout so evaluating this package for another
    # platform does not force a build of the composed source derivation.
    cargoVendorDir = craneLib.vendorCargoDeps { src = cleanSource; };
    pname = "korrid";
    version = "0.0.0";
    strictDeps = true;
    meta.mainProgram = "korrid";
    nativeBuildInputs = [
      pkgs.clang
      pkgs.llvmPackages.libclang
    ];
    LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
  };
  cargoArtifacts = craneLib.buildDepsOnly (
    (builtins.removeAttrs commonArgs [ "src" ])
    // {
      # The dummy source is derived by reading the crate layout during
      # evaluation. Use the filtered checkout for the same reason as the vendor
      # directory above; the proseql cache link is re-added by the script.
      dummySrc = craneLib.mkDummySrc {
        src = cleanSource;
        extraDummyScript = proseqlSource.dummySourceScript;
      };
    }
  );
in
craneLib.buildPackage (
  commonArgs
  // {
    inherit cargoArtifacts;
    # korrid bundles no plugin declarations. A plugin arrives through the
    # plugin host's installed registry, never through this package.
    preConfigure = ''
      if [[ -e plugins ]]; then
        echo 'korrid source carries bundled plugins again' >&2
        exit 1
      fi
    '';
    # Probe binaries and most tests include review fixtures outside this crate
    # package. The flake package ships the runtime binary plus embedded library;
    # the full repository gate remains `nix run .#korrid-check`.
    cargoBuildExtraArgs = "--bin korrid --lib";
    # The Nix builder rejects ACL mutation. The exact ACL policy test still
    # runs here; korrid-check runs all real-filesystem cases outside the sandbox.
    cargoTestExtraArgs = "--bin korrid -- --skip credential_accepts_systemd_acl --skip credential_rejects_other_users_groups_and_world_access --skip host_portal_reads_acl_credentials_and_rejects_symlinks_and_fifos";
    postInstall = ''
      if ${pkgs.gnugrep}/bin/grep -R -F 'KORRI_PRIVATE_STATE_ROOT' src; then
        echo 'korrid source contains the retired private-state environment name' >&2
        exit 1
      fi
      ${pkgs.bash}/bin/bash ${./package-runtime-check.sh} \
        "$out/bin/korrid" \
        ${pkgs.bash}/bin/bash \
        ${pkgs.curl}/bin/curl \
        ${pkgs.coreutils}/bin
    '';
  }
)
