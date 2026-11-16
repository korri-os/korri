# The release APK's notices (#434 D12): one UTF-8 text with the licence of
# each part of the APK that other people wrote. default.nix opens it with the
# app's own licence and its source commit, and packs it as
# assets/notices/NOTICES.txt.
#
# Its parts, in order:
# - what cargo does not build (the viewer's C parts, Skia's archive and what
#   it holds, the dexed Kotlin, the NDK's C++ library, Rust's standard
#   library), and the C code that a crate bundles and builds;
# - the surface packages' fonts and Pico's vendored scripts;
# - the Rust crates of the arm64 library, by cargo-about (about.toml);
# - the GNU GPL version 3, the licence of the app as a whole.
#
# Each licence text comes from its project's own source. The build fails
# when one is missing, so a moved file cannot leave a notice out.
{
  pkgs,
  src,
  cargoDeps,
  rust,
  surfaces,
  # The viewer's C parts for arm64 (clients/viewer/android.nix).
  viewerParts,
  skiaArchive,
  skiaVersion,
  kotlinVersion,
  ndk,
  target,
}:
let
  inherit (pkgs) lib;
  lock = builtins.fromTOML (builtins.readFile ./Cargo.lock);
  # A crate's directory in the vendored sources, at the version that
  # Cargo.lock pins.
  vendored =
    name:
    let
      crate = lib.findFirst (crate: crate.name == name) (throw "Cargo.lock has no ${name}") lock.package;
    in
    "${cargoDeps}/${name}-${crate.version}";
  curlSys = vendored "curl-sys";
  # Skia's archive carries only Skia's own licence. Of the libraries built
  # into it (libjpeg-turbo, libpng, zlib, FreeType, Expat, Wuffs, Vulkan
  # Memory Allocator), the arm64 library keeps the code of four: libpng,
  # zlib, FreeType and Expat, read from its symbols (README). Their texts
  # come from each project's source in nixpkgs. Skia pins its own revisions
  # of them, so a copyright year can differ.
  skiaParts = {
    inherit (pkgs)
      libpng
      freetype
      expat
      ;
  };
  # Skia's zlib is Chromium's copy, whose SIMD parts are under Chromium's
  # licence. Both texts at one Chromium release.
  chromium = "140.0.7339.80";
  chromiumZlibLicence = pkgs.fetchurl {
    name = "chromium-${chromium}-zlib-LICENSE";
    url = "https://raw.githubusercontent.com/chromium/chromium/${chromium}/third_party/zlib/LICENSE";
    hash = "sha256-4c/MVcMls/eM9V35Zkq6oGbiJx3/6CEzR9n8zfusjyw=";
  };
  chromiumLicence = pkgs.fetchurl {
    name = "chromium-${chromium}-LICENSE";
    url = "https://raw.githubusercontent.com/chromium/chromium/${chromium}/LICENSE";
    hash = "sha256-NozKEQa+mdOezTKjjYMFWF2AKkde/7ZjgLkf/JvPcJs=";
  };
  # The Kotlin standard library's jar holds no licence file. Its project's
  # licence, its NOTICE, and the Boost licence of the code that
  # libraries/stdlib/jvm/src/kotlin/util/MathJVM.kt derives from boost
  # (license/README.md), at the release that default.nix dexes.
  kotlinLicence =
    file: hash:
    pkgs.fetchurl {
      name = "kotlin-${kotlinVersion}-${baseNameOf file}";
      url = "https://raw.githubusercontent.com/JetBrains/kotlin/v${kotlinVersion}/license/${file}";
      inherit hash;
    };
  kotlin = [
    (kotlinLicence "LICENSE.txt" "sha256-z8d0m5b2O9McPEK1xHG/dWgUBT6EfBDz6wA0F7xSPTA=")
    (kotlinLicence "NOTICE.txt" "sha256-CwmoPT73eVx97GXoFYZll8BmxTiY8oUqdlmbj1VSlBo=")
    (kotlinLicence "third_party/boost_LICENSE.txt" "sha256-jYKRyvHO4m0jrPPrZ8n5otWPHGgbFqT76Mv7njwLWps=")
  ];
  moonlight = viewerParts.network.src;
