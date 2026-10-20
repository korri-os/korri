// FEX runtime files for runners that emulate x86 code. This plugin declares
// no runner and no handler. Its native files are declared in plugin.nix; a
// runner that requires this plugin reads them from its launch input under
// `@korri:fex/<key>`. A runner that lists `@korri:fex/rootfs` in its
// `artifacts` receives the ready rootfs directory in `input.artifacts`.
export const name = "fex"
export const title = "FEX"
export const description =
  "FEX x86 emulation runtime: its configuration and the pinned x86 rootfs."

// The pinned FEX x86 rootfs: the ArchLinux 2026-01-08 squashfs, then the ten
// Arch Mesa 1:26.1.2-1 overlay packages in extraction order. korrid downloads
// and verifies every file, then runs `fex-unpack` with them in this order.
// `bytes` is each server's Content-Length, read on 2026-10-19.
export const artifacts = {
  rootfs: {
    id: "@korri:fex/rootfs",
    version: "archlinux-2026-01-08-mesa-26.1.2",
    files: [
      {
        url: "https://rootfs.fex-emu.gg/ArchLinux/2026-01-08/ArchLinux.sqsh",
        sha256: "sha256:cb059973b7953ad9165845529655189b96f9a174b14a6a149c87ec884b0c5e90",
        bytes: 1158209536,
      },
      {
        url: "https://archive.archlinux.org/packages/m/mesa/mesa-1%3A26.1.2-1-x86_64.pkg.tar.zst",
        sha256: "sha256:be9cdb7a2a1c4eb096f57328c955b018950946edfdbb7f04ef697d4999e5a808",
        bytes: 13845889,
      },
      {
        url: "https://archive.archlinux.org/packages/l/lib32-mesa/lib32-mesa-1%3A26.1.2-1-x86_64.pkg.tar.zst",
        sha256: "sha256:a32b6c7f956ac6ddff53126d9717643d6edf5215d4279b7a84f72a5908a42c71",
        bytes: 13752793,
      },
      {
        url: "https://archive.archlinux.org/packages/v/vulkan-freedreno/vulkan-freedreno-1%3A26.1.2-1-x86_64.pkg.tar.zst",
        sha256: "sha256:7db14d6e8e6926b31de09716dfd03451c22e348da8b1ea35f5438fbb9a618dee",
        bytes: 2248347,
      },
      {
        url: "https://archive.archlinux.org/packages/l/lib32-vulkan-freedreno/lib32-vulkan-freedreno-1%3A26.1.2-1-x86_64.pkg.tar.zst",
        sha256: "sha256:98793dac11cefb40b43f4e9007d0c183b523dbf6c340020d4102b41228958a6f",
        bytes: 2273347,
      },
      {
        url: "https://archive.archlinux.org/packages/v/vulkan-mesa-implicit-layers/vulkan-mesa-implicit-layers-1%3A26.1.2-1-x86_64.pkg.tar.zst",
        sha256: "sha256:e708f001c78a220e87eeaf42df022664df6ced4963bf61d4a22528d21fe61292",
        bytes: 56508,
      },
      {
        url: "https://archive.archlinux.org/packages/l/lib32-vulkan-mesa-implicit-layers/lib32-vulkan-mesa-implicit-layers-1%3A26.1.2-1-x86_64.pkg.tar.zst",
        sha256: "sha256:a9e2ca9c2df8fa5897d3baeadb298cd2969a229539119c05ff436550246915b7",
        bytes: 54830,
      },
      {
        url: "https://archive.archlinux.org/packages/l/libdisplay-info/libdisplay-info-0.3.0-1-x86_64.pkg.tar.zst",
        sha256: "sha256:07ab1479ec5e70aea174e2d0e340bd9364b507ed74f2df3bfd51b3c6f40e5787",
        bytes: 124625,
      },
      {
        url: "https://archive.archlinux.org/packages/l/lib32-libdisplay-info/lib32-libdisplay-info-0.3.0-1-x86_64.pkg.tar.zst",
        sha256: "sha256:5ffcb2f1e599529014beaadff257ee805ecdf3991948e33ebf398e202ce135f0",
        bytes: 80659,
      },
      {
        url: "https://archive.archlinux.org/packages/x/xcb-util-keysyms/xcb-util-keysyms-0.4.1-5-x86_64.pkg.tar.zst",
        sha256: "sha256:312213911881c0742e2a825432feac761e863945b74bb3a6daca97a99778d35e",
        bytes: 7729,
      },
      {
        url: "https://archive.archlinux.org/packages/l/lib32-xcb-util-keysyms/lib32-xcb-util-keysyms-0.4.1-2-x86_64.pkg.tar.zst",
        sha256: "sha256:62940da026cc3ca714d6989a542edfe3aaf7ec8b5c148fc9007e2e3b9ec3f5a6",
        bytes: 6167,
      },
    ],
    unpack: "fex-unpack",
  },
}
