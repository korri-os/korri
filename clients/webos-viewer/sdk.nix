{ pkgs }:
let
  archive = pkgs.fetchurl {
    url = "https://github.com/openlgtv/buildroot-nc4/releases/download/2026.08-webos/arm-webos-linux-gnueabi_sdk-buildroot-x86_64.tar.gz";
    sha256 = "2230a6351e5cce6fcb9d4fdcb4697335f223995401ef969c81744aaeb14c5344";
  };
  # Same prebuilt SDK and host-loader preparation as slint-spike/webos-cross.sh.
  # This is a private immutable Nix extraction, not a shared SDK mutation. No
  # TV libc overlay is needed for this C/C++ link experiment (SDK glibc 2.12.2).
  sdk = pkgs.stdenvNoCC.mkDerivation {
    name = "korri-webos-viewer-spike-sdk";
    dontUnpack = true;
    dontFixup = true;
    nativeBuildInputs = [
      pkgs.patchelf
      pkgs.file
    ];
    buildCommand = ''
      mkdir -p "$out"
      tar xzf ${archive} --strip-components=1 -C "$out"
      (cd "$out" && bash ./relocate-sdk.sh)
      while IFS= read -r -d "" executable; do
        if file -b "$executable" | grep -q 'ELF 64-bit.*x86-64.*interpreter'; then
          patchelf --set-interpreter ${pkgs.stdenv.cc.libc}/lib/ld-linux-x86-64.so.2 "$executable"
          existing="$(patchelf --print-rpath "$executable")"
          patchelf --set-rpath "${
            pkgs.lib.makeLibraryPath [
              pkgs.stdenv.cc.cc.lib
              pkgs.stdenv.cc.libc
              pkgs.zlib
              pkgs.expat
            ]
          }''${existing:+:$existing}" "$executable"
        fi
      done < <(find "$out/bin" "$out/libexec" "$out/arm-webos-linux-gnueabi/bin" -type f -perm /111 -print0)
    '';
  };
in
sdk