in
pkgs.stdenv.mkDerivation {
  pname = "korri-android-app-notices";
  version = "0.1.0";
  inherit src cargoDeps;
  sourceRoot = "korrid-source-with-proseql/clients/android-app";
  nativeBuildInputs = [
    rust
    pkgs.rustPlatform.cargoSetupHook
    pkgs.cargo-about
    pkgs.bun
    pkgs.python3
  ];
  dontConfigure = true;
  buildPhase = ''
    runHook preBuild
    export HOME=$TMPDIR
    cargo about generate --frozen --fail --target ${target} \
      -c ${./about.toml} -o crates.txt ${./about.hbs}

    # A licence file, which must be there and not be empty.
    licence() {
      if [ ! -s "$1" ]; then
        echo "notices: no licence text at $1" >&2
        exit 1
      fi
      printf '\n'
      cat "$1"
    }
    # A part: its title, what it is and where it is, then its licence files.
    part() {
      local title="$1" about="$2"
      shift 2
      printf '\n================================================================================\n%s\n\n%s\n' "$title" "$about" |
        fold -s -w 80 | sed 's/[[:space:]]*$//'
      for file in "$@"; do licence "$file"; done
    }
    # The one licence file `name` inside the source tarball `archive`.
    unpacked() {
      local archive="$1" name="$2" directory
      directory=$(mktemp -d)
      tar -xf "$archive" -C "$directory" --wildcards "*/$name"
      find "$directory" -path "*/$name" | head -n 1
    }

    {
      printf 'PARTS THAT CARGO DOES NOT BUILD, AND C CODE INSIDE CRATES\n'

      part 'moonlight-common-c ${viewerParts.network.version}' \
        "The stream's network core, from Moonlight Embedded, linked unmodified into the library. GNU General Public License version 3; the text is at the end of this file."
      part 'ENet' \
        'The network library inside moonlight-common-c.' \
        ${moonlight}/enet/LICENSE
      # The licence is the head comment of the file.
      sed -n '1,/\*\//p' ${moonlight}/reedsolomon/rs.c > reed-solomon.txt
      part 'Reed-Solomon forward error correction' \
        "The FEC code inside moonlight-common-c (reedsolomon/rs.c)." \
        reed-solomon.txt
      # The notice of the data that Korri's viewer took from Valve and SDL,
      # inside its source file's head comment.
      sed -n '/^\/\/! ```text$/,/^\/\/! ```$/p' ../viewer/src/android/controller_list.rs |
        grep -v '```' | sed 's|^//! \{0,1\}||' > controller-list.txt
      part "Valve's and SDL's controller lists" \
        "Data in Korri's viewer (clients/viewer/src/android/controller_list.rs), taken from Valve's controller_list.h and SDL's minisdl.c and changed by Korri." \
        controller-list.txt
      part 'OpenSSL ${viewerParts.openssl.version}' \
        "The viewer's cryptography and TLS, from nixpkgs' source, linked statically." \
        "$(unpacked ${viewerParts.openssl.src} LICENSE.txt)"
      part 'Opus ${viewerParts.opus.version}' \
        "The viewer's sound decoder (libopus), from nixpkgs' source, linked statically." \
        "$(unpacked ${viewerParts.opus.src} COPYING)"
      part 'curl' \
        'The copy of curl that the crate curl-sys builds and links statically (${baseNameOf curlSys}).' \
        ${curlSys}/curl/COPYING
      part 'QuickJS' \
        'The JavaScript engine that the crate rquickjs-sys builds (${baseNameOf (vendored "rquickjs-sys")}).' \
        ${vendored "rquickjs-sys"}/quickjs/LICENSE
      part 'libsecp256k1' \
        'The elliptic curve library that the crate secp256k1-sys builds (${baseNameOf (vendored "secp256k1-sys")}).' \
        ${vendored "secp256k1-sys"}/depend/secp256k1/COPYING
      part 'ring' \
        'The C and assembly code from BoringSSL that the crate ring builds (${baseNameOf (vendored "ring")}).' \
        ${vendored "ring"}/LICENSE ${vendored "ring"}/LICENSE-BoringSSL ${vendored "ring"}/LICENSE-other-bits
      skia=$(mktemp -d)
      tar -xzf ${skiaArchive} -C "$skia"
      part 'Skia' \
        "The 2D graphics library under Slint's renderer: the prebuilt archive of rust-skia's release ${skiaVersion}." \
        "$skia/skia-binaries/LICENSE_SKIA"
      part 'libpng' \
        "Built into Skia's archive. The text is libpng ${skiaParts.libpng.version}'s." \
        "$(unpacked ${skiaParts.libpng.src} LICENSE)"
      part 'zlib' \
        "Chromium's copy of zlib, built into Skia's archive. Its SIMD parts are under Chromium's licence, which follows the zlib licence. The texts are Chromium ${chromium}'s." \
        ${chromiumZlibLicence} ${chromiumLicence}
      part 'FreeType' \
        "Built into Skia's archive, used under the FreeType License. Portions of this software are copyright The FreeType Project (www.freetype.org). All rights reserved. The text is FreeType ${skiaParts.freetype.version}'s." \
        "$(unpacked ${skiaParts.freetype.src} docs/FTL.TXT)"
      part 'Expat' \
        "Built into Skia's archive. The text is Expat ${skiaParts.expat.version}'s." \
        "$(unpacked ${skiaParts.expat.src} COPYING)"
      part 'Kotlin standard library ${kotlinVersion}' \
        "Dexed unchanged into the library with rustls-platform-verifier's Kotlin component, whose licence is the crate rustls-platform-verifier-android's, under the Rust crates below." \
        ${lib.concatStringsSep " " kotlin}
      # The first of the NDK toolchain's notices is the LLVM Project's licence.
      awk '/^The LLVM Project is under the Apache License v2.0 with LLVM Exceptions:/ { n++ } n == 1' \
        ${ndk}/toolchains/llvm/prebuilt/linux-x86_64/NOTICE > llvm.txt
      grep -q '^---- LLVM Exceptions to the Apache 2.0 License ----' llvm.txt
      part 'LLVM libc++ and compiler-rt' \
        "The Android NDK's C++ standard library and compiler runtime, linked statically. The LLVM exceptions in their licence apply." \
        llvm.txt
      # Rust ships its standard library's notices as HTML only.
      python3 ${./html-notice.py} ${rust}/share/doc/rust/COPYRIGHT-library.html > rust-std.txt
      grep -q '^Copyright notices for The Rust Standard Library$' rust-std.txt
      part 'The Rust standard library ${rust.version}' \
        "Rust's core, alloc and std, and the crates they are built from, compiled into the library by the toolchain that built it. Rust's own notice for this release follows." \
        rust-std.txt
      printf '\n'
      printf "Slint's Java helper (SlintAndroidJavaHelper) is part of the crate\ni-slint-backend-android-activity, under the Rust crates below.\n"

      printf '\n\nFONTS AND SCRIPTS IN THE SURFACE PACKAGES\n'
      printf "\nPico's faces (clients/shell/packages/pico/src/pico-fonts.ts):\n\n"
      bun -e '
        const { PICO_FONTS } = await import(process.argv[1]);
        for (const font of PICO_FONTS) console.log(`  ''${font.name}, by ''${font.by}: ''${font.license}`);
      ' ${surfaces}/packages/pico/src/pico-fonts.ts
      find ${surfaces}/packages/{pico,velvet,zone}/fonts -maxdepth 1 -type f \
        \( -name '*OFL*.txt' -o -name '*LICENSE*.txt' \) | sort > font-licences.txt
      test -s font-licences.txt
      while read -r file; do
        part "''${file#${surfaces}/packages/}" 'A font licence, which also ships beside its font.' "$file"
      done < font-licences.txt
      part 'qrcode' "Pico's QR code encoder, vendored unchanged." \
        ${surfaces}/packages/pico/src/vendor/qrcode/LICENSE
      part 'dijkstrajs' "Pico's shortest-path search for the QR encoder, vendored unchanged." \
        ${surfaces}/packages/pico/src/vendor/dijkstrajs/LICENSE.md

      printf '\n\nRUST CRATES IN THE LIBRARY\n'
      cat crates.txt

      printf '\n================================================================================\n'
      printf 'GNU GENERAL PUBLIC LICENSE VERSION 3\n\nThe licence of the app as a whole, and of moonlight-common-c.\n'
      licence ${moonlight}/LICENSE.txt
    } > NOTICES.txt
    runHook postBuild
  '';
  installPhase = ''
    runHook preInstall
    install -D -m 0644 NOTICES.txt $out/NOTICES.txt
    runHook postInstall
  '';
}
