// Android platform declaration. The source remains bundled with the shell;
// Linux uses the independently installed plugin in the parent directory.
export const name = "retroarch"
export const title = "RetroArch"
export const description =
  "Owns the RetroArch launcher on every supported platform."
export const launchers = {
  retroarch: {
    id: "@korri:retroarch/retroarch",
    plugin: "@korri:retroarch",
    command: "retroarch",
    android: {
      packageName: "com.korri.retroarch",
      className: "com.retroarch.browser.retroactivity.RetroActivityFuture",
    },
  },
}
export const sessionControls = {
  openMenu: {
    order: 0,
    id: "@korri:retroarch/open-menu",
    owner: { kind: "launcher", id: "@korri:retroarch/retroarch" },
    label: "Open RetroArch menu",
    interaction: { kind: "command" },
    effect: "@korri:retroarch/open-menu",
    dismissOnSuccess: true,
  },
  quit: {
    order: 1,
    id: "@korri:retroarch/quit",
    owner: { kind: "launcher", id: "@korri:retroarch/retroarch" },
    label: "Quit game",
    interaction: { kind: "command" },
    effect: "@korri:retroarch/quit",
    destructive: true,
    dismissOnSuccess: true,
  },
}
