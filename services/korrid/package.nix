{
  pkgs,
  crane,
  proseql,
}:
let
  lib = pkgs.lib;
  craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.stable.latest.default;
  proseqlSource = import ./proseql-source.nix { inherit pkgs proseql; };
  retroarchInputplumberAutoconfig =
    pkgs.callPackage ../inputd/nix/retroarch-inputplumber-autoconfig.nix
      { };
  sourceRoot = ./.;
  sourceRootString = toString sourceRoot;
  bundledPluginSources = [
    "plugins/android-app.plugin.ts"
    "plugins/mgba.plugin.ts"
    "plugins/moonlight.plugin.ts"
    "plugins/retroarch.plugin.ts"
  ];
  relativeSourcePath = path: lib.removePrefix "${sourceRootString}/" (toString path);
  cleanSource = lib.cleanSourceWith {
    src = sourceRoot;
    filter =
      path: type:
      (craneLib.filterCargoSources path type)
      # The script unit tests include the checked-in example plugin source.
      || lib.hasPrefix "${sourceRootString}/examples/" (toString path)
      # The production plugin is bundled with include_str! and must survive the
      # clean/composed cargo source, without pulling in arbitrary plugin source.
      || (type == "directory" && (toString path) == "${sourceRootString}/plugins")
      || (type != "directory" && builtins.elem (relativeSourcePath path) bundledPluginSources);
  };
  composedSource = proseqlSource.composeCargoSource cleanSource;
  # The checkout uses a relative symlink so the plugin-owned declaration stays
  # beside its Android acquisition/build package. Nix sources cannot retain a
  # symlinks that escape sourceRoot, so materialize those canonical files in
  # the hermetic crate source.
  src = pkgs.runCommand "korrid-source-with-bundled-plugins" { } ''
    mkdir -p "$out"
    cp -R --no-preserve=mode,ownership ${composedSource}/. "$out/"
    rm -f "$out/plugins/mgba.plugin.ts" "$out/plugins/moonlight.plugin.ts" "$out/plugins/retroarch.plugin.ts"
    cp ${../../plugins/mgba/plugin.ts} "$out/plugins/mgba.plugin.ts"
    cp ${../../plugins/moonlight/plugin.ts} "$out/plugins/moonlight.plugin.ts"
    cp ${../../plugins/retroarch/plugin.ts} "$out/plugins/retroarch.plugin.ts"
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
      pkgs.makeWrapper
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
    preConfigure = ''
      plugin_sources="$(${pkgs.findutils}/bin/find plugins -type f -name '*.plugin.ts' -printf '%P\n' | sort)"
      expected_plugin_sources=$'android-app.plugin.ts\nmgba.plugin.ts\nmoonlight.plugin.ts\nretroarch.plugin.ts'
      if [[ "$plugin_sources" != "$expected_plugin_sources" ]]; then
        echo "unexpected bundled plugin source set:" >&2
        printf '%s\n' "$plugin_sources" >&2
        exit 1
      fi
    '';
    # Probe binaries and most tests include review fixtures outside this crate
    # package. The flake package ships the runtime binary plus embedded library;
    # the full repository gate remains `nix run .#korrid-check`.
    cargoBuildExtraArgs = "--bin korrid --lib";
    cargoTestExtraArgs = "--bin korrid";
    postInstall = ''
      if ${pkgs.gnugrep}/bin/grep -R -F 'KORRI_PRIVATE_STATE_ROOT' src; then
        echo 'korrid source contains the retired private-state environment name' >&2
        exit 1
      fi
      wrapProgram "$out/bin/korrid" \
        --set KORRI_RETROARCH_EXECUTABLE ${pkgs.retroarch-bare}/bin/retroarch \
        --set KORRI_MGBA_CORE ${pkgs.libretro.mgba}/lib/retroarch/cores/mgba_libretro.so \
        --set KORRI_RETROARCH_AUTOCONFIG ${retroarchInputplumberAutoconfig}/share/libretro/autoconfig
      ${pkgs.bash}/bin/bash ${./package-runtime-check.sh} \
        "$out/bin/korrid" \
        ${pkgs.bash}/bin/bash \
        ${pkgs.curl}/bin/curl \
        ${pkgs.jq}/bin/jq \
        ${pkgs.coreutils}/bin
    '';
  }
)
