// FEX runtime files for runners that emulate x86 code. This plugin declares
// no runner and no handler: a runner that requires it reads the files below
// from its launch input under `@korri:fex/<key>` and runs them itself.
//   fex-prepare  prepares the pinned x86 rootfs in korrid's cache root
//   config       the FEX configuration directory
//   fex          the FEX launcher
export const name = "fex"
export const title = "FEX"
export const description =
  "FEX x86 emulation runtime: the launcher, its configuration and a pinned rootfs preparer."
