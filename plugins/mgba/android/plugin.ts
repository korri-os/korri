export const name = "mgba"
export const title = "mGBA"
export const description =
  "Runs Game Boy Advance content with the mGBA libretro core."
export const systems = {
  gba: { id: "gba", title: "Game Boy Advance" },
}
export const runners = {
  mgba: {
    id: "@korri:mgba/mgba",
    family: "@korri:retroarch",
    command: "retroarch",
    systems: ["gba"],
    core: "/data/data/com.korri.retroarch/cores/mgba_libretro_android.so",
    android: {
      packageName: "com.korri.retroarch",
      className: "com.retroarch.browser.retroactivity.RetroActivityFuture",
    },
  },
}
export const sessionControls = {
  openMenu: {
    order: 0,
    id: "@korri:mgba/open-menu",
    owner: { kind: "runner", id: "@korri:mgba/mgba" },
    label: "Open RetroArch menu",
    interaction: { kind: "command" },
    effect: "@korri:mgba/open-menu",
    dismissOnSuccess: true,
  },
  quit: {
    order: 1,
    id: "@korri:mgba/quit",
    owner: { kind: "runner", id: "@korri:mgba/mgba" },
    label: "Quit game",
    interaction: { kind: "command" },
    effect: "@korri:mgba/quit",
    destructive: true,
    dismissOnSuccess: true,
  },
}
export const discovery = {
  fileReleases: {
    "gba-files": {
      id: "@korri:mgba/gba-files",
      title: "Game Boy Advance ROM files",
      extensions: ["gba"],
      system: "gba",
      runners: ["@korri:mgba/mgba"],
    },
  },
}
