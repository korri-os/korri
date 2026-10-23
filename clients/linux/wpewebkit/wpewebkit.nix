# Vendored from eval-exec/nix-wpe-webkit at
# f00a6fa7b7fe678bba8588edc21e233ac1eedce8 (pkgs/wpewebkit.nix).
# Dependencies come from Korri's pinned nixpkgs, not the upstream flake.
# Version 2.52.6 and clangStdenv follow the pinned nixpkgs webkitgtk_6_0
# (same 2.52.6 base): 2.52.3's WebKitMacros.cmake does not configure with the
# pinned CMake 4.4 ("if given arguments ... Unknown arguments specified").
{
  addDriverRunpath,
  at-spi2-core,
  bison,
  bubblewrap,
  cairo,
  cmake,
  expat,
  fetchurl,
  flex,
  fontconfig,
  freetype,
  gettext,
  glib,
  gperf,
  gst_all_1,
  harfbuzz,
  hyphen,
  icu,
  lcms2,
  lib,
  libavif,
  libdrm,
  libedit,
  libepoxy,
  libgbm,
  libgcrypt,
  libgpg-error,
  libinput,
  libjpeg,
  libjxl,
  libpng,
  libseccomp,
  libsoup_3,
  libsysprof-capture,
  libtasn1,
  libwebp,
  libwpe,
  libwpe-fdo,
  libxkbcommon,
  libxml2,
  libxslt,
  mesa,
  ninja,
  p11-kit,
  pcre2,
  perl,
  pkg-config,
  python3,
  replaceVars,
  ruby,
  sqlite,
  clangStdenv,
  systemd,
  testers,
  unifdef,
  wayland,
  wayland-protocols,
  wayland-scanner,
  woff2,
  xdg-dbus-proxy,
  zlib,
}:

clangStdenv.mkDerivation (finalAttrs: {
  pname = "wpewebkit";
  version = "2.52.6";

  outputs = [
    "out"
    "dev"
  ];

  outputBin = "dev";

  src = fetchurl {
    url = "https://wpewebkit.org/releases/wpewebkit-${finalAttrs.version}.tar.xz";
    hash = "sha256-srr+8nUWJbf99TDyMP8PVC/w7ro1kMOpidkxsqVchY4=";
  };

  # From nixpkgs 549bd84d6279f9852cae6225e372cc67fb91a4c1:
  # the shared GTK/WPE launcher must see Nix libraries inside its sandbox.
  patches = [
    ./prefer-gpu-render-node.patch
    (replaceVars ./fix-bubblewrap-paths.patch {
      inherit (builtins) storeDir;
      inherit (addDriverRunpath) driverLink;
    })
  ];

  nativeBuildInputs = [
    bison
    cmake
    flex
    gettext
    gperf
    libsysprof-capture
    ninja
    perl
    perl.pkgs.FileCopyRecursive # copy-user-interface-resources.pl, as in nixpkgs webkitgtk
    glib # gdbus-codegen
    pkg-config
    python3
    ruby
    unifdef
    wayland-protocols
    wayland-scanner
  ];

  postPatch = ''
    patchShebangs .
  '';

  requiredSystemFeatures = [ "big-parallel" ];

  buildInputs = [
    at-spi2-core
    bubblewrap
    cairo
    expat
    fontconfig
    freetype
    glib
    gst_all_1.gst-plugins-bad
    gst_all_1.gst-plugins-base
    gst_all_1.gstreamer
    icu
    (harfbuzz.override { withIcu = true; })
    hyphen
    lcms2
    libavif
    libdrm
    libedit
    libepoxy
    libgbm
    libgcrypt
    libgpg-error
    libinput
    libjpeg
    libjxl
    libpng
    libseccomp
    libsoup_3
    libtasn1
    libwebp
    libwpe
    libwpe-fdo
    libxkbcommon
    libxml2
    libxslt
    mesa
    p11-kit
    pcre2
    sqlite
    systemd
    wayland
    woff2
    xdg-dbus-proxy
    zlib
  ];

  cmakeFlags = [
    (lib.cmakeFeature "PORT" "WPE")
    (lib.cmakeFeature "BWRAP_EXECUTABLE" (lib.getExe bubblewrap))
    (lib.cmakeFeature "DBUS_PROXY_EXECUTABLE" (lib.getExe xdg-dbus-proxy))
    (lib.cmakeBool "ENABLE_BUBBLEWRAP_SANDBOX" true)
    (lib.cmakeBool "ENABLE_DOCUMENTATION" false)
    (lib.cmakeBool "ENABLE_INTROSPECTION" false)
    (lib.cmakeBool "ENABLE_MINIBROWSER" true)
    (lib.cmakeBool "ENABLE_SPEECH_SYNTHESIS" false)
    (lib.cmakeBool "ENABLE_WPE_PLATFORM" true)
    (lib.cmakeBool "USE_LIBBACKTRACE" false)
  ];

  passthru.tests = {
    pkg-config = testers.hasPkgConfigModules {
      package = finalAttrs.finalPackage;
    };

    simple-compilation = clangStdenv.mkDerivation {
      name = "wpewebkit-test-simple-compilation";

      nativeBuildInputs = [ pkg-config ];
      buildInputs = [
        finalAttrs.finalPackage
        glib
        libwpe
        libsoup_3
      ];

      dontUnpack = true;

      buildPhase = ''
        cat > test.c <<EOF
        #include <wpe/webkit.h>
        #include <wpe/wpe-platform.h>
        #include <stdio.h>

        int main(void) {
            printf("WPE WebKit version: %d.%d.%d\n",
                   webkit_get_major_version(),
                   webkit_get_minor_version(),
                   webkit_get_micro_version());
            return 0;
        }
        EOF

        $CC test.c -o test $(pkg-config --cflags --libs wpe-webkit-2.0 wpe-platform-2.0)
      '';

      installPhase = ''
        mkdir -p $out/bin
        install -m755 test $out/bin/wpewebkit-test
      '';
    };
  };

  meta = {
    description = "WPE WebKit port optimized for embedded devices";
    homepage = "https://wpewebkit.org";
    license = lib.licenses.bsd2;
    maintainers = [
      {
        name = "Eval Exec";
        email = "execvy@gmail.com";
        github = "eval-exec";
      }
    ];
    platforms = lib.platforms.linux;
    pkgConfigModules = [
      "wpe-webkit-2.0"
      "wpe-platform-2.0"
    ];
  };
})
