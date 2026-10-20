// Proton compatibility tools for runners that start Windows content. This
// plugin declares no runner and no handler. Its native files are declared in
// plugin.nix. A runner that lists `@korri:proton/cachyos` in its `artifacts`
// receives the ready tool directory in `input.artifacts`.
export const name = "proton"
export const title = "Proton"
export const description =
  "Proton compatibility tools: the pinned Proton-CachyOS arm64 build."

// The pinned Proton-CachyOS arm64 release. korrid downloads and verifies it,
// then runs `cachyos-unpack` on it. `bytes` is the server's Content-Length,
// read on 2026-10-19.
export const artifacts = {
  cachyos: {
    id: "@korri:proton/cachyos",
    // Build metadata derives Steam's tool name from this artifact version.
    version: "11.0-20260601-slr-arm64",
    files: [
      {
        url: "https://github.com/CachyOS/proton-cachyos/releases/download/cachyos-11.0-20260601-slr/proton-cachyos-11.0-20260601-slr-arm64.tar.xz",
        sha256: "sha256:6793a697c8153626a9c01fcd212aae972bd1596a1d13d482620f64abf5fc69d9",
        bytes: 335847508,
      },
    ],
    unpack: "cachyos-unpack",
  },
}
