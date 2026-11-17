# The Korri Android app with no Korri Java (#434, slice D8): korrid and the
# native shell in one library that Android's own NativeActivity loads. It is
# built as the viewer's APK is (clients/viewer/android.nix): Nix, cargo,
# aapt2 and zipalign, no Gradle. Two ABIs: arm64-v8a for phones, x86_64 for
# the emulator, at API 31 (ADR 0064). The APK is debuggable and not signed;
# the proofs sign it with the user's debug key. Not for distribution.
#
# `releaseApk` (#434 D12) is the APK for a phone: the same product library
# for arm64 only, not debuggable, with its notices (notices.nix). It is not
# signed either: sign-release.sh signs it with the release key, outside the
# store (README).
#
# The relay list is an asset: `apk` packs the product's relays, and
# `fixtureApk` one loopback relay that the holder proofs (proof/holder.py,
# #434 D9; proof/play.py, D10) reach through adb.
#
# Since D11 the fixture's library is its own build: the same source with
# the cargo feature `proof`, the emulator proof's trigger for the stream's
# controls (src/proof.rs), for x86_64 only, the emulator's ABI. The product
# library's build checks that it carries none of the trigger.
#
# Since D10 the library links the Rust viewer too (clients/viewer), with the
# network core, OpenSSL and libopus that clients/viewer/android.nix builds
# for each ABI (`viewerParts`), exactly as the viewer's own APK links them.
{
  pkgs,
  proseql,
  viewerParts,
  # The flake's own commit, which the release APK's notices name as its
  # source. Null for a checkout that is not clean.
  sourceRevision ? null,
}:
let
  inherit (pkgs) lib;
  sdk = import ../android/sdk.nix { inherit pkgs; };
  api = "31";
  ndk = sdk.ndkRoot;
  toolchain = "${ndk}/toolchains/llvm/prebuilt/linux-x86_64";
  buildTools = "${sdk.sdkRoot}/build-tools/${sdk.buildToolsVersion}";
  # The newest platform in sdk.nix: aapt2's view of the framework, and the
  # android.jar that Slint's Java helper compiles against (its build.rs).
  platform = "android-36";
  framework = "${sdk.sdkRoot}/platforms/${platform}/android.jar";
  proseqlSource = import ../../services/korrid/proseql-source.nix { inherit pkgs proseql; };
  # The product's relays, as the Artemis app's BuildConfig.PRODUCT_RELAYS.
  # constants.relays does not force the product's korri argument (#315).
  productRelays = import ../android/product.nix {
    inherit pkgs;
    korri = null;
  };
  # The holder proof's one relay, inside the emulator; the proof maps this
  # port to its own relay on the host with `adb reverse`.
  fixtureRelays = pkgs.writeText "korri-android-app-fixture-relays.json" (
    builtins.toJSON [ "ws://127.0.0.1:47800" ]
  );

  # rustls-platform-verifier's Kotlin component and the Kotlin standard
  # library that it needs, unchanged, dexed into one file that korrid loads
  # in memory (services/korrid/src/android/app.rs). The component's version
  # is the one that Cargo.lock pins: its JVM part and its Rust part must
  # match, as clients/android/app/build.gradle requires.
  verifierVersion = "0.2.0";
  lockedVerifier =
    (lib.findFirst (crate: crate.name == "rustls-platform-verifier-android") null
      (builtins.fromTOML (builtins.readFile ./Cargo.lock)).package
    ).version;
  verifierAar =
    assert lib.assertMsg (
      lockedVerifier == verifierVersion
    ) "Cargo.lock pins rustls-platform-verifier-android ${lockedVerifier}; fetch that AAR here";
    pkgs.fetchurl {
      # The repository that clients/android/build.gradle names for org.rustls.
      url = "https://raw.githubusercontent.com/rustls/rustls-platform-verifier/maven-archive/android-release-support/maven/org/rustls/rustls-platform-verifier/${verifierVersion}/rustls-platform-verifier-${verifierVersion}.aar";
      hash = "sha256-qgIXlCMPvC8L41WZniz2c5jdVj3gZqLhcAH/vQtpEBs=";
    };
  # The component's classes call kotlin.enums.EnumEntries (Kotlin 1.9 and
  # later); its POM names no version.
  kotlinVersion = "2.0.21";
  kotlinStdlib = pkgs.fetchurl {
    url = "https://repo1.maven.org/maven2/org/jetbrains/kotlin/kotlin-stdlib/${kotlinVersion}/kotlin-stdlib-${kotlinVersion}.jar";
    hash = "sha256-8xzFPxBafkjAk2g7vVQ3Vh0SM5IFE3dLRwgFZBvtvAk=";
  };
  verifierDex =
    pkgs.runCommand "korri-android-app-verifier-dex"
      {
        nativeBuildInputs = [
          pkgs.unzip
          sdk.jdk
        ];
      }
      ''
        unzip -q ${verifierAar} classes.jar
        mkdir -p $out
        ${buildTools}/d8 --release --min-api ${api} --lib ${framework} \
          --output $out classes.jar ${kotlinStdlib}
        # One dex file: korrid gives the loader one buffer.
        test "$(ls $out)" = classes.dex
      '';

  # Slint's Android renderer is Skia. skia-bindings downloads its prebuilt
  # archive at build time; here each is a fixed input, read through a file
  # URL. The key is skia-bindings 0.153.3's repository hash, the target and
  # the features that Slint 1.18.1 asks for.
  skiaVersion = "0.153.3";
  skiaKey = target: "b7f043e0b1e2a850e702-${target}-ganesh-gl-jpegd-jpege-pdf-vulkan";
  abis = lib.mapAttrs (name: abi: abi // { inherit name; }) {
    arm64-v8a = {
      target = "aarch64-linux-android";
      machine = "AArch64";
      skia = "06gmdhzfmzv509g3m28qjvw66mmybdqzy04f8al2yqq3imh7p48d";
    };
    x86_64 = {
      target = "x86_64-linux-android";
      machine = "Advanced Micro Devices X86-64";
      skia = "1cclyw6yihmgbdm05k8lr0kmzac64s4nx0xcf29fjizz84w14954";
    };
  };
  skia = pkgs.linkFarm "skia-binaries-${skiaVersion}" (
    lib.mapAttrsToList (_: abi: {
      name = "skia-binaries-${skiaKey abi.target}.tar.gz";
      path = pkgs.fetchurl {
        url = "https://github.com/rust-skia/skia-binaries/releases/download/${skiaVersion}/skia-binaries-${skiaKey abi.target}.tar.gz";
        sha256 = abi.skia;
      };
    }) abis
  );

  rust = pkgs.rust-bin.stable.latest.default.override {
    targets = map (abi: abi.target) (lib.attrValues abis);
  };

  # The surface packages, verified on the build machine as the Linux image
  # verifies them (clients/shell/nix/installed.nix). The verifier is the
  # shell's own release build; its test suite runs in `nix run .#shell-check`.
  verifier = (import ../shell/package.nix { pkgs = pkgs.buildPackages; }).overrideAttrs (_: {
    doCheck = false;
    cargoBuildType = "release";
  });
  surfaces = import ../shell/nix/installed.nix {
    inherit pkgs;
    shell = verifier;
  };

  # One cargo invocation's environment for one ABI.
  environmentFor =
    abi:
    let
      # Shell names cannot hold "-"; cc-rs and bindgen also read the "_" form.
      target = lib.replaceStrings [ "-" ] [ "_" ] abi.target;
      cargoTarget = lib.toUpper target;
      cc = "${toolchain}/bin/${abi.target}${api}-clang";
      viewer = viewerParts.${abi.name};
    in
    ''
      # The Rust viewer's C parts (clients/viewer/android.nix, environmentFor).
      export MOONLIGHT_COMMON_INCLUDE=${viewer.network}/include
      export MOONLIGHT_COMMON_LIB=${viewer.network}/lib
      export OPUS_LIB=${viewer.opus}/lib
      export ${cargoTarget}_OPENSSL_DIR=${viewer.openssl}
      export ${cargoTarget}_OPENSSL_STATIC=1
      export CC_${target}=${cc}
      export CXX_${target}=${cc}++
      export AR_${target}=${toolchain}/bin/llvm-ar
      export CARGO_TARGET_${cargoTarget}_LINKER=${cc}
      # 16 KiB load segments, as clients/android/build.sh links its libraries.
      export CARGO_TARGET_${cargoTarget}_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
      export BINDGEN_EXTRA_CLANG_ARGS_${target}="--target=${abi.target}${api} --sysroot=${toolchain}/sysroot"
    '';

  # Android ships every library the app needs.
  systemLibraries = [
    "libc.so"
    "libm.so"
    "libdl.so"
    "liblog.so"
    "libandroid.so"
    # Slint's Skia renderer.
    "libEGL.so"
    "libGLESv2.so"
    "libvulkan.so"
    "libjnigraphics.so"
    # The Rust viewer (clients/viewer/android.nix, sessionLibraries): curl's
    # zlib, the picture's AMediaCodec and the sound's AAudio.
    "libz.so"
    "libmediandk.so"
    "libaaudio.so"
  ];
  library = "libkorri_android_app.so";
  # A string that only the proof's trigger has (src/proof.rs, its log lines).
  proofMarker = "korri-app: proof:";

  src = proseqlSource.composeCargoSource (
    lib.fileset.toSource {
      root = ../..;
      fileset = lib.fileset.unions [
        ./Cargo.toml
        ./Cargo.lock
        ./src
        ./tests
        ../shell/Cargo.toml
        ../shell/src
        ../shell/examples
        ../shell/fixtures
        ../shell/vendor
        ../viewer/Cargo.toml
        ../viewer/build.rs
        ../viewer/src
        ../../services/korrid/Cargo.toml
        ../../services/korrid/src
        # The vendored CPace crate (the join's key exchange, ADR 0070).
        ../../services/korrid/vendor
        ../../services/korrid/examples
        # Producer declaration snapshots that the library includes.
        ../../services/korrid/tests/fixtures/first-party
        ../../contracts
        ../../packages/input-trace
      ];
    }
  );

  cargoDeps = pkgs.rustPlatform.importCargoLock { lockFile = ./Cargo.lock; };

  # Fails the build when cargo built `crate`'s library with `feature`. It
  # reads the features of cargo's build fingerprints, as
  # services/korrid/package.nix checks the shipped korrid. Each match is
  # tested, so the glob options of the later phases stay as stdenv set them.
  refuseFeature =
    {
      target,
      crate,
      file,
      feature,
    }:
    ''
      fingerprints=()
      for fingerprint in target/${target}/release/.fingerprint/${crate}-*/lib-${file}.json; do
        if [ -e "$fingerprint" ]; then fingerprints+=("$fingerprint"); fi
      done
      if (( ''${#fingerprints[@]} == 0 )); then
        echo 'no build fingerprint of ${crate} to check its features' >&2
        exit 1
      fi
      if grep -l '"features":"\[[^]]*${feature}' "''${fingerprints[@]}"; then
        echo '${library} was built with the feature ${feature} of ${crate}' >&2
        exit 1
      fi
    '';

  # The library for one ABI. Its own derivation, so that a change to the
  # packages or the manifest packs the APK again without compiling.
  # `proof`: the fixture's library, with the proof's trigger.
  libraryFor =
    {
      abi,
      proof ? false,
    }:
    pkgs.stdenv.mkDerivation {
      pname = "korri-android-app-${lib.optionalString proof "fixture-"}${abi.name}";
      version = "0.1.0";
      inherit src cargoDeps;
      sourceRoot = "korrid-source-with-proseql/clients/android-app";
      nativeBuildInputs = [
        rust
        pkgs.rustPlatform.cargoSetupHook
        sdk.jdk
      ];
      LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      SKIA_BINARIES_URL = "file://${skia}/skia-binaries-{key}.tar.gz";
      KORRI_ANDROID_VERIFIER_DEX = "${verifierDex}/classes.dex";
      # Slint's build.rs compiles its Java helper and dexes it (android-build).
      ANDROID_HOME = sdk.sdkRoot;
      ANDROID_SDK_ROOT = sdk.sdkRoot;
      ANDROID_PLATFORM = platform;
      ANDROID_BUILD_TOOLS_VERSION = sdk.buildToolsVersion;
      JAVA_HOME = sdk.jdk;
      dontStrip = true;
      dontFixup = true;
      # The rules that need no Android, on the build machine (tests/host.rs).
      doCheck = true;
      checkPhase = ''
        runHook preCheck
        cargo test --locked --offline --test host
        runHook postCheck
      '';
      buildPhase = ''
        runHook preBuild
        export HOME=$TMPDIR
        ${environmentFor abi}
        cargo build --locked --release --offline --lib --target ${abi.target}${lib.optionalString proof " --features proof"}
        built=target/${abi.target}/release/${library}
        "${toolchain}/bin/llvm-readelf" --file-header --dynamic $built > elf.txt
        grep -q 'Machine: *${abi.machine}' elf.txt
        grep -q 'Type: *DYN' elf.txt
        grep NEEDED elf.txt | sed 's/.*\[\(.*\)\]/\1/' | sort > elf.txt.needed
        if grep -v -x ${lib.concatMapStringsSep " " (name: "-e ${name}") systemLibraries} elf.txt.needed; then
          echo '${library} needs a library that Android does not ship' >&2
          exit 1
        fi
        "${toolchain}/bin/llvm-nm" --defined-only --extern-only $built | grep -q ' T ANativeActivity_onCreate$'
        # No library of this app carries the federation fixture's in-memory
        # launch units (korrid's feature `fixture`, #434 D10), as no shipped
        # korrid does (services/korrid/package.nix).
        ${refuseFeature {
          inherit (abi) target;
          crate = "korrid";
          file = "korrid";
          feature = "fixture";
        }}
        ${
          if proof then
            ''
              # The fixture's library has the proof's trigger.
              grep -q -F '${proofMarker}' $built
            ''
          else
            ''
              # The product library never carries the proof's trigger (feature
              # `proof`, #434 D11): the features cargo built it with, and its bytes.
              ${refuseFeature {
                inherit (abi) target;
                crate = "korri-android-app";
                file = "korri_android_app";
                feature = "proof";
              }}
              # Nor korrid's half of it (#476 slice 3).
              ${refuseFeature {
                inherit (abi) target;
                crate = "korrid";
                file = "korrid";
                feature = "proof";
              }}
              if grep -q -F '${proofMarker}' $built; then
                echo '${library} carries the proof trigger' >&2
                exit 1
              fi
            ''
        }
        runHook postBuild
      '';
      installPhase = ''
        runHook preInstall
        mkdir -p $out
        cp target/${abi.target}/release/${library} $out/
        cp elf.txt $out/elf-${abi.name}.txt
        runHook postInstall
      '';
    };
  libraries = lib.mapAttrs (_: abi: libraryFor { inherit abi; }) abis;
  fixtureLibraries = {
    x86_64 = libraryFor {
      abi = abis.x86_64;
      proof = true;
    };
  };

  # The release APK's notices (notices.nix): every licence of what the APK
  # carries, except the source commit, which the packing adds.
  notices = import ./notices.nix {
    inherit
      pkgs
      src
      cargoDeps
      rust
      surfaces
      skiaVersion
      kotlinVersion
      ndk
      ;
    viewerParts = viewerParts.arm64-v8a;
    skiaArchive = "${skia}/skia-binaries-${skiaKey abis.arm64-v8a.target}.tar.gz";
    target = abis.arm64-v8a.target;
  };

  # The paragraph that opens the release APK's notices: the licence of the
  # whole app and where its source is (GPL-3.0 section 6(d)). The commit is
  # the flake's own revision, which Nix knows only for a clean checkout.
  sourceNotice =
    revision:
    pkgs.writeText "korri-android-app-source-notice" ''
      Korri (native), the Android app com.simonwjackson.korri.app
      ===========================================================

      This app is free software. Its library links Korri's stream viewer,
      whose Android parts port Artemis code under the GNU General Public
      License version 3 only, and moonlight-common-c, which is under GPL
      version 3 with no "or later" grant. So the app as a whole is
      distributed under the GNU General Public License version 3 only
      (GPL-3.0-only). The full text of that licence is at the end of this
      file. This app comes with ABSOLUTELY NO WARRANTY, to the extent
      permitted by law (sections 15 and 16 of the licence).

      Source code
      -----------

      The complete corresponding source code of this build is commit
      ${revision} of the public repository
      https://github.com/korri-os/korri, at

        https://github.com/korri-os/korri/tree/${revision}

      flake.lock, clients/android-app/Cargo.lock and the hashes in the Nix
      files of that commit pin every other source that the build reads,
      each from a public location: nixpkgs (OpenSSL, libopus and the
      toolchains), moonlight-embedded (moonlight-common-c and ENet),
      proseql, the Rust crates on crates.io, and the Kotlin standard
      library and rustls-platform-verifier's Kotlin component on Maven
      repositories. Skia is the prebuilt archive of rust-skia's release
      ${skiaVersion}, built from https://github.com/rust-skia/rust-skia at
      tag ${skiaVersion}. This command builds the same unsigned APK:

        nix build github:korri-os/korri/${revision}#korri-android-app-release-apk

      The parts of the app that other people wrote, and their licences,
      follow.

    '';

  # One packing of `libs` (by ABI), with `relays` as the relay list.
  # `release` is null for a build for development and the proofs, which is
  # debuggable. For the release APK it holds the source commit (null when
  # the checkout is not clean, which fails the build): such an APK is not
  # debuggable and carries the notices (assets/notices/NOTICES.txt).
  packApk =
    {
      pname,
      relays,
      libs,
      release ? null,
    }:
    pkgs.stdenv.mkDerivation {
      inherit pname;
      version = "0.1.0";
      src = lib.fileset.toSource {
        root = ./.;
        fileset = ./AndroidManifest.xml;
      };
      nativeBuildInputs = [
        pkgs.zip
        pkgs.unzip
      ];
      dontStrip = true;
      dontFixup = true;
      buildPhase = ''
        runHook preBuild
        ${lib.concatStrings (
          lib.mapAttrsToList (name: built: ''
            mkdir -p stage/lib/${name}
            cp ${built}/${library} stage/lib/${name}/
            cp ${built}/elf-${name}.txt .
          '') libs
        )}
        # The packages exactly as the Artemis app carries them
        # (clients/android/app/build.gradle, surfaceAssets).
        mkdir -p assets/surfaces
        (
          cd ${surfaces}/packages
          find pico zone velvet -type f \( -name '*.slint' -o -name '*.js' -o -path '*/fonts/*' \) -print
          find pico zone -type f -path '*/art/*' -print
        ) | sort -u | while read -r file; do
          install -D -m 0644 "${surfaces}/packages/$file" "assets/surfaces/$file"
        done
        test -f assets/surfaces/velvet/main.slint
        test -f assets/surfaces/velvet/logic.js
        install -m 0644 ${relays} assets/relays.json
        ${
          if release == null then
            ''
              # --debug-mode makes the APK debuggable, as the viewer's is: a build
              # for development and the proofs. The release APK leaves it out.
              ${buildTools}/aapt2 link -o unsigned.apk -I ${framework} \
                --manifest AndroidManifest.xml -A assets --debug-mode
            ''
          else if release.revision == null then
            ''
              echo 'the release APK names its source commit: build it from a clean, committed checkout' >&2
              exit 1
            ''
          else
            ''
              mkdir -p assets/notices
              cat ${sourceNotice release.revision} ${notices}/NOTICES.txt > assets/notices/NOTICES.txt
              ${buildTools}/aapt2 link -o unsigned.apk -I ${framework} \
                --manifest AndroidManifest.xml -A assets
            ''
        }
        # Fixed times and no extra attributes, as aapt2 writes its entries.
        find stage -exec touch -h -d @315532800 {} +
        (cd stage && zip -q -r -X ../unsigned.apk lib)
        ${buildTools}/zipalign -p -f 4 unsigned.apk aligned.apk
        ${lib.optionalString (release != null) ''
          # Not debuggable: no android:debuggable in the manifest at all, so
          # neither run-as nor a debugger reaches the app's process and files.
          ${buildTools}/aapt2 dump xmltree --file AndroidManifest.xml aligned.apk > manifest.txt
          if grep -i debuggable manifest.txt; then
            echo 'the release APK is debuggable' >&2
            exit 1
          fi
          ${buildTools}/aapt2 dump badging aligned.apk > badging.txt
          if grep application-debuggable badging.txt; then
            echo 'the release APK is debuggable' >&2
            exit 1
          fi
          grep -q -x "native-code: ${lib.concatMapStringsSep " " (name: "'${name}'") (lib.attrNames libs)}" badging.txt
          unzip -l aligned.apk > entries.txt
          grep -q ' assets/notices/NOTICES.txt$' entries.txt
          if grep -q 'classes.*\.dex$' entries.txt; then
            echo 'the release APK carries a dex' >&2
            exit 1
          fi
        ''}
        runHook postBuild
      '';
      installPhase = ''
        runHook preInstall
        mkdir -p $out
        cp aligned.apk $out/korri-android-app-${lib.optionalString (release != null) "release-"}unsigned.apk
        cp elf-*.txt $out/
        ${lib.optionalString (release != null) "cp manifest.txt badging.txt $out/"}
        runHook postInstall
      '';
    };
  apk = packApk {
    pname = "korri-android-app-apk";
    relays = productRelays;
    libs = libraries;
  };
  fixtureApk = packApk {
    pname = "korri-android-app-fixture-apk";
    relays = fixtureRelays;
    libs = fixtureLibraries;
  };
  # The APK for a phone (#434 D12): the product library for arm64-v8a only,
  # not debuggable, with the notices. It is not signed: the release key
  # stays out of the store (sign-release.sh signs it, README).
  releaseApk = packApk {
    pname = "korri-android-app-release-apk";
    relays = productRelays;
    libs = { inherit (libraries) arm64-v8a; };
    release.revision = sourceRevision;
  };
in
{
  inherit
    apk
    fixtureApk
    releaseApk
    notices
    libraries
    surfaces
    verifierDex
    ;
}
