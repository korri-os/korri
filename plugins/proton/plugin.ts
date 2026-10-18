// Proton compatibility tools for runners that start Windows content. This
// plugin declares no runner and no handler: a runner that requires it reads
// the files below from its launch input under `@korri:proton/<key>`.
//   cachyos-prepare            prepares the pinned Proton-CachyOS arm64 build
//                              in korrid's cache root
//   cachyos-compatibilitytool  that build's Steam compatibilitytool.vdf
export const name = "proton"
export const title = "Proton"
export const description =
  "Proton compatibility tools: a preparer for the pinned Proton-CachyOS arm64 build."
