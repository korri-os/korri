{ pkgs }:
let
  base = pkgs.fetchurl {
    url = "https://rootfs.fex-emu.gg/ArchLinux/2026-01-08/ArchLinux.sqsh";
    sha256 = "142y1i5qiv47kha6ljmifjhzk5lv31arclj5b0bdjflmnxrrj1fb";
  };
  overlay = [
    (pkgs.fetchurl {
      url = "https://archive.archlinux.org/packages/m/mesa/mesa-1%3A26.1.2-1-x86_64.pkg.tar.zst";
      sha256 = "be9cdb7a2a1c4eb096f57328c955b018950946edfdbb7f04ef697d4999e5a808";
    })
    (pkgs.fetchurl {
      url = "https://archive.archlinux.org/packages/l/lib32-mesa/lib32-mesa-1%3A26.1.2-1-x86_64.pkg.tar.zst";
      sha256 = "a32b6c7f956ac6ddff53126d9717643d6edf5215d4279b7a84f72a5908a42c71";
    })
    (pkgs.fetchurl {
      url = "https://archive.archlinux.org/packages/v/vulkan-freedreno/vulkan-freedreno-1%3A26.1.2-1-x86_64.pkg.tar.zst";
      sha256 = "7db14d6e8e6926b31de09716dfd03451c22e348da8b1ea35f5438fbb9a618dee";
    })
    (pkgs.fetchurl {
      url = "https://archive.archlinux.org/packages/l/lib32-vulkan-freedreno/lib32-vulkan-freedreno-1%3A26.1.2-1-x86_64.pkg.tar.zst";
      sha256 = "98793dac11cefb40b43f4e9007d0c183b523dbf6c340020d4102b41228958a6f";
    })
    (pkgs.fetchurl {
      url = "https://archive.archlinux.org/packages/v/vulkan-mesa-implicit-layers/vulkan-mesa-implicit-layers-1%3A26.1.2-1-x86_64.pkg.tar.zst";
      sha256 = "e708f001c78a220e87eeaf42df022664df6ced4963bf61d4a22528d21fe61292";
    })
    (pkgs.fetchurl {
      url = "https://archive.archlinux.org/packages/l/lib32-vulkan-mesa-implicit-layers/lib32-vulkan-mesa-implicit-layers-1%3A26.1.2-1-x86_64.pkg.tar.zst";
      sha256 = "a9e2ca9c2df8fa5897d3baeadb298cd2969a229539119c05ff436550246915b7";
    })
    (pkgs.fetchurl {
      url = "https://archive.archlinux.org/packages/l/libdisplay-info/libdisplay-info-0.3.0-1-x86_64.pkg.tar.zst";
      sha256 = "07ab1479ec5e70aea174e2d0e340bd9364b507ed74f2df3bfd51b3c6f40e5787";
    })
    (pkgs.fetchurl {
      url = "https://archive.archlinux.org/packages/l/lib32-libdisplay-info/lib32-libdisplay-info-0.3.0-1-x86_64.pkg.tar.zst";
      sha256 = "5ffcb2f1e599529014beaadff257ee805ecdf3991948e33ebf398e202ce135f0";
    })
    (pkgs.fetchurl {
      url = "https://archive.archlinux.org/packages/x/xcb-util-keysyms/xcb-util-keysyms-0.4.1-5-x86_64.pkg.tar.zst";
      sha256 = "312213911881c0742e2a825432feac761e863945b74bb3a6daca97a99778d35e";
    })
    (pkgs.fetchurl {
      url = "https://archive.archlinux.org/packages/l/lib32-xcb-util-keysyms/lib32-xcb-util-keysyms-0.4.1-2-x86_64.pkg.tar.zst";
      sha256 = "62940da026cc3ca714d6989a542edfe3aaf7ec8b5c148fc9007e2e3b9ec3f5a6";
    })
  ];
in
pkgs.runCommand "steam-fex-rootfs-archlinux-2026-01-08-mesa-26.1.2"
  {
    nativeBuildInputs = [
      pkgs.squashfsTools
      pkgs.gnutar
      pkgs.zstd
    ];
    # This is an x86 filesystem, not a set of native ELF objects to patch/strip.
    dontFixup = true;
  }
  ''
    unsquashfs -no-progress -no-xattrs -d "$out" ${base}
    chmod -R u+w "$out"
    for package in ${pkgs.lib.escapeShellArgs overlay}; do
      tar --zstd --no-same-owner \
        --exclude='.BUILDINFO' --exclude='.CHANGELOG' --exclude='.INSTALL' \
        --exclude='.MTREE' --exclude='.PKGINFO' -xf "$package" -C "$out"
    done
    test -f "$out/usr/lib/libgallium-26.1.2-arch1.1.so"
    test -f "$out/usr/lib32/libgallium-26.1.2-arch1.1.so"
    for arch in x86_64 i686; do
      test -f "$out/usr/share/vulkan/icd.d/freedreno_icd.$arch.json"
    done
    for lib in libdisplay-info.so.3 libxcb-keysyms.so.1; do
      test -f "$out/usr/lib/$lib"
      test -f "$out/usr/lib32/$lib"
    done
    case "$(od -An -tx1 -j18 -N2 "$out/usr/lib/libvulkan_freedreno.so")" in
      *"3e 00"*) ;; *) echo 'Freedreno ICD is not x86_64' >&2; exit 1 ;;
    esac
    case "$(od -An -tx1 -j18 -N2 "$out/usr/lib32/libvulkan_freedreno.so")" in
      *"03 00"*) ;; *) echo 'Freedreno ICD is not i686' >&2; exit 1 ;;
    esac
  ''
