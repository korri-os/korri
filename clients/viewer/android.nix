# The shared session part on Android (#391 step D, slice D0): the launch
# chain, pinned HTTPS, pairing and the network-core binding, built for
# aarch64-linux-android at API 31 (ADR 0064) with the NDK that clients/android
# uses. Nothing runs on a device or an emulator: the check builds the library,
# links a test executable that exercises it, and reads the result.
{
  pkgs,
  upstream,
  repositorySource,
  cargoDeps,
}:
let
  sdk = import ../android/sdk.nix { inherit pkgs; };
  api = "31";
  target = "aarch64-linux-android";
  cargoTarget = "AARCH64_LINUX_ANDROID";
  ndk = sdk.ndkRoot;
  toolchain = "${ndk}/toolchains/llvm/prebuilt/linux-x86_64";
  cc = "${toolchain}/bin/${target}${api}-clang";
  rust = pkgs.rust-bin.stable.latest.default.override { targets = [ target ]; };

  # The OpenSSL release that the Linux viewer links (nixpkgs' source, without
  # its Nix certificate-path patches), as static archives. The network core
  # needs libcrypto on Android as on Linux; the session's openssl and curl
  # crates link this same copy, so the binary holds one OpenSSL.
  openssl = pkgs.stdenv.mkDerivation {
    pname = "korri-viewer-android-openssl";
    inherit (pkgs.openssl) version src;
    nativeBuildInputs = [ pkgs.perl ];
    dontStrip = true;
    configurePhase = ''
      runHook preConfigure
      export ANDROID_NDK_ROOT=${ndk} PATH=${toolchain}/bin:$PATH
      unset CC CXX AR RANLIB LD
      perl ./Configure android-arm64 -D__ANDROID_API__=${api} \
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
  network = pkgs.stdenv.mkDerivation {
    pname = "korri-viewer-android-moonlight-common-c";
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
        -DANDROID_ABI=arm64-v8a -DANDROID_PLATFORM=android-${api} \
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

  check = pkgs.stdenv.mkDerivation {
    name = "korri-viewer-android-check";
    src = repositorySource;
    sourceRoot = "source/clients/viewer";
    inherit cargoDeps;
    nativeBuildInputs = [
      rust
      pkgs.rustPlatform.cargoSetupHook
      pkgs.perl
    ];
    env = {
      MOONLIGHT_COMMON_INCLUDE = "${network}/include";
      MOONLIGHT_COMMON_LIB = "${network}/lib";
      "${cargoTarget}_OPENSSL_DIR" = "${openssl}";
      "${cargoTarget}_OPENSSL_STATIC" = "1";
      "CC_${target}" = cc;
      "AR_${target}" = "${toolchain}/bin/llvm-ar";
      "CARGO_TARGET_${cargoTarget}_LINKER" = cc;
      # 16 KiB load segments, as clients/android/build.sh links its libraries.
      "CARGO_TARGET_${cargoTarget}_RUSTFLAGS" = "-C link-arg=-Wl,-z,max-page-size=16384";
      "BINDGEN_EXTRA_CLANG_ARGS_${target}" = "--target=${target}${api} --sysroot=${toolchain}/sysroot";
      LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
    };
    buildPhase = ''
      runHook preBuild
      export HOME=$TMPDIR
      cargo build --locked --release --offline --lib --target ${target}
      # The HTTPS test drives Transport and the launch chain into the core;
      # linking it proves that every symbol of that path resolves.
      cargo test --locked --release --offline --no-run --target ${target} --lib --test https \
        --message-format=json > tests.json
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      mkdir -p $out
      readelf=${toolchain}/bin/llvm-readelf
      nm=${toolchain}/bin/llvm-nm
      library=target/${target}/release/libkorri_viewer.rlib
      test -s "$library"
      https=$(grep -o '"executable":"[^"]*/https-[^"]*"' tests.json | cut -d'"' -f4)
      test -x "$https"
      "$readelf" --file-header --program-headers --dynamic "$https" > $out/https-elf.txt
      grep -q 'Machine: *AArch64' $out/https-elf.txt
      grep -q 'Requesting program interpreter: /system/bin/linker64' $out/https-elf.txt
      # curl, OpenSSL and the core are inside the executable: an app needs no
      # library beyond Android's own.
      grep NEEDED $out/https-elf.txt | sed 's/.*\[\(.*\)\]/\1/' | sort > $out/needed.txt
      cat $out/needed.txt
      if grep -v -x -e libc.so -e libm.so -e libdl.so -e libz.so $out/needed.txt; then
        echo 'needs a library that Android does not ship' >&2
        exit 1
      fi
      # The crate's curl hides its symbols, so they are local (t), not global.
      "$nm" --defined-only "$https" > symbols.txt
      for symbol in LiStartConnection LiStopConnection curl_multi_perform SSL_CTX_new EVP_aes_128_gcm; do
        grep -Eq " [Tt] $symbol$" symbols.txt || { echo "missing $symbol" >&2; exit 1; }
        echo "$symbol" >> $out/symbols.txt
      done
      runHook postInstall
    '';
    dontStrip = true;
    dontFixup = true;
  };
in
{
  inherit openssl network check;
}
