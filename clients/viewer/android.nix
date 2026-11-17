# The shared session part on Android (#391 step D) and the viewer app around
# it (slices D1 to D4b), at API 31 (ADR 0064) with the NDK that clients/android uses.
# Two ABIs: arm64-v8a for phones, x86_64 for the emulator.
#
# - check: builds the library and links its own tests and the HTTPS and
#   trust-anchor test executables for each ABI, then reads them. Nothing runs
#   here; the app's proof runs the x86_64 ones on the emulator.
# - apk: the app. Android's own NativeActivity loads the Rust library; no
#   Java and no Gradle. Nix builds the libraries with cargo and packs them with
#   aapt2 and zipalign. The APK is not signed; android/proof/run.py signs it
#   with the user's debug key.
# - nightjarCheck, nightjarApk: the same with the `nightjar` feature, which
#   adds nightjar-core as a second network core (#391 step E,
#   vendor/README.md). The intent's `-networkcore nightjar` picks it; the C
#   core stays the default.
{
  pkgs,
  upstream,
  sourceFiles,
  cargoDeps,
}:
let
  inherit (pkgs) lib;
  sdk = import ../android/sdk.nix { inherit pkgs; };
  api = "31";
  ndk = sdk.ndkRoot;
  toolchain = "${ndk}/toolchains/llvm/prebuilt/linux-x86_64";
  buildTools = "${sdk.sdkRoot}/build-tools/${sdk.buildToolsVersion}";
  # The newest platform in sdk.nix, for aapt2's view of the framework.
  framework = "${sdk.sdkRoot}/platforms/android-36/android.jar";

  abis = {
    arm64-v8a = {
      target = "aarch64-linux-android";
      opensslTarget = "android-arm64";
      machine = "AArch64";
      # RustCrypto's aes 0.8 and polyval 0.6, which nightjar-core decrypts
      # with, compile their ARMv8 AES and PMULL code only under these two
      # cfgs; without them every aarch64 build decrypts in software.
      # nightjar-core refuses to build for aarch64 without them
      # (vendor/nightjar/protocol/client/src/crypto.rs:10). Nightjar's own
      # protocol/.cargo/config.toml sets them, which a cargo run outside its
      # workspace never reads. Only the nightjar builds take them.
      nightjarRustflags = [
        "--cfg"
        "aes_armv8"
        "--cfg"
        "polyval_armv8"
      ];
    };
    x86_64 = {
      target = "x86_64-linux-android";
      opensslTarget = "android-x86_64";
      machine = "Advanced Micro Devices X86-64";
    };
  };
  rust = pkgs.rust-bin.stable.latest.default.override {
    targets = map (abi: abi.target) (lib.attrValues abis);
  };

  # The OpenSSL release that the Linux viewer links (nixpkgs' source, without
  # its Nix certificate-path patches), as static archives. The network core
  # needs libcrypto on Android as on Linux; the session's openssl and curl
  # crates link this same copy, so the binary holds one OpenSSL.
  opensslFor =
    name: abi:
    pkgs.stdenv.mkDerivation {
      pname = "korri-viewer-android-openssl-${name}";
      inherit (pkgs.openssl) version src;
      nativeBuildInputs = [ pkgs.perl ];
      dontStrip = true;
      configurePhase = ''
        runHook preConfigure
        export ANDROID_NDK_ROOT=${ndk} PATH=${toolchain}/bin:$PATH
        unset CC CXX AR RANLIB LD
        perl ./Configure ${abi.opensslTarget} -D__ANDROID_API__=${api} \
          no-shared no-module no-tests no-docs --prefix=$out --libdir=lib
        runHook postConfigure
      '';
      buildPhase = ''
        runHook preBuild
        make -j$NIX_BUILD_CORES build_libs
        runHook postBuild
      '';
      installPhase = ''
        runHook preInstall
        make install_dev
        runHook postInstall
      '';
    };

  # The unmodified core and ENet that the Linux viewer links (default.nix),
  # with their own CMake recipe and the NDK's toolchain file, as static
  # archives against the OpenSSL above.
  networkFor =
    name: openssl:
    pkgs.stdenv.mkDerivation {
      pname = "korri-viewer-android-moonlight-common-c-${name}";
      version = "b126e481";
      src = "${upstream}/third_party/moonlight-common-c";
      nativeBuildInputs = [ pkgs.cmake ];
      dontUseCmakeConfigure = true;
      dontStrip = true;
      configurePhase = ''
        runHook preConfigure
        # The toolchain file searches only the NDK sysroot, so name OpenSSL.
        cmake -S . -B build \
          -DCMAKE_TOOLCHAIN_FILE=${ndk}/build/cmake/android.toolchain.cmake \
          -DANDROID_ABI=${name} -DANDROID_PLATFORM=android-${api} \
          -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF \
          -DOPENSSL_INCLUDE_DIR=${openssl}/include \
          -DOPENSSL_CRYPTO_LIBRARY=${openssl}/lib/libcrypto.a
        runHook postConfigure
      '';
      buildPhase = ''
        runHook preBuild
        cmake --build build -j$NIX_BUILD_CORES
        runHook postBuild
      '';
      installPhase = ''
        runHook preInstall
        mkdir -p $out/lib $out/include
        cp build/libmoonlight-common-c.a build/enet/libenet.a $out/lib/
        cp src/Limelight.h $out/include/
        runHook postInstall
      '';
    };

  # The sound's Opus decoder (slice D3): the libopus release that the Linux
  # viewer links (default.nix, without custom modes), from nixpkgs' source,
  # with its own CMake recipe and the NDK's toolchain file, as a static
  # archive. Artemis links prebuilt archives of moonlight-mobile-deps instead.
  opusFor =
    name:
    pkgs.stdenv.mkDerivation {
      pname = "korri-viewer-android-opus-${name}";
      inherit (pkgs.libopus) version src;
      nativeBuildInputs = [ pkgs.cmake ];
      dontUseCmakeConfigure = true;
      dontStrip = true;
      configurePhase = ''
        runHook preConfigure
        cmake -S . -B build \
          -DCMAKE_TOOLCHAIN_FILE=${ndk}/build/cmake/android.toolchain.cmake \
          -DANDROID_ABI=${name} -DANDROID_PLATFORM=android-${api} \
          -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF \
          -DOPUS_CUSTOM_MODES=OFF -DOPUS_BUILD_PROGRAMS=OFF -DOPUS_BUILD_TESTING=OFF \
          -DCMAKE_INSTALL_PREFIX=$out -DCMAKE_INSTALL_LIBDIR=lib
        runHook postConfigure
      '';
      buildPhase = ''
        runHook preBuild
        cmake --build build -j$NIX_BUILD_CORES
        runHook postBuild
      '';
      installPhase = ''
        runHook preInstall
        cmake --install build
        runHook postInstall
      '';
    };

  parts = lib.mapAttrs (
    name: abi:
    let
      openssl = opensslFor name abi;
    in
    abi
    // {
      inherit name openssl;
      network = networkFor name openssl;
      opus = opusFor name;
    }
  ) abis;

  # One cargo invocation's environment for one ABI. The core's path is not
  # scoped by target in build.rs, so each ABI gets its own invocation.
  # `nightjar` adds the ABI's flags for nightjar-core.
  environmentWith =
    nightjar: part:
    let
      rustflags = [
        "-C link-arg=-Wl,-z,max-page-size=16384"
      ]
      ++ lib.optionals nightjar (part.nightjarRustflags or [ ]);
      # Shell names cannot hold "-"; cc-rs and bindgen also read the "_" form.
      target = lib.replaceStrings [ "-" ] [ "_" ] part.target;
      cargoTarget = lib.toUpper target;
      cc = "${toolchain}/bin/${part.target}${api}-clang";
    in
    ''
      export MOONLIGHT_COMMON_INCLUDE=${part.network}/include
      export MOONLIGHT_COMMON_LIB=${part.network}/lib
      export OPUS_LIB=${part.opus}/lib
      export ${cargoTarget}_OPENSSL_DIR=${part.openssl}
      export ${cargoTarget}_OPENSSL_STATIC=1
      export CC_${target}=${cc}
      export AR_${target}=${toolchain}/bin/llvm-ar
      export CARGO_TARGET_${cargoTarget}_LINKER=${cc}
      # 16 KiB load segments, as clients/android/build.sh links its libraries.
      export CARGO_TARGET_${cargoTarget}_RUSTFLAGS="${lib.concatStringsSep " " rustflags}"
      export BINDGEN_EXTRA_CLANG_ARGS_${target}="--target=${part.target}${api} --sysroot=${toolchain}/sysroot"
    '';
  environmentFor = environmentWith false;
  forEachPart = script: lib.concatMapStrings script (lib.attrValues parts);

  common = {
    nativeBuildInputs = [
      rust
      pkgs.rustPlatform.cargoSetupHook
      pkgs.perl
    ];
    LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
    dontStrip = true;
    dontFixup = true;
  };

  # A library or executable for Android needs only Android's own libraries.
  readElf = part: allowed: file: report: ''
    "${toolchain}/bin/llvm-readelf" --file-header --program-headers --dynamic "${file}" > "${report}"
    grep -q 'Machine: *${part.machine}' "${report}"
    grep NEEDED "${report}" | sed 's/.*\[\(.*\)\]/\1/' | sort > "${report}.needed"
    if grep -v -x ${lib.concatMapStringsSep " " (name: "-e ${name}") allowed} "${report}.needed"; then
      echo '${file} needs a library that Android does not ship' >&2
      exit 1
    fi
  '';
  sessionLibraries = [
    "libc.so"
    "libm.so"
    "libdl.so"
    "libz.so"
    "liblog.so"
    # The picture (slice D2): AMediaCodec, and ANativeWindow for its Surface.
    "libmediandk.so"
    "libandroid.so"
    # The sound (slice D3): AAudio.
    "libaaudio.so"
  ];

  checkWith =
    nightjar:
    let
      features = lib.optionalString nightjar " --features nightjar";
    in
    pkgs.stdenv.mkDerivation (
      common
      // {
        name = if nightjar then "korri-viewer-android-nightjar-check" else "korri-viewer-android-check";
        src = lib.fileset.toSource {
          root = ../..;
          fileset = sourceFiles;
        };
        sourceRoot = "source/clients/viewer";
        inherit cargoDeps;
        buildPhase = ''
          runHook preBuild
          export HOME=$TMPDIR
          ${forEachPart (part: ''
            (
              ${environmentWith nightjar part}
              cargo build --locked --release --offline --lib --target ${part.target}${features}
              # The HTTPS test drives Transport and the launch chain into the
              # core; linking it proves that every symbol of that path resolves.
              cargo test --locked --release --offline --no-run --target ${part.target} \
                --lib --test https --test trust_anchor${features} --message-format=json > tests-${part.name}.json
            )
          '')}
          runHook postBuild
        '';
        installPhase = ''
          runHook preInstall
          ${forEachPart (part: ''
            mkdir -p $out/${part.name}/tests
            test -s target/${part.target}/release/libkorri_viewer.rlib
            for test in https trust_anchor; do
              executable=$(grep -o "\"executable\":\"[^\"]*/$test-[^\"]*\"" tests-${part.name}.json | cut -d'"' -f4)
              test -x "$executable"
              cp "$executable" $out/${part.name}/tests/$test
            done
            # The library's own tests: the proof runs the picture's pure parts.
            executable=$(grep -o "\"executable\":\"[^\"]*/korri_viewer-[^\"]*\"" tests-${part.name}.json | cut -d'"' -f4)
            test -x "$executable"
            cp "$executable" $out/${part.name}/tests/lib
            https=$out/${part.name}/tests/https
            ${readElf part sessionLibraries "$https" "$out/${part.name}/https-elf.txt"}
            grep -q 'Requesting program interpreter: /system/bin/linker64' $out/${part.name}/https-elf.txt
            # curl, OpenSSL and the core are inside the executable. The crate's
            # curl hides its symbols, so they are local (t), not global.
            "${toolchain}/bin/llvm-nm" --defined-only "$https" > symbols.txt
            for symbol in LiStartConnection LiStopConnection curl_multi_perform SSL_CTX_new EVP_aes_128_gcm viewer_core_log opus_multistream_decode; do
              grep -Eq " [Tt] $symbol$" symbols.txt || { echo "missing $symbol" >&2; exit 1; }
              echo "$symbol" >> $out/${part.name}/symbols.txt
            done
            ${lib.optionalString nightjar ''
              # The launch chain of the HTTPS test reaches nightjar-core's
              # session through the seam (`Session::start` is inlined into
              # the adapter; its inner `session::start` stays). On aarch64
              # its AES and POLYVAL also
              # have their ARMv8 code (the cfgs above), which the crates pick
              # at run time on a CPU that has the instructions.
              "${toolchain}/bin/llvm-nm" --demangle --defined-only "$https" > nightjar-symbols.txt
              for symbol in 'nightjar_core::session::start' ${
                lib.optionalString (part ? nightjarRustflags) "'aes::armv8::' 'polyval::backend::pmull::'"
              }; do
                grep -qF "$symbol" nightjar-symbols.txt || { echo "missing $symbol" >&2; exit 1; }
                echo "$symbol" >> $out/${part.name}/symbols.txt
              done
            ''}
          '')}
          runHook postInstall
        '';
      }
    );
  check = checkWith false;
  nightjarCheck = checkWith true;

  # The app's own crate has its own lock: the viewer's crates at the same
  # versions, and android-activity and jni.
  appFiles = lib.fileset.unions [
    sourceFiles
    ./android/Cargo.toml
    ./android/Cargo.lock
    ./android/src
    ./android/AndroidManifest.xml
  ];
  library = "libkorri_viewer_android.so";

  apkWith =
    nightjar:
    let
      features = lib.optionalString nightjar " --features nightjar";
    in
    pkgs.stdenv.mkDerivation (
      common
      // {
        pname = if nightjar then "korri-viewer-android-nightjar-apk" else "korri-viewer-android-apk";
        version = "0.1.0";
        src = lib.fileset.toSource {
          root = ../..;
          fileset = appFiles;
        };
        sourceRoot = "source/clients/viewer/android";
        cargoDeps = pkgs.rustPlatform.importCargoLock { lockFile = ./android/Cargo.lock; };
        nativeBuildInputs = common.nativeBuildInputs ++ [ pkgs.zip ];
        buildPhase = ''
          runHook preBuild
          export HOME=$TMPDIR
          ${forEachPart (part: ''
            (
              ${environmentWith nightjar part}
              cargo build --locked --release --offline --lib --target ${part.target}${features}
            )
            mkdir -p stage/lib/${part.name}
            cp target/${part.target}/release/${library} stage/lib/${part.name}/
            ${readElf part sessionLibraries "stage/lib/${part.name}/${library}" "elf-${part.name}.txt"}
            grep -q 'Shared library' elf-${part.name}.txt
            "${toolchain}/bin/llvm-nm" --defined-only --extern-only stage/lib/${part.name}/${library} \
              | grep -q ' T ANativeActivity_onCreate$'
          '')}
          ${buildTools}/aapt2 link -o unsigned.apk -I ${framework} \
            --manifest AndroidManifest.xml --debug-mode
          # Fixed times and no extra attributes, as aapt2 writes its entries.
          find stage -exec touch -h -d @315532800 {} +
          (cd stage && zip -q -r -X ../unsigned.apk lib)
          ${buildTools}/zipalign -p -f 4 unsigned.apk aligned.apk
          runHook postBuild
        '';
        installPhase = ''
          runHook preInstall
          mkdir -p $out
          cp aligned.apk $out/korri-viewer-unsigned.apk
          cp elf-*.txt $out/
          runHook postInstall
        '';
      }
    );
  apk = apkWith false;
  nightjarApk = apkWith true;
  # The proof's refusal probe (android/proof/refusal): an ordinary app that
  # asks Android to start the viewer's activity. x86_64 only, for the
  # emulator; never shipped. Unsigned, as the APK above.
  refusalProbe =
    let
      part = parts.x86_64;
    in
    pkgs.stdenv.mkDerivation (
      common
      // {
        pname = "korri-viewer-refusal-probe-apk";
        version = "0.1.0";
        src = lib.fileset.toSource {
          root = ./android/proof/refusal;
          fileset = ./android/proof/refusal;
        };
        cargoDeps = pkgs.rustPlatform.importCargoLock {
          lockFile = ./android/proof/refusal/Cargo.lock;
        };
        nativeBuildInputs = common.nativeBuildInputs ++ [ pkgs.zip ];
        buildPhase = ''
          runHook preBuild
          export HOME=$TMPDIR
          ${environmentFor part}
          cargo build --locked --release --offline --lib --target ${part.target}
          mkdir -p stage/lib/${part.name}
          cp target/${part.target}/release/libkorri_viewer_refusal_probe.so stage/lib/${part.name}/
          ${buildTools}/aapt2 link -o unsigned.apk -I ${framework} \
            --manifest AndroidManifest.xml --debug-mode
          find stage -exec touch -h -d @315532800 {} +
          (cd stage && zip -q -r -X ../unsigned.apk lib)
          ${buildTools}/zipalign -p -f 4 unsigned.apk aligned.apk
          runHook postBuild
        '';
        installPhase = ''
          runHook preInstall
          mkdir -p $out
          cp aligned.apk $out/korri-viewer-refusal-probe-unsigned.apk
          runHook postInstall
        '';
      }
    );
  # The proof's feedback probe (android/proof/feedback): it drives the
  # viewer's controller feedback (slice D4b) against a virtual pad, because
  # the proof's host sends no rumble and no light. It links the shared viewer
  # as the app does. x86_64 only, for the emulator; never shipped. Unsigned,
  # as the APK above.
  feedbackProbe =
    let
      part = parts.x86_64;
      probeLibrary = "libkorri_viewer_feedback_probe.so";
    in
    pkgs.stdenv.mkDerivation (
      common
      // {
        pname = "korri-viewer-feedback-probe-apk";
        version = "0.1.0";
        src = lib.fileset.toSource {
          root = ../..;
          fileset = lib.fileset.unions [
            sourceFiles
            ./android/proof/feedback
          ];
        };
        sourceRoot = "source/clients/viewer/android/proof/feedback";
        cargoDeps = pkgs.rustPlatform.importCargoLock {
          lockFile = ./android/proof/feedback/Cargo.lock;
        };
        nativeBuildInputs = common.nativeBuildInputs ++ [ pkgs.zip ];
        buildPhase = ''
          runHook preBuild
          export HOME=$TMPDIR
          (
            ${environmentFor part}
            cargo build --locked --release --offline --lib --target ${part.target}
          )
          mkdir -p stage/lib/${part.name}
          cp target/${part.target}/release/${probeLibrary} stage/lib/${part.name}/
          ${readElf part sessionLibraries "stage/lib/${part.name}/${probeLibrary}" "elf-${part.name}.txt"}
          ${buildTools}/aapt2 link -o unsigned.apk -I ${framework} \
            --manifest AndroidManifest.xml --debug-mode
          find stage -exec touch -h -d @315532800 {} +
          (cd stage && zip -q -r -X ../unsigned.apk lib)
          ${buildTools}/zipalign -p -f 4 unsigned.apk aligned.apk
          runHook postBuild
        '';
        installPhase = ''
          runHook preInstall
          mkdir -p $out
          cp aligned.apk $out/korri-viewer-feedback-probe-unsigned.apk
          runHook postInstall
        '';
      }
    );
in
{
  inherit
    parts
    check
    apk
    nightjarCheck
    nightjarApk
    refusalProbe
    feedbackProbe
    ;
}
