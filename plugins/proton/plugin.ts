// Proton compatibility tools for runners that start Windows content. This
// plugin declares no runner and no handler. Its native artifacts are declared
// in plugin.nix; a runner that requires this plugin reads them from its launch
// input under `@korri:proton/<key>`.
export const name = "proton"
export const title = "Proton"
export const description =
  "Proton compatibility tools: a preparer for the pinned Proton-CachyOS arm64 build."
