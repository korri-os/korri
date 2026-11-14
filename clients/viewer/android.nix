# The shared session part on Android (#391 step D) at API 31 (ADR 0064),
# with the NDK that clients/android uses. Two ABIs: arm64-v8a for phones,
# x86_64 for the emulator.
#
# - check: builds the library and links the HTTPS and trust-anchor test
#   executables for each ABI, then reads them. Nothing runs here.
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

  abis = {
    arm64-v8a = {
      target = "aarch64-linux-android";
      opensslTarget = "android-arm64";
      machine = "AArch64";
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

  parts = lib.mapAttrs (
    name: abi:
    let
      openssl = opensslFor name abi;
    in
    abi
    // {
      inherit name openssl;
      network = networkFor name openssl;
    }
  ) abis;

  # One cargo invocation's environment for one ABI. The core's path is not
  # scoped by target in build.rs, so each ABI gets its own invocation.
  environmentFor =
    part:
    let
      # Shell names cannot hold "-"; cc-rs and bindgen also read the "_" form.
      target = lib.replaceStrings [ "-" ] [ "_" ] part.target;
      cargoTarget = lib.toUpper target;
      cc = "${toolchain}/bin/${part.target}${api}-clang";
    in
    ''
      export MOONLIGHT_COMMON_INCLUDE=${part.network}/include
      export MOONLIGHT_COMMON_LIB=${part.network}/lib
      export ${cargoTarget}_OPENSSL_DIR=${part.openssl}
      export ${cargoTarget}_OPENSSL_STATIC=1
      export CC_${target}=${cc}
      export AR_${target}=${toolchain}/bin/llvm-ar
      export CARGO_TARGET_${cargoTarget}_LINKER=${cc}
      # 16 KiB load segments, as clients/android/build.sh links its libraries.
      export CARGO_TARGET_${cargoTarget}_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
      export BINDGEN_EXTRA_CLANG_ARGS_${target}="--target=${part.target}${api} --sysroot=${toolchain}/sysroot"
    '';
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
  ];

  check = pkgs.stdenv.mkDerivation (
    common
    // {
      name = "korri-viewer-android-check";
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
            ${environmentFor part}
            cargo build --locked --release --offline --lib --target ${part.target}
            # The HTTPS test drives Transport and the launch chain into the
            # core; linking it proves that every symbol of that path resolves.
            cargo test --locked --release --offline --no-run --target ${part.target} \
              --lib --test https --test trust_anchor --message-format=json > tests-${part.name}.json
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
          https=$out/${part.name}/tests/https
          ${readElf part sessionLibraries "$https" "$out/${part.name}/https-elf.txt"}
          grep -q 'Requesting program interpreter: /system/bin/linker64' $out/${part.name}/https-elf.txt
          # curl, OpenSSL and the core are inside the executable. The crate's
          # curl hides its symbols, so they are local (t), not global.
          "${toolchain}/bin/llvm-nm" --defined-only "$https" > symbols.txt
          for symbol in LiStartConnection LiStopConnection curl_multi_perform SSL_CTX_new EVP_aes_128_gcm viewer_android_log; do
            grep -Eq " [Tt] $symbol$" symbols.txt || { echo "missing $symbol" >&2; exit 1; }
            echo "$symbol" >> $out/${part.name}/symbols.txt
          done
        '')}
        runHook postInstall
      '';
    }
  );

in
{
  inherit parts check;
}
