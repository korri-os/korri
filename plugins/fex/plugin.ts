// FEX runtime files for runners that emulate x86 code. This plugin declares
// no runner and no handler. Its native artifacts are declared in plugin.nix;
// a runner that requires this plugin reads them from its launch input under
// `@korri:fex/<key>`.
export const name = "fex"
export const title = "FEX"
export const description =
  "FEX x86 emulation runtime: its configuration and a pinned rootfs preparer."
